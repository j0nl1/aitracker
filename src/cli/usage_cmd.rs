use anyhow::Result;
use serde::Serialize;

use crate::cli::output::{OutputFormat, OutputOptions};
use crate::cli::renderer;
use crate::core::config::AppConfig;
use crate::core::models::credits::CreditsSnapshot;
use crate::core::models::status::StatusInfo;
use crate::core::models::usage::UsageSnapshot;
use crate::core::providers::Provider;

#[derive(Serialize)]
struct ProviderPayload {
    #[serde(flatten)]
    usage: UsageSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    credits: Option<CreditsSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<crate::core::models::cost::CostSummary>,
}

fn dispatch_fetch(
    provider: Provider,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<crate::core::providers::fetch::FetchResult>> + Send>>
{
    use crate::core::providers::*;
    Box::pin(async move {
        match provider {
            Provider::Claude => claude::fetch().await,
            Provider::Codex => codex::fetch().await,
            Provider::Copilot => copilot::fetch().await,
            Provider::Warp => warp::fetch().await,
            Provider::Kimi => kimi::fetch().await,
            Provider::KimiK2 => kimi_k2::fetch().await,
            Provider::OpenRouter => openrouter::fetch().await,
            Provider::MiniMax => minimax::fetch().await,
            Provider::Zai => zai::fetch().await,
            Provider::Ollama => ollama::fetch().await,
            Provider::Gemini => gemini::fetch().await,
            Provider::Kiro => kiro::fetch().await,
            Provider::Augment => augment::fetch().await,
            Provider::JetBrains => jetbrains::fetch().await,
            Provider::Cursor => cursor::fetch().await,
            Provider::OpenCode => opencode::fetch().await,
            Provider::Factory => factory::fetch().await,
            Provider::Amp => amp::fetch().await,
            Provider::Antigravity => antigravity::fetch().await,
            Provider::Synthetic => synthetic::fetch().await,
            Provider::VertexAi => vertex_ai::fetch().await,
        }
    })
}

pub async fn run(
    provider_filter: Option<String>,
    _source: Option<String>,
    fetch_status: bool,
    show_all: bool,
    opts: &OutputOptions,
) -> Result<()> {
    let config = AppConfig::load().unwrap_or_default();

    // Determine which providers to fetch
    let providers: Vec<Provider> = if let Some(filter) = &provider_filter {
        if filter == "all" {
            config
                .providers
                .iter()
                .filter(|p| p.enabled)
                .filter_map(|p| Provider::from_id(&p.id))
                .filter(|p| p.is_supported())
                .collect()
        } else {
            match Provider::from_id(filter) {
                Some(p) => vec![p],
                None => {
                    eprintln!("Unknown provider: '{}'", filter);
                    std::process::exit(1);
                }
            }
        }
    } else {
        // Default: all enabled and supported providers
        config
            .providers
            .iter()
            .filter(|p| p.enabled)
            .filter_map(|p| Provider::from_id(&p.id))
            .filter(|p| p.is_supported())
            .collect()
    };

    if providers.is_empty() {
        eprintln!("No supported providers enabled. Run `ait config init` to set up providers.");
        return Ok(());
    }

    // Spawn cost scan concurrently if any cost-scannable provider is requested
    let has_cost_provider = providers.iter().any(|p| {
        matches!(p, Provider::Claude | Provider::Codex | Provider::VertexAi)
    });
    let cost_handle = if has_cost_provider {
        Some(tokio::task::spawn_blocking(|| {
            crate::core::cost::scanner::scan(30).ok()
        }))
    } else {
        None
    };

    // Show spinner on stderr (text mode only)
    let show_spinner = matches!(opts.format, OutputFormat::Text);
    let spinner = if show_spinner {
        let is_cold_cache =
            has_cost_provider && !crate::core::cost::cache::CostCache::has_warm_cache();
        let msg: &'static str = if is_cold_cache {
            "First scan, indexing session files..."
        } else {
            "Fetching usage data..."
        };
        Some(tokio::spawn(async move {
            let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut i = 0usize;
            loop {
                eprint!("\r {} {}", frames[i % frames.len()], msg);
                i = i.wrapping_add(1);
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            }
        }))
    } else {
        None
    };

    // Fetch all providers concurrently
    let handles: Vec<_> = providers
        .into_iter()
        .map(|provider| {
            let should_fetch_status = fetch_status;
            tokio::spawn(async move {
                let result = dispatch_fetch(provider).await;
                let status = if should_fetch_status {
                    crate::core::status::fetch_status(&provider).await.ok()
                } else {
                    None
                };
                (provider, result, status)
            })
        })
        .collect();

    let mut results: Vec<(Provider, UsageSnapshot, Option<CreditsSnapshot>, Option<StatusInfo>)> =
        Vec::new();
    let mut errors: Vec<(Provider, String)> = Vec::new();

    for handle in handles {
        let (provider, result, status) = handle.await?;
        match result {
            Ok(fetch_result) => {
                results.push((provider, fetch_result.usage, fetch_result.credits, status));
            }
            Err(e) => {
                errors.push((provider, format!("{:#}", e)));
            }
        }
    }

    let cost_map: Option<std::collections::HashMap<Provider, crate::core::models::cost::CostSummary>> =
        match cost_handle {
            Some(handle) => handle.await.unwrap_or(None),
            None => None,
        };

    // Stop spinner and clear the line
    if let Some(s) = spinner {
        s.abort();
        eprint!("\r\x1b[2K");
    }

    match opts.format {
        OutputFormat::Text => {
            let mut sections: Vec<String> = Vec::new();

            for (provider, usage, credits, status) in &results {
                let provider_cost = cost_map
                    .as_ref()
                    .and_then(|m| m.get(provider));
                let text = renderer::render_provider(
                    usage,
                    credits.as_ref(),
                    provider_cost,
                    status.as_ref(),
                    show_all,
                    opts.use_color,
                );
                sections.push(text);
            }

            for (provider, err) in &errors {
                let header = format!(" {} (error)", provider.display_name());
                let msg = format!("  {}", err);
                if opts.use_color {
                    use colored::Colorize;
                    colored::control::set_override(true);
                    sections.push(format!("{}\n{}", header.bold(), msg.red()));
                } else {
                    sections.push(format!("{}\n{}", header, msg));
                }
            }

            println!("{}", sections.join("\n\n"));
        }
        OutputFormat::Json => {
            let payloads: Vec<ProviderPayload> = results
                .into_iter()
                .map(|(provider, usage, credits, _)| {
                    let cost = cost_map
                        .as_ref()
                        .and_then(|m| m.get(&provider))
                        .cloned();
                    ProviderPayload { usage, credits, cost }
                })
                .collect();

            let json = if opts.pretty {
                serde_json::to_string_pretty(&payloads)?
            } else {
                serde_json::to_string(&payloads)?
            };
            println!("{}", json);

            if !errors.is_empty() && opts.verbose {
                for (provider, err) in &errors {
                    eprintln!("Error fetching {}: {}", provider.display_name(), err);
                }
            }
        }
    }

    Ok(())
}
