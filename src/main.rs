use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Context;
use ash::{
    agent::ProviderAgent,
    config::AshConfig,
    context::SqliteContextStore,
    providers::{AnyProvider, CodexProvider, UnimplementedProvider},
    session::{AshSession, PromptMode, SessionResponse},
};
use clap::Parser;

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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let default_ashrc = default_ashrc_path();
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
    let provider = CodexProvider::discover().map_or_else(
        |_| AnyProvider::Unimplemented(UnimplementedProvider::new("codex")),
        AnyProvider::Codex,
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

fn parse_prompt_mode(value: &str) -> Result<PromptMode, String> {
    PromptMode::parse(value).map_err(|err| err.to_string())
}

fn run_interactive<S, A>(session: &mut AshSession<S, A>) -> anyhow::Result<()>
where
    S: ash::context::ContextStore,
    A: ash::agent::Agent,
{
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        write!(stdout, "{}", session.prompt())?;
        stdout.flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }

        let response = session.handle_line(&line)?;
        let should_exit = matches!(
            &response,
            SessionResponse::Command(result) if result.should_exit
        );
        render_response(&response);

        if should_exit {
            break;
        }
    }

    Ok(())
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
