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
    /// Run the read-only usage broker
    Serve {
        /// Loopback address to listen on
        #[arg(long, default_value = "127.0.0.1:7843")]
        bind: String,

        /// Seconds between provider refreshes
        #[arg(long, default_value_t = 60)]
        refresh_seconds: u64,

        /// Timeout in seconds for each provider request
        #[arg(long, default_value_t = 30)]
        provider_timeout_seconds: u64,
    },
    /// Query a running usage broker
    Client {
        #[command(subcommand)]
        action: ClientAction,
    },
    /// Install this project's Codex skill into an agents skills directory
    InstallSkill(InstallSkillArgs),
}

#[derive(Subcommand)]
enum ClientAction {
    /// Read sanitized provider usage from the broker
    Usage {
        /// Broker URL (loopback HTTP only)
        #[arg(long, default_value = "http://127.0.0.1:7843")]
        url: String,

        /// Return only one provider ID
        #[arg(short, long)]
        provider: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Generate default config file
    Init,
    /// Edit enabled providers interactively, or configure one provider
    Edit {
        /// Provider ID to configure
        provider: Option<String>,
    },
    /// List all providers and whether they are enabled
    List,
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
            ConfigAction::Edit { provider } => {
                cli::config_cmd::edit(provider.as_deref(), &output_opts)?
            }
            ConfigAction::List => cli::config_cmd::list(&output_opts)?,
            ConfigAction::Check => cli::config_cmd::check(&output_opts)?,
            ConfigAction::Add { provider } => cli::config_cmd::add(&provider, &output_opts)?,
            ConfigAction::Remove { provider } => {
                cli::config_cmd::remove(&provider, &output_opts)?
            }
        },
        Some(Commands::Serve {
            bind,
            refresh_seconds,
            provider_timeout_seconds,
        }) => {
            cli::broker_cmd::serve(
                bind,
                refresh_seconds,
                provider_timeout_seconds,
                &output_opts,
            )
            .await?;
        }
        Some(Commands::Client { action }) => match action {
            ClientAction::Usage { url, provider } => {
                cli::broker_cmd::client_usage(url, provider, &output_opts).await?
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_list() {
        let cli = Cli::try_parse_from(["ait", "config", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Config {
                action: ConfigAction::List
            })
        ));
    }

    #[test]
    fn parses_config_edit_provider() {
        let cli = Cli::try_parse_from(["ait", "config", "edit", "copilot"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Config {
                action: ConfigAction::Edit {
                    provider: Some(provider)
                }
            }) if provider == "copilot"
        ));
    }

    #[test]
    fn parses_broker_server_options() {
        let cli = Cli::try_parse_from([
            "ait",
            "serve",
            "--bind",
            "127.0.0.1:9000",
            "--refresh-seconds",
            "30",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Serve {
                bind,
                refresh_seconds: 30,
                provider_timeout_seconds: 30,
            }) if bind == "127.0.0.1:9000"
        ));
    }

    #[test]
    fn parses_broker_client_usage() {
        let cli = Cli::try_parse_from([
            "ait",
            "client",
            "usage",
            "--provider",
            "copilot",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Client {
                action: ClientAction::Usage {
                    provider: Some(provider),
                    ..
                }
            }) if provider == "copilot"
        ));
    }
}
