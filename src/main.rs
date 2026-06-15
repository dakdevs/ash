use std::{
    io,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::Context;
use ash::{
    agent::ProviderAgent,
    config::AshConfig,
    context::SqliteContextStore,
    providers::{AnyProvider, CodexProvider, UnimplementedProvider},
    session::{AshSession, PromptMode, SessionResponse},
    setup::{
        AshrcEditor, ProviderSetup, default_base_url_for_kind, default_env_for_kind,
        display_provider, doctor_lines,
    },
    ui::TerminalRenderer,
};
use clap::{Args, Parser, Subcommand};

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
            let response = session.handle_line_stream(&line, |chunk| {
                renderer
                    .stream_agent_chunk(chunk)
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
