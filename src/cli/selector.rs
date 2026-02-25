use std::io::{self, Write};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Attribute, Print, SetAttribute},
    terminal::{self, ClearType},
    ExecutableCommand, QueueableCommand,
};

use crate::core::providers::Provider;

pub struct SelectableProvider {
    pub id: String,
    pub display_name: String,
    pub auth_hint: String,
    pub detected: bool,
}

/// RAII guard that restores terminal state on drop (even on panic).
struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        io::stdout().execute(cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = io::stdout().execute(cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}

/// Returns `Ok(Some(selected_ids))` on confirm, `Ok(None)` if not a TTY, `Err` on cancel/Ctrl-C.
pub fn interactive_select(items: &[SelectableProvider]) -> anyhow::Result<Option<Vec<String>>> {
    if !io::stdin().is_terminal() {
        return Ok(None);
    }

    let _guard = RawModeGuard::enable()?;

    let mut checked: Vec<bool> = items.iter().map(|i| i.detected).collect();
    let mut cursor_pos: usize = 0;

    draw(&items, &checked, cursor_pos)?;

    loop {
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read()?
        {
            match (code, modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    clear_ui(items.len())?;
                    anyhow::bail!("cancelled");
                }
                (KeyCode::Esc, _) | (KeyCode::Char('q'), KeyModifiers::NONE) => {
                    clear_ui(items.len())?;
                    anyhow::bail!("cancelled");
                }
                (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                    if cursor_pos > 0 {
                        cursor_pos -= 1;
                    }
                }
                (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                    if cursor_pos + 1 < items.len() {
                        cursor_pos += 1;
                    }
                }
                (KeyCode::Char(' '), _) => {
                    checked[cursor_pos] = !checked[cursor_pos];
                }
                (KeyCode::Char('a'), KeyModifiers::NONE) => {
                    let all_checked = checked.iter().all(|&c| c);
                    for c in checked.iter_mut() {
                        *c = !all_checked;
                    }
                }
                (KeyCode::Enter, _) => {
                    clear_ui(items.len())?;
                    let selected: Vec<String> = items
                        .iter()
                        .zip(checked.iter())
                        .filter(|(_, &c)| c)
                        .map(|(item, _)| item.id.clone())
                        .collect();
                    return Ok(Some(selected));
                }
                _ => {}
            }
            draw(&items, &checked, cursor_pos)?;
        }
    }
}

fn draw(items: &[SelectableProvider], checked: &[bool], cursor_pos: usize) -> io::Result<()> {
    let mut stdout = io::stdout();

    // Move to start and clear
    stdout
        .queue(cursor::MoveToColumn(0))?
        .queue(terminal::Clear(ClearType::FromCursorDown))?;

    // Header
    stdout
        .queue(Print("Select providers to enable\r\n"))?
        .queue(Print("\r\n"))?
        .queue(Print(
            "  Use arrow keys to navigate, space to toggle, enter to confirm\r\n",
        ))?
        .queue(Print("\r\n"))?;

    // Items
    for (i, item) in items.iter().enumerate() {
        let marker = if i == cursor_pos { "> " } else { "  " };
        let check = if checked[i] { "X" } else { " " };

        if i == cursor_pos {
            stdout.queue(SetAttribute(Attribute::Reverse))?;
        }

        stdout.queue(Print(format!(
            "{marker}[{check}] {:<15} {}\r\n",
            item.display_name, item.auth_hint
        )))?;

        if i == cursor_pos {
            stdout.queue(SetAttribute(Attribute::Reset))?;
        }
    }

    // Footer
    let count = checked.iter().filter(|&&c| c).count();
    stdout
        .queue(Print("\r\n"))?
        .queue(Print(format!(
            "  {count} selected | enter: confirm | q: cancel\r\n"
        )))?;

    // Move cursor back up to top for next redraw
    let total_lines = items.len() + 5; // header(4) + items + footer(2)
    stdout.queue(cursor::MoveUp(total_lines as u16 + 1))?;

    stdout.flush()?;
    Ok(())
}

fn clear_ui(item_count: usize) -> io::Result<()> {
    let mut stdout = io::stdout();
    stdout
        .queue(cursor::MoveToColumn(0))?
        .queue(terminal::Clear(ClearType::FromCursorDown))?;
    // Extra clear: move down to where content was and clear
    let total_lines = item_count + 6;
    for _ in 0..total_lines {
        stdout
            .queue(Print("                                                                  \r\n"))?;
    }
    stdout.queue(cursor::MoveUp(total_lines as u16))?;
    stdout
        .queue(cursor::MoveToColumn(0))?
        .queue(terminal::Clear(ClearType::FromCursorDown))?;
    stdout.flush()?;
    Ok(())
}

/// Detect whether credentials for a provider are available locally.
/// Only checks files and env vars — no subprocess execution or network calls.
pub fn detect_credentials(provider: &Provider) -> bool {
    match provider {
        Provider::Claude => {
            let claude_dir = std::env::var("CLAUDE_CONFIG_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    dirs::home_dir()
                        .unwrap_or_default()
                        .join(".claude")
                });
            claude_dir.join(".credentials.json").exists()
        }
        Provider::Codex => {
            let codex_dir = std::env::var("CODEX_HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    dirs::home_dir()
                        .unwrap_or_default()
                        .join(".codex")
                });
            codex_dir.join("auth.json").exists()
        }
        Provider::Copilot => {
            std::env::var("GITHUB_TOKEN").is_ok() || which_exists("gh")
        }
        Provider::Gemini => {
            let gemini_dir = dirs::home_dir()
                .unwrap_or_default()
                .join(".gemini");
            gemini_dir.join("oauth_creds.json").exists()
        }
        Provider::Warp => std::env::var("WARP_TOKEN").is_ok(),
        Provider::Kimi => std::env::var("KIMI_TOKEN").is_ok(),
        Provider::KimiK2 => std::env::var("KIMI_K2_API_KEY").is_ok(),
        Provider::OpenRouter => std::env::var("OPENROUTER_API_KEY").is_ok(),
        Provider::MiniMax => std::env::var("MINIMAX_API_TOKEN").is_ok(),
        Provider::Zai => std::env::var("Z_AI_API_KEY").is_ok(),
        Provider::Kiro => which_exists("kiro-cli"),
        Provider::JetBrains => {
            // Check common JetBrains config directories
            if let Some(home) = dirs::home_dir() {
                let config_dir = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
                config_dir.join("JetBrains").exists()
            } else {
                false
            }
        }
        Provider::Antigravity => false, // Requires running language server, no static check
        Provider::Synthetic => std::env::var("SYNTHETIC_API_KEY").is_ok(),
        _ => false, // Stubs
    }
}

fn which_exists(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                dir.join(cmd).is_file()
            })
        })
        .unwrap_or(false)
}

/// Build the list of selectable providers (non-stubs only).
pub fn build_selectable_list() -> Vec<SelectableProvider> {
    Provider::all()
        .iter()
        .filter(|p| !p.is_stub())
        .map(|p| SelectableProvider {
            id: p.id().to_string(),
            display_name: p.display_name().to_string(),
            auth_hint: p.auth_hint().to_string(),
            detected: detect_credentials(p),
        })
        .collect()
}

/// Build the list of selectable providers with pre-checked state from existing config.
/// Providers in the config use their `enabled` flag; new providers default to unchecked.
pub fn build_selectable_list_from_config(config: &crate::core::config::AppConfig) -> Vec<SelectableProvider> {
    Provider::all()
        .iter()
        .filter(|p| !p.is_stub())
        .map(|p| {
            let detected = config
                .providers
                .iter()
                .find(|c| c.id == p.id())
                .map(|c| c.enabled)
                .unwrap_or(false);
            SelectableProvider {
                id: p.id().to_string(),
                display_name: p.display_name().to_string(),
                auth_hint: p.auth_hint().to_string(),
                detected,
            }
        })
        .collect()
}

/// Non-TTY fallback: returns IDs of providers with detected credentials.
pub fn auto_detect_providers() -> Vec<String> {
    Provider::all()
        .iter()
        .filter(|p| !p.is_stub() && detect_credentials(p))
        .map(|p| p.id().to_string())
        .collect()
}

use std::io::IsTerminal;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_selectable_list_excludes_stubs() {
        let items = build_selectable_list();
        assert_eq!(items.len(), 14);
    }

    #[test]
    fn selectable_list_has_correct_ids() {
        let items = build_selectable_list();
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"claude"));
        assert!(ids.contains(&"codex"));
        assert!(ids.contains(&"synthetic"));
        assert!(!ids.contains(&"cursor"));
        assert!(!ids.contains(&"ollama"));
    }

    #[test]
    fn which_exists_finds_common_binary() {
        // `ls` should exist on any unix system
        assert!(which_exists("ls"));
    }

    #[test]
    fn which_exists_returns_false_for_nonexistent() {
        assert!(!which_exists("definitely_not_a_real_command_xyz"));
    }

    #[test]
    fn auto_detect_providers_returns_vec() {
        // Just verify it runs without panic — actual detection depends on environment
        let detected = auto_detect_providers();
        assert!(detected.len() <= 14);
    }
}
