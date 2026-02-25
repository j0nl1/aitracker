use anyhow::Result;

use crate::cli::output::OutputOptions;
use crate::cli::selector;
use crate::core::config::{AppConfig, ProviderConfig};
use crate::core::providers::Provider;

pub fn init(_opts: &OutputOptions) -> Result<()> {
    let path = AppConfig::config_path();
    if path.exists() {
        eprintln!("Config file already exists at {}", path.display());
        eprintln!("Remove it first if you want to regenerate.");
        return Ok(());
    }

    let items = selector::build_selectable_list();
    let selected_ids = match selector::interactive_select(&items) {
        Ok(Some(ids)) => ids,
        Ok(None) => {
            // Non-TTY fallback: enable all detected providers
            selector::auto_detect_providers()
        }
        Err(_) => {
            eprintln!("Config init cancelled.");
            return Ok(());
        }
    };

    match AppConfig::generate_with_providers(&selected_ids) {
        Ok(path) => {
            let count = selected_ids.len();
            println!("Generated config at {}", path.display());
            if count > 0 {
                println!(
                    "  {} provider{} enabled: {}",
                    count,
                    if count == 1 { "" } else { "s" },
                    selected_ids.join(", ")
                );
            } else {
                println!("  No providers enabled. Edit the config to enable providers.");
            }
        }
        Err(e) => {
            eprintln!("Failed to generate config: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}

pub fn edit(_opts: &OutputOptions) -> Result<()> {
    let path = AppConfig::config_path();
    if !path.exists() {
        eprintln!("No config file found at {}", path.display());
        eprintln!("Run `ait config init` to create one first.");
        return Ok(());
    }

    let mut config = match AppConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    };

    let items = selector::build_selectable_list_from_config(&config);
    let selected_ids = match selector::interactive_select(&items) {
        Ok(Some(ids)) => ids,
        Ok(None) => {
            eprintln!("Not a terminal. Edit the config manually at {}", path.display());
            return Ok(());
        }
        Err(_) => {
            eprintln!("Config edit cancelled.");
            return Ok(());
        }
    };

    match config.update_providers(&selected_ids) {
        Ok(path) => {
            let count = selected_ids.len();
            println!("Updated config at {}", path.display());
            if count > 0 {
                println!(
                    "  {} provider{} enabled: {}",
                    count,
                    if count == 1 { "" } else { "s" },
                    selected_ids.join(", ")
                );
            } else {
                println!("  No providers enabled.");
            }
        }
        Err(e) => {
            eprintln!("Failed to update config: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}

pub fn add(provider_id: &str, _opts: &OutputOptions) -> Result<()> {
    let provider = match Provider::from_id(provider_id) {
        Some(p) => p,
        None => {
            eprintln!("Unknown provider: {}", provider_id);
            std::process::exit(1);
        }
    };

    if provider.is_stub() {
        eprintln!(
            "Provider '{}' is not yet supported (stub)",
            provider_id
        );
        std::process::exit(1);
    }

    let mut config = AppConfig::load()?;

    if let Some(existing) = config.providers.iter().find(|p| p.id == provider.id()) {
        if existing.enabled {
            eprintln!("Provider '{}' is already enabled", provider.id());
            std::process::exit(1);
        }
    }

    // Enable existing entry or add a new one
    let mut found = false;
    for p in &mut config.providers {
        if p.id == provider.id() {
            p.enabled = true;
            found = true;
            break;
        }
    }
    if !found {
        config.providers.push(ProviderConfig {
            id: provider.id().to_string(),
            enabled: true,
            source: "auto".to_string(),
            api_key: None,
        });
    }

    config.save()?;
    println!("Enabled provider: {}", provider.id());
    Ok(())
}

pub fn remove(provider_id: &str, _opts: &OutputOptions) -> Result<()> {
    let provider = match Provider::from_id(provider_id) {
        Some(p) => p,
        None => {
            eprintln!("Unknown provider: {}", provider_id);
            std::process::exit(1);
        }
    };

    if provider.is_stub() {
        eprintln!(
            "Provider '{}' is not yet supported (stub)",
            provider_id
        );
        std::process::exit(1);
    }

    let mut config = AppConfig::load()?;

    match config.providers.iter().find(|p| p.id == provider.id()) {
        Some(existing) if !existing.enabled => {
            eprintln!("Provider '{}' is already disabled", provider.id());
            std::process::exit(1);
        }
        None => {
            eprintln!("Provider '{}' is already disabled", provider.id());
            std::process::exit(1);
        }
        _ => {}
    }

    for p in &mut config.providers {
        if p.id == provider.id() {
            p.enabled = false;
            break;
        }
    }

    config.save()?;
    println!("Disabled provider: {}", provider.id());
    Ok(())
}

pub fn check(_opts: &OutputOptions) -> Result<()> {
    let path = AppConfig::config_path();
    if !path.exists() {
        eprintln!("No config file found at {}", path.display());
        eprintln!("Run `ait config init` to create one.");
        return Ok(());
    }

    let config = match AppConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    };

    let issues = config.validate();
    if issues.is_empty() {
        println!("Config is valid: {}", path.display());
        let enabled: Vec<_> = config
            .providers
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.id.as_str())
            .collect();
        if enabled.is_empty() {
            println!("  No providers enabled.");
        } else {
            println!("  Enabled providers: {}", enabled.join(", "));
        }
    } else {
        eprintln!("Config issues found in {}:", path.display());
        for issue in &issues {
            eprintln!("  - {}", issue);
        }
        std::process::exit(1);
    }
    Ok(())
}
