use std::{
    borrow::Cow,
    io,
    io::IsTerminal,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use ash::{
    agent::ProviderAgent,
    codex_native::CodexSubscriptionProvider,
    config::AshConfig,
    context::SqliteContextStore,
    error::AshError,
    providers::{AnyProvider, CodexProvider, UnimplementedProvider},
    session::{AshSession, PromptMode, SessionResponse},
    setup::{
        AshrcEditor, ProviderSetup, default_base_url_for_kind, default_env_for_kind,
        display_provider, doctor_lines,
    },
    ui::TerminalRenderer,
};
use clap::{Args, Parser, Subcommand};
use crossterm::{
    event::{
        self as terminal_event, Event as TerminalEvent, KeyCode as TerminalKeyCode, KeyEvent,
        KeyEventKind, KeyModifiers as TerminalKeyModifiers,
    },
    terminal,
};
use reedline::{
    EditCommand, Emacs, KeyCode as ReedlineKeyCode, KeyModifiers as ReedlineKeyModifiers, Prompt,
    PromptEditMode, PromptHistorySearch, Reedline, ReedlineEvent, Signal, Span, Suggestion,
    default_emacs_keybindings,
};

const TOGGLE_MODE_COMMAND: &str = "ash.toggle-mode";

#[derive(Debug, Parser)]
#[command(name = "ash", about = "Agentic Shell")]
struct Cli {
    #[arg(long, value_name = "PATH", help = "Load this ASH startup file")]
    ashrc: Option<PathBuf>,

    #[arg(long, help = "Skip loading .ashrc")]
    no_ashrc: bool,

    #[arg(long, value_name = "PATH", help = "SQLite context database path")]
    context_db: Option<PathBuf>,

    #[arg(long, value_name = "LINE", help = "Evaluate one input line and exit")]
    eval: Option<String>,

    #[arg(long, value_parser = parse_prompt_mode, help = "Override initial prompt mode")]
    mode: Option<PromptMode>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Configure provider authentication")]
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },
    #[command(about = "Manage AI providers")]
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
}

#[derive(Debug, Subcommand)]
enum AuthCommands {
    #[command(about = "Authenticate with an OpenAI Codex subscription")]
    Codex(AuthCodexArgs),
}

#[derive(Debug, Args)]
struct AuthCodexArgs {
    #[arg(long, help = "Print the auth command without running it")]
    dry_run: bool,
}

#[derive(Debug, Subcommand)]
enum ProviderCommands {
    #[command(about = "Add or update a provider in .ashrc")]
    Add(ProviderAddArgs),
    #[command(about = "Set the default provider in .ashrc")]
    Default { name: String },
    #[command(about = "List configured providers")]
    List,
    #[command(about = "Diagnose configured providers")]
    Doctor,
}

#[derive(Debug, Args)]
struct ProviderAddArgs {
    #[arg(value_name = "KIND", help = "Provider kind, such as codex or openai")]
    kind: String,

    #[arg(long, help = "Provider name; defaults to the kind")]
    name: Option<String>,

    #[arg(
        long,
        value_name = "ENV",
        help = "Environment variable containing the API key"
    )]
    env: Option<String>,

    #[arg(long, value_name = "URL", help = "Provider base URL")]
    base_url: Option<String>,

    #[arg(long, value_name = "MODEL", help = "Default model for this provider")]
    model: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let default_ashrc = default_ashrc_path();
    if let Some(command) = &cli.command {
        return handle_cli_command(&cli, command, default_ashrc.as_deref());
    }

    let config_path = if cli.no_ashrc {
        None
    } else {
        cli.ashrc.as_deref().or(default_ashrc.as_deref())
    };
    let mut config = AshConfig::load(config_path).context("failed to load ASH configuration")?;

    if let Some(mode) = cli.mode {
        config.default_mode = mode;
    }

    let context_path = cli.context_db.unwrap_or_else(default_context_db_path);
    let context = SqliteContextStore::open(context_path).context("failed to open context store")?;
    let provider = CodexSubscriptionProvider::discover().map_or_else(
        |_| {
            CodexProvider::discover().map_or_else(
                |_| AnyProvider::Unimplemented(UnimplementedProvider::new("codex")),
                AnyProvider::Codex,
            )
        },
        AnyProvider::CodexSubscription,
    );
    let agent = ProviderAgent::new(provider);
    let cwd = std::env::current_dir().context("failed to determine current directory")?;
    let mut session = AshSession::new(config, context, agent, cwd);

    if let Some(line) = cli.eval {
        let response = session.handle_line(&line)?;
        render_response(&response);
        return Ok(());
    }

    run_interactive(&mut session)
}

fn handle_cli_command(
    cli: &Cli,
    command: &Commands,
    default_ashrc: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    match command {
        Commands::Auth { command } => handle_auth_command(command),
        Commands::Provider { command } => {
            let path = cli
                .ashrc
                .as_deref()
                .or(default_ashrc)
                .context("no .ashrc path available; pass --ashrc or set HOME")?;
            let editor = AshrcEditor::new(path);
            handle_provider_command(&editor, command)
        }
    }
}

fn handle_auth_command(command: &AuthCommands) -> anyhow::Result<()> {
    match command {
        AuthCommands::Codex(args) => {
            if args.dry_run {
                println!("codex login");
                return Ok(());
            }

            let provider = CodexProvider::discover().context(
                "failed to find Codex; install the Codex CLI or open the Codex app first",
            )?;
            let status = Command::new(provider.executable())
                .arg("login")
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .with_context(|| {
                    format!("failed to run `{}` login", provider.executable().display())
                })?;

            if status.success() {
                Ok(())
            } else {
                anyhow::bail!("`codex login` exited with {status}");
            }
        }
    }
}

fn handle_provider_command(editor: &AshrcEditor, command: &ProviderCommands) -> anyhow::Result<()> {
    match command {
        ProviderCommands::Add(args) => {
            let name = args.name.clone().unwrap_or_else(|| args.kind.clone());
            let setup = ProviderSetup {
                name: name.clone(),
                kind: args.kind.clone(),
                env: args
                    .env
                    .clone()
                    .or_else(|| default_env_for_kind(&args.kind)),
                base_url: args
                    .base_url
                    .clone()
                    .or_else(|| default_base_url_for_kind(&args.kind)),
                model: args.model.clone(),
            };
            let provider = setup.into_config();
            editor.add_provider(&provider)?;
            println!("Added provider {name}");
            Ok(())
        }
        ProviderCommands::Default { name } => {
            editor.set_default_provider(name)?;
            println!("Default provider set to {name}");
            Ok(())
        }
        ProviderCommands::List => {
            let config = editor.load_config()?;
            for provider in config.providers.values() {
                println!(
                    "{}",
                    display_provider(provider, config.default_provider.as_str())
                );
            }
            Ok(())
        }
        ProviderCommands::Doctor => {
            let config = editor.load_config()?;
            for line in doctor_lines(&config) {
                println!("{line}");
            }
            Ok(())
        }
    }
}

fn parse_prompt_mode(value: &str) -> Result<PromptMode, String> {
    PromptMode::parse(value).map_err(|err| err.to_string())
}

fn run_interactive<S, A>(session: &mut AshSession<S, A>) -> anyhow::Result<()>
where
    S: ash::context::ContextStore,
    A: ash::agent::Agent,
{
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        return run_reedline_interactive(session);
    }

    run_plain_interactive(session)
}

fn run_plain_interactive<S, A>(session: &mut AshSession<S, A>) -> anyhow::Result<()>
where
    S: ash::context::ContextStore,
    A: ash::agent::Agent,
{
    let stdin = io::stdin();
    let mut renderer = TerminalRenderer::new(io::stdout());

    loop {
        renderer.render_prompt(&session.prompt())?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }

        let input = line.trim_end_matches(['\r', '\n']);
        let streams_agent_response =
            session.mode() == PromptMode::Agent && !input.is_empty() && input != "\t";

        let response = if streams_agent_response {
            renderer.begin_agent_response(&session.status_line())?;
            let response = session.handle_line_stream(&line, |event| {
                renderer
                    .stream_agent_event(&event)
                    .map_err(ash::AshError::from)
            });
            renderer.end_agent_response()?;
            response?
        } else {
            let response = session.handle_line(&line)?;
            renderer.render_response(&response)?;
            response
        };

        let should_exit = matches!(
            &response,
            SessionResponse::Command(result) if result.should_exit
        );

        if should_exit {
            break;
        }
    }

    Ok(())
}

fn run_reedline_interactive<S, A>(session: &mut AshSession<S, A>) -> anyhow::Result<()>
where
    S: ash::context::ContextStore,
    A: ash::agent::Agent,
{
    let mode = Arc::new(Mutex::new(session.mode()));
    let status = Arc::new(Mutex::new(session.prompt_status_line()));
    let mut line_editor = ash_line_editor();
    let prompt = AshPrompt::new(Arc::clone(&mode), Arc::clone(&status));
    let commands = discover_shell_commands();
    let mut renderer = TerminalRenderer::new(io::stdout());
    let mut next_buffer = None;

    loop {
        set_shared_mode(&mode, session.mode());
        set_shared_status(&status, session.prompt_status_line());
        restore_reedline_buffer(&mut line_editor, &mut next_buffer);
        match line_editor.read_line(&prompt)? {
            Signal::Success(line) => {
                let streams_agent_response =
                    session.mode() == PromptMode::Agent && !line.is_empty();
                let response = if streams_agent_response {
                    let _raw_mode = RawModeGuard::enable()?;
                    renderer.set_raw_mode_line_endings(true);
                    renderer.begin_agent_response(&session.status_line())?;
                    let response = session.handle_line_stream(&line, |event| {
                        poll_agent_escape(&mut renderer)?;
                        renderer
                            .stream_agent_event(&event)
                            .map_err(AshError::from)?;
                        poll_agent_escape(&mut renderer)
                    });
                    let cancelled = matches!(response, Err(AshError::AgentCancelled));
                    if cancelled {
                        renderer.render_agent_cancelled(&line)?;
                        next_buffer = Some(line.clone());
                    }
                    renderer.end_agent_response()?;
                    renderer.set_raw_mode_line_endings(false);
                    match response {
                        Ok(response) => response,
                        Err(AshError::AgentCancelled) => continue,
                        Err(error) => return Err(error.into()),
                    }
                } else {
                    let response = if session.mode() == PromptMode::Command {
                        session.handle_line_interactive(&line)?
                    } else {
                        session.handle_line(&line)?
                    };
                    renderer.render_response(&response)?;
                    response
                };

                if matches!(
                    &response,
                    SessionResponse::Command(result) if result.should_exit
                ) {
                    break;
                }
            }
            Signal::HostCommand(command) if command == TOGGLE_MODE_COMMAND => {
                if line_editor.current_buffer_contents().trim().is_empty() {
                    let _response = session.toggle_mode()?;
                    set_shared_mode(&mode, session.mode());
                } else if session.mode() == PromptMode::Command {
                    complete_reedline_buffer(&mut line_editor, &commands);
                }
            }
            Signal::CtrlD | Signal::CtrlC => break,
            _ => {}
        }
    }

    Ok(())
}

fn ash_line_editor() -> Reedline {
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        ReedlineKeyModifiers::NONE,
        ReedlineKeyCode::Tab,
        ReedlineEvent::ExecuteHostCommand(TOGGLE_MODE_COMMAND.to_owned()),
    );

    Reedline::create().with_edit_mode(Box::new(Emacs::new(keybindings)))
}

fn restore_reedline_buffer(line_editor: &mut Reedline, next_buffer: &mut Option<String>) {
    let Some(buffer) = next_buffer.take() else {
        return;
    };

    line_editor.run_edit_commands(&[EditCommand::Clear, EditCommand::InsertString(buffer)]);
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> anyhow::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn poll_agent_escape<W>(renderer: &mut TerminalRenderer<W>) -> ash::error::Result<()>
where
    W: io::Write,
{
    while terminal_event::poll(Duration::from_millis(0))? {
        let TerminalEvent::Key(key) = terminal_event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press || key.code != TerminalKeyCode::Esc {
            continue;
        }

        renderer.render_cancel_prompt()?;
        if read_escape_confirmation()? {
            return Err(AshError::AgentCancelled);
        }
    }

    Ok(())
}

fn read_escape_confirmation() -> ash::error::Result<bool> {
    loop {
        if let TerminalEvent::Key(key) = terminal_event::read()?
            && key.kind == KeyEventKind::Press
            && let Some(cancelled) = escape_confirmation_decision(key)
        {
            return Ok(cancelled);
        }
    }
}

fn escape_confirmation_decision(key: KeyEvent) -> Option<bool> {
    match key.code {
        TerminalKeyCode::Esc | TerminalKeyCode::Char('y' | 'Y') => Some(true),
        TerminalKeyCode::Char('c' | 'C')
            if key.modifiers.contains(TerminalKeyModifiers::CONTROL) =>
        {
            Some(true)
        }
        TerminalKeyCode::Enter | TerminalKeyCode::Char('n' | 'N') => Some(false),
        _ => None,
    }
}

fn set_shared_mode(mode: &Arc<Mutex<PromptMode>>, next: PromptMode) {
    if let Ok(mut mode) = mode.lock() {
        *mode = next;
    }
}

fn set_shared_status(status: &Arc<Mutex<String>>, next: String) {
    if let Ok(mut status) = status.lock() {
        *status = next;
    }
}

fn shared_mode(mode: &Arc<Mutex<PromptMode>>) -> PromptMode {
    mode.lock().map_or(PromptMode::Agent, |mode| *mode)
}

fn shared_status(status: &Arc<Mutex<String>>) -> String {
    status
        .lock()
        .map_or_else(|_| String::new(), |status| status.clone())
}

struct AshPrompt {
    mode: Arc<Mutex<PromptMode>>,
    status: Arc<Mutex<String>>,
}

impl AshPrompt {
    const fn new(mode: Arc<Mutex<PromptMode>>, status: Arc<Mutex<String>>) -> Self {
        Self { mode, status }
    }
}

impl Prompt for AshPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}\n", shared_status(&self.status)))
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _prompt_mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Owned(format!("{} ", shared_mode(&self.mode).prompt()))
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed(".. ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        _history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Borrowed("? ")
    }
}

fn complete_reedline_buffer(line_editor: &mut Reedline, commands: &[String]) {
    let buffer = line_editor.current_buffer_contents().to_owned();
    let pos = line_editor.current_insertion_point();
    let suggestions = shell_suggestions(&buffer, pos, commands);
    let Some(replacement) = completion_replacement(&buffer, pos, &suggestions) else {
        return;
    };

    let Some(suggestion) = suggestions.first() else {
        return;
    };
    let mut completed = String::new();
    completed.push_str(&buffer[..suggestion.span.start]);
    completed.push_str(&replacement);
    completed.push_str(&buffer[suggestion.span.end..]);
    if completed != buffer {
        line_editor.run_edit_commands(&[EditCommand::Clear, EditCommand::InsertString(completed)]);
    }
}

fn shell_suggestions(line: &str, pos: usize, commands: &[String]) -> Vec<Suggestion> {
    let prefix = &line[..pos.min(line.len())];
    if prefix.trim().is_empty() {
        return Vec::new();
    }

    let (start, token) = current_token(prefix);
    if is_first_token(prefix) && !token.contains('/') {
        commands
            .iter()
            .filter(|command| command.starts_with(token) && command.as_str() != token)
            .take(80)
            .map(|command| suggestion(command.clone(), start, pos, true))
            .collect::<Vec<_>>()
    } else {
        path_suggestions(token, start, pos)
    }
}

fn completion_replacement(line: &str, pos: usize, suggestions: &[Suggestion]) -> Option<String> {
    let first = suggestions.first()?;
    if suggestions.len() == 1 {
        return Some(first.value.clone());
    }

    let common = common_prefix(
        suggestions
            .iter()
            .map(|suggestion| suggestion.value.as_str()),
    );
    let current = &line[first.span.start..pos.min(line.len())];
    (common.len() > current.len()).then_some(common)
}

fn common_prefix<'a>(mut values: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = values.next() else {
        return String::new();
    };
    let mut prefix = first.to_owned();
    for value in values {
        while !value.starts_with(&prefix) {
            if prefix.is_empty() {
                return prefix;
            }
            prefix.pop();
        }
    }
    prefix
}

fn current_token(prefix: &str) -> (usize, &str) {
    let start = prefix
        .char_indices()
        .rev()
        .find_map(|(index, character)| character.is_whitespace().then_some(index + 1))
        .unwrap_or(0);
    (start, &prefix[start..])
}

fn is_first_token(prefix: &str) -> bool {
    prefix.split_whitespace().count() <= 1
}

fn path_suggestions(token: &str, start: usize, pos: usize) -> Vec<Suggestion> {
    let path = std::path::Path::new(token);
    let (directory, file_prefix) = if token.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new(".")),
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(""),
        )
    };

    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };

    let prefix_path = if directory == std::path::Path::new(".") {
        String::new()
    } else {
        format!("{}/", directory.display())
    };

    let mut suggestions = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(file_prefix) {
                return None;
            }
            let is_dir = entry.file_type().ok().is_some_and(|kind| kind.is_dir());
            let value = format!("{}{}{}", prefix_path, name, if is_dir { "/" } else { "" });
            Some(suggestion(value, start, pos, false))
        })
        .take(80)
        .collect::<Vec<_>>();
    suggestions.sort_by(|left, right| left.value.cmp(&right.value));
    suggestions
}

fn suggestion(value: String, start: usize, end: usize, append_whitespace: bool) -> Suggestion {
    Suggestion {
        value,
        span: Span::new(start, end),
        append_whitespace,
        ..Suggestion::default()
    }
}

fn discover_shell_commands() -> Vec<String> {
    let mut commands = vec![
        "cd".to_owned(),
        "exit".to_owned(),
        "pwd".to_owned(),
        "jobs".to_owned(),
        "fg".to_owned(),
        "bg".to_owned(),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            commands.extend(
                entries
                    .filter_map(std::result::Result::ok)
                    .filter_map(|entry| {
                        let file_type = entry.file_type().ok()?;
                        file_type
                            .is_file()
                            .then(|| entry.file_name().to_string_lossy().into_owned())
                    }),
            );
        }
    }
    commands.sort();
    commands.dedup();
    commands
}

fn render_response(response: &SessionResponse) {
    match response {
        SessionResponse::Agent(text) => {
            println!("{text}");
        }
        SessionResponse::Command(result) => {
            print!("{}", result.stdout);
            eprint!("{}", result.stderr);
        }
        SessionResponse::ModeChanged(mode) => {
            eprintln!("[ash] mode {}", mode.prompt());
        }
        SessionResponse::Empty => {}
    }
}

fn default_ashrc_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ashrc"))
}

fn default_context_db_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local/share/ash/context.db")
    } else {
        PathBuf::from(".ash-context.db")
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        KeyCode as TerminalKeyCode, KeyEvent, KeyModifiers as TerminalKeyModifiers,
    };

    use reedline::{Prompt, PromptEditMode};

    use super::{
        AshPrompt, PromptMode, completion_replacement, escape_confirmation_decision,
        shell_suggestions,
    };

    #[test]
    fn command_completion_replaces_the_current_command_token() {
        let commands = vec!["pwd".to_owned()];
        let suggestions = shell_suggestions("pw", 2, &commands);

        assert_eq!(
            completion_replacement("pw", 2, &suggestions),
            Some("pwd".to_owned())
        );
    }

    #[test]
    fn command_completion_uses_common_prefix_for_multiple_matches() {
        let commands = vec!["git".to_owned(), "gitk".to_owned()];
        let suggestions = shell_suggestions("gi", 2, &commands);

        assert_eq!(
            completion_replacement("gi", 2, &suggestions),
            Some("git".to_owned())
        );
    }

    #[test]
    fn empty_command_line_has_no_completion_so_tab_can_toggle_modes() {
        let commands = vec!["pwd".to_owned()];

        assert!(shell_suggestions("", 0, &commands).is_empty());
    }

    #[test]
    fn escape_confirmation_requires_explicit_cancel_key() {
        assert_eq!(
            escape_confirmation_decision(KeyEvent::new(
                TerminalKeyCode::Esc,
                TerminalKeyModifiers::NONE,
            )),
            Some(true)
        );
        assert_eq!(
            escape_confirmation_decision(KeyEvent::new(
                TerminalKeyCode::Char('y'),
                TerminalKeyModifiers::NONE,
            )),
            Some(true)
        );
        assert_eq!(
            escape_confirmation_decision(KeyEvent::new(
                TerminalKeyCode::Enter,
                TerminalKeyModifiers::NONE,
            )),
            Some(false)
        );
        assert_eq!(
            escape_confirmation_decision(KeyEvent::new(
                TerminalKeyCode::Char('n'),
                TerminalKeyModifiers::NONE,
            )),
            Some(false)
        );
        assert_eq!(
            escape_confirmation_decision(KeyEvent::new(
                TerminalKeyCode::Char('x'),
                TerminalKeyModifiers::NONE,
            )),
            None
        );
        assert_eq!(
            escape_confirmation_decision(KeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::CONTROL,
            )),
            Some(true)
        );
    }

    #[test]
    fn escape_confirmation_ignores_plain_control_keys() {
        assert_eq!(
            escape_confirmation_decision(KeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::NONE,
            )),
            None
        );
    }

    #[test]
    fn ash_prompt_renders_statusline_above_input_indicator() {
        let prompt = AshPrompt::new(
            std::sync::Arc::new(std::sync::Mutex::new(PromptMode::Agent)),
            std::sync::Arc::new(std::sync::Mutex::new("status".to_owned())),
        );

        assert_eq!(prompt.render_prompt_left(), "status\n");
        assert_eq!(prompt.render_prompt_right(), "");
        assert_eq!(
            prompt.render_prompt_indicator(PromptEditMode::Default),
            "> "
        );
    }
}
