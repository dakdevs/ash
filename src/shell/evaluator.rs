use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    error::{AshError, Result},
    shell::{Expander, Parser, ShellProgram, SimpleCommand},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub should_exit: bool,
}

impl ExecutionResult {
    #[must_use]
    pub const fn success(stdout: String) -> Self {
        Self {
            status: 0,
            stdout,
            stderr: String::new(),
            should_exit: false,
        }
    }

    #[must_use]
    pub const fn failure(status: i32, stderr: String) -> Self {
        Self {
            status,
            stdout: String::new(),
            stderr,
            should_exit: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShellExecutor {
    cwd: PathBuf,
    environment: HashMap<String, String>,
}

impl ShellExecutor {
    #[must_use]
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            environment: std::env::vars().collect(),
        }
    }

    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn execute_line(&mut self, input: &str) -> Result<ExecutionResult> {
        let raw_program = Parser::parse(input)?;
        let program = self.expand(raw_program);
        self.execute_program(program)
    }

    fn expand(&self, program: ShellProgram) -> ShellProgram {
        match program {
            ShellProgram::Simple(command) => {
                let words = Expander::new(self.environment.clone()).expand_words(command.words);
                ShellProgram::Simple(SimpleCommand::new(
                    command.assignments,
                    words
                        .into_iter()
                        .map(|text| crate::shell::LexedWord {
                            text,
                            expand_env: false,
                        })
                        .collect(),
                ))
            }
        }
    }

    fn execute_program(&mut self, program: ShellProgram) -> Result<ExecutionResult> {
        match program {
            ShellProgram::Simple(command) => self.execute_simple(command),
        }
    }

    fn execute_simple(&mut self, command: SimpleCommand) -> Result<ExecutionResult> {
        if command.words.is_empty() {
            for (key, value) in command.assignments {
                self.environment.insert(key, value);
            }
            return Ok(ExecutionResult::success(String::new()));
        }

        let words = command
            .words
            .iter()
            .map(|word| word.text.clone())
            .collect::<Vec<_>>();

        match words[0].as_str() {
            "cd" => self.change_directory(words.get(1)),
            "pwd" => Ok(ExecutionResult::success(format!(
                "{}\n",
                self.cwd.display()
            ))),
            "export" => Ok(self.export(&words[1..])),
            "exit" => Ok(ExecutionResult {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
                should_exit: true,
            }),
            "ash" if words.get(1).is_some_and(|word| word == "status") => {
                Ok(ExecutionResult::success(format!(
                    "cwd={}\nvars={}\n",
                    self.cwd.display(),
                    self.environment.len()
                )))
            }
            _ => self.execute_external(command),
        }
    }

    fn change_directory(&mut self, target: Option<&String>) -> Result<ExecutionResult> {
        let target = target
            .cloned()
            .or_else(|| self.environment.get("HOME").cloned())
            .unwrap_or_else(|| "/".to_owned());
        let path = if Path::new(&target).is_absolute() {
            PathBuf::from(&target)
        } else {
            self.cwd.join(&target)
        };
        let canonical = path
            .canonicalize()
            .map_err(|source| AshError::ChangeDirectory {
                path: path.clone(),
                source,
            })?;

        self.cwd.clone_from(&canonical);
        self.environment
            .insert("PWD".to_owned(), canonical.display().to_string());
        Ok(ExecutionResult::success(String::new()))
    }

    fn export(&mut self, args: &[String]) -> ExecutionResult {
        if args.is_empty() {
            let mut variables = self
                .environment
                .iter()
                .map(|(key, value)| format!("export {key}={value}\n"))
                .collect::<Vec<_>>();
            variables.sort();
            return ExecutionResult::success(variables.concat());
        }

        for arg in args {
            if let Some((key, value)) = arg.split_once('=') {
                self.environment.insert(key.to_owned(), value.to_owned());
            }
        }

        ExecutionResult::success(String::new())
    }

    fn execute_external(&self, command: SimpleCommand) -> Result<ExecutionResult> {
        let words = command
            .words
            .iter()
            .map(|word| word.text.clone())
            .collect::<Vec<_>>();
        let program = &words[0];
        let mut process = Command::new(program);
        process
            .args(&words[1..])
            .current_dir(&self.cwd)
            .envs(&self.environment)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in command.assignments {
            process.env(key, value);
        }

        let output = process.output().map_err(|source| AshError::ProcessSpawn {
            program: program.clone(),
            source,
        })?;

        Ok(ExecutionResult {
            status: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            should_exit: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::ShellExecutor;

    #[test]
    fn executes_pwd_builtin() {
        let cwd = std::env::current_dir().expect("cwd");
        let mut executor = ShellExecutor::new(cwd.clone());

        let result = executor.execute_line("pwd").expect("pwd");

        assert_eq!(result.status, 0);
        assert_eq!(result.stdout, format!("{}\n", cwd.display()));
    }

    #[test]
    fn changes_directory_without_spawning_shell() {
        let dir = tempdir().expect("tempdir");
        let mut executor = ShellExecutor::new(std::env::current_dir().expect("cwd"));

        executor
            .execute_line(&format!("cd {}", dir.path().display()))
            .expect("cd");

        assert_eq!(
            executor.cwd(),
            dir.path().canonicalize().expect("canonical tempdir")
        );
    }

    #[test]
    fn executes_external_command_directly() {
        let mut executor = ShellExecutor::new(std::env::current_dir().expect("cwd"));

        let result = executor.execute_line("printf hello").expect("printf");

        assert_eq!(result.status, 0);
        assert_eq!(result.stdout, "hello");
    }

    #[test]
    fn does_not_expand_single_quoted_variables() {
        let mut executor = ShellExecutor::new(std::env::current_dir().expect("cwd"));
        executor.execute_line("export NAME=ash").expect("export");

        let result = executor
            .execute_line("printf '$NAME'")
            .expect("single quoted printf");

        assert_eq!(result.stdout, "$NAME");
    }
}
