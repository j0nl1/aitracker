mod cli;
mod core;

use clap::{Parser, Subcommand};
use skillinstaller::rust_embed;
use skillinstaller::{
    InstallSkillArgs, install_interactive, load_embedded_skill, print_install_result,
};

#[derive(Parser)]
#[command(name = "ait", about = "AI provider usage tracking CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Output format
    #[arg(short, long, global = true)]
    format: Option<String>,

    /// Shorthand for --format json
    #[arg(short = 'j', long = "json", global = true)]
    json: bool,

    /// Pretty-print JSON output
    #[arg(long, global = true)]
    pretty: bool,

    /// Disable ANSI colors
    #[arg(long, global = true)]
    no_color: bool,

    /// Verbose logging to stderr
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Fetch and display provider usage
    Usage {
        /// Provider to query (default: all enabled)
        #[arg(short, long)]
        provider: Option<String>,

        /// Override source mode (auto|oauth|cli|api)
        #[arg(long)]
        source: Option<String>,

        /// Include provider health status
        #[arg(long)]
        status: bool,

        /// Show detailed cost breakdown (by-model + recent days)
        #[arg(short, long)]
        all: bool,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Install this project's Codex skill into an agents skills directory
    InstallSkill(InstallSkillArgs),
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Generate default config file
    Init,
    /// Edit enabled providers interactively
    Edit,
    /// Validate config file
    Check,
    /// Enable a provider
    Add {
        /// Provider ID to enable
        provider: String,
    },
    /// Disable a provider
    Remove {
        /// Provider ID to disable
        provider: String,
    },
}

#[derive(rust_embed::RustEmbed)]
#[folder = ".skill"]
struct SkillAssets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let output_opts = cli::output::OutputOptions {
        format: if cli.json {
            cli::output::OutputFormat::Json
        } else {
            match cli.format.as_deref() {
                Some("json") => cli::output::OutputFormat::Json,
                _ => cli::output::OutputFormat::Text,
            }
        },
        pretty: cli.pretty,
        use_color: cli::output::detect_color(!cli.no_color),
        verbose: cli.verbose,
    };

    match cli.command {
        None | Some(Commands::Usage { .. }) => {
            let (provider, source, status, all) = match cli.command {
                Some(Commands::Usage {
                    provider,
                    source,
                    status,
                    all,
                }) => (provider, source, status, all),
                _ => (None, None, false, false),
            };
            cli::usage_cmd::run(provider, source, status, all, &output_opts).await?;
        }
        Some(Commands::Config { action }) => match action {
            ConfigAction::Init => cli::config_cmd::init(&output_opts)?,
            ConfigAction::Edit => cli::config_cmd::edit(&output_opts)?,
            ConfigAction::Check => cli::config_cmd::check(&output_opts)?,
            ConfigAction::Add { provider } => cli::config_cmd::add(&provider, &output_opts)?,
            ConfigAction::Remove { provider } => {
                cli::config_cmd::remove(&provider, &output_opts)?
            }
        },
        Some(Commands::InstallSkill(args)) => {
            let source = load_embedded_skill::<SkillAssets>();
            let result = install_interactive(source, &args)?;
            print_install_result(&result);
        }
    }

    Ok(())
}
