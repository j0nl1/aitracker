use anyhow::{Context, Result};
use dialoguer::{Confirm, Password, Select};
use std::io::IsTerminal;

use crate::cli::output::{OutputFormat, OutputOptions};
use crate::cli::selector;
use crate::core::config::{AppConfig, ProviderConfig};
use crate::core::providers::Provider;
use crate::core::secrets;

pub fn init(_opts: &OutputOptions) -> Result<()> {
    secrets::load_env_file(&AppConfig::secrets_path())?;
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

pub fn edit(provider_id: Option<&str>, opts: &OutputOptions) -> Result<()> {
    match provider_id {
        Some(provider_id) => edit_provider(provider_id, opts),
        None => edit_all(opts),
    }
}

fn edit_all(_opts: &OutputOptions) -> Result<()> {
    secrets::load_env_file(&AppConfig::secrets_path())?;
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

fn edit_provider(provider_id: &str, _opts: &OutputOptions) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("`ait config edit <provider>` requires an interactive terminal");
    }

    let provider = Provider::from_id(provider_id)
        .with_context(|| format!("Unknown provider: {}", provider_id))?;
    if provider.is_stub() {
        anyhow::bail!("Provider '{}' is not yet supported (stub)", provider.id());
    }

    let mut config = AppConfig::load()?;
    let currently_enabled = config
        .providers
        .iter()
        .find(|entry| entry.id == provider.id())
        .map(|entry| entry.enabled)
        .unwrap_or(false);
    let enabled = Confirm::new()
        .with_prompt(format!(
            "Enable {} ({})?",
            provider.display_name(),
            provider.id()
        ))
        .default(currently_enabled)
        .interact()?;

    set_provider_enabled(&mut config, provider, enabled);
    if enabled {
        configure_provider_secret(provider)?;
    }
    let path = config.save()?;
    println!(
        "{} provider '{}' in {}",
        if enabled { "Enabled" } else { "Disabled" },
        provider.id(),
        path.display()
    );
    Ok(())
}

fn set_provider_enabled(config: &mut AppConfig, provider: Provider, enabled: bool) {
    if let Some(entry) = config
        .providers
        .iter_mut()
        .find(|entry| entry.id == provider.id())
    {
        entry.enabled = enabled;
    } else {
        config.providers.push(ProviderConfig {
            id: provider.id().to_string(),
            enabled,
            source: "auto".to_string(),
            api_key: None,
        });
    }
}

fn configure_provider_secret(provider: Provider) -> Result<()> {
    let Some(env_var) = provider.api_key_env_var() else {
        println!("Authentication: {}", provider.auth_hint());
        return Ok(());
    };

    let secrets_path = AppConfig::secrets_path();
    let stored_value = secrets::read_env_value(&secrets_path, env_var)?;
    let mut choices = vec![
        format!(
            "Provide {} now (store in {})",
            env_var,
            secrets_path.display()
        ),
        format!(
            "Use environment or other provider auth (remove stored {})",
            env_var
        ),
    ];
    if stored_value.is_some() {
        choices.push(format!("Retain stored {} value", env_var));
    }
    let default = if stored_value.is_some() { 2 } else { 1 };
    let choice = Select::new()
        .with_prompt(format!("Credential source for {}", provider.display_name()))
        .items(&choices)
        .default(default)
        .interact()?;

    match choice {
        0 => {
            let secret = Password::new()
                .with_prompt(format!("Enter {}", env_var))
                .with_confirmation("Confirm API key", "API keys did not match")
                .interact()?;
            secrets::set_env_value(&secrets_path, env_var, Some(&secret))?;
            println!(
                "Stored {} in {} (owner-only permissions)",
                env_var,
                secrets_path.display()
            );
        }
        1 => {
            secrets::set_env_value(&secrets_path, env_var, None)?;
            println!(
                "Removed stored {}. Authentication: {}",
                env_var,
                provider.auth_hint()
            );
        }
        _ => println!("Retained stored {} value", env_var),
    }

    if provider.auth_hint().contains(" + ") {
        println!("Additional configuration: {}", provider.auth_hint());
    }
    Ok(())
}

pub fn list(opts: &OutputOptions) -> Result<()> {
    let config = AppConfig::load()?;
    let rows: Vec<_> = Provider::all()
        .iter()
        .map(|provider| {
            let enabled = config
                .providers
                .iter()
                .find(|entry| entry.id == provider.id())
                .map(|entry| entry.enabled)
                .unwrap_or(false);
            (provider, enabled)
        })
        .collect();

    match opts.format {
        OutputFormat::Json => {
            let value: Vec<_> = rows
                .iter()
                .map(|(provider, enabled)| {
                    serde_json::json!({
                        "id": provider.id(),
                        "enabled": enabled,
                        "supported": !provider.is_stub(),
                    })
                })
                .collect();
            if opts.pretty {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("{}", serde_json::to_string(&value)?);
            }
        }
        OutputFormat::Text => {
            println!("{:<16} ENABLED", "PROVIDER");
            for (provider, enabled) in rows {
                println!(
                    "{:<16} {}",
                    provider.id(),
                    if enabled { "yes" } else { "no" }
                );
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_provider_enabled_updates_existing_entry() {
        let mut config = AppConfig::default();
        set_provider_enabled(&mut config, Provider::Copilot, true);

        assert!(
            config
                .providers
                .iter()
                .find(|entry| entry.id == "copilot")
                .unwrap()
                .enabled
        );
    }

    #[test]
    fn set_provider_enabled_adds_missing_entry() {
        let mut config = AppConfig::default();
        config.providers.retain(|entry| entry.id != "openrouter");
        set_provider_enabled(&mut config, Provider::OpenRouter, true);

        let entry = config
            .providers
            .iter()
            .find(|entry| entry.id == "openrouter")
            .unwrap();
        assert!(entry.enabled);
        assert_eq!(entry.source, "auto");
        assert!(entry.api_key.is_none());
    }
}
