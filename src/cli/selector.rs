use std::io;

use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::core::providers::Provider;

pub struct SelectableProvider {
    pub id: String,
    pub display_name: String,
    pub auth_hint: String,
    pub detected: bool,
}

/// Interactive checkbox list. Scrolling, viewport sizing, and keeping the
/// selected row visible are handled entirely by ratatui's `List`/`ListState`
/// — no hand-rolled cursor math, which is what made the list unreachable
/// once it grew past one screen.
struct SelectorApp<'a> {
    items: &'a [SelectableProvider],
    checked: Vec<bool>,
    state: ListState,
    should_exit: bool,
    cancelled: bool,
}

impl<'a> SelectorApp<'a> {
    fn new(items: &'a [SelectableProvider]) -> Self {
        let checked = items.iter().map(|i| i.detected).collect();
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self {
            items,
            checked,
            state,
            should_exit: false,
            cancelled: false,
        }
    }

    fn run(mut self, mut terminal: ratatui::DefaultTerminal) -> io::Result<Option<Vec<String>>> {
        while !self.should_exit {
            terminal.draw(|frame| self.draw(frame))?;
            if let Event::Key(key) = event::read()? {
                self.handle_key(key);
            }
        }
        if self.cancelled {
            return Ok(None);
        }
        let selected = self
            .items
            .iter()
            .zip(self.checked.iter())
            .filter(|(_, &c)| c)
            .map(|(item, _)| item.id.clone())
            .collect();
        Ok(Some(selected))
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.cancelled = true;
                self.should_exit = true;
            }
            (KeyCode::Esc, _) | (KeyCode::Char('q'), KeyModifiers::NONE) => {
                self.cancelled = true;
                self.should_exit = true;
            }
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.state.select_previous();
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.state.select_next();
            }
            (KeyCode::PageUp, _) => self.state.scroll_up_by(10),
            (KeyCode::PageDown, _) => self.state.scroll_down_by(10),
            (KeyCode::Home, _) => self.state.select_first(),
            (KeyCode::End, _) => self.state.select_last(),
            (KeyCode::Char(' '), _) => {
                if let Some(i) = self.state.selected() {
                    if let Some(c) = self.checked.get_mut(i) {
                        *c = !*c;
                    }
                }
            }
            (KeyCode::Char('a'), KeyModifiers::NONE) => {
                let all_checked = self.checked.iter().all(|&c| c);
                for c in self.checked.iter_mut() {
                    *c = !all_checked;
                }
            }
            (KeyCode::Enter, _) => {
                self.should_exit = true;
            }
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title
                Constraint::Length(1), // blank
                Constraint::Length(1), // instructions
                Constraint::Length(1), // blank
                Constraint::Min(1),    // scrollable list
                Constraint::Length(1), // blank
                Constraint::Length(1), // status
            ])
            .split(frame.area());

        frame.render_widget(Paragraph::new("Select providers to enable"), chunks[0]);
        frame.render_widget(
            Paragraph::new("  Use arrow keys to navigate, space to toggle, enter to confirm"),
            chunks[2],
        );

        let list_items: Vec<ListItem> = self
            .items
            .iter()
            .zip(self.checked.iter())
            .map(|(item, &checked)| {
                let mark = if checked { "X" } else { " " };
                ListItem::new(format!(
                    "[{mark}] {:<15} {}",
                    item.display_name, item.auth_hint
                ))
            })
            .collect();

        let list = List::new(list_items)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, chunks[4], &mut self.state);

        let count = self.checked.iter().filter(|&&c| c).count();
        frame.render_widget(
            Paragraph::new(format!("  {count} selected | enter: confirm | q: cancel")),
            chunks[6],
        );
    }
}

/// Returns `Ok(Some(selected_ids))` on confirm, `Ok(None)` if not a TTY, `Err` on cancel/Ctrl-C.
pub fn interactive_select(items: &[SelectableProvider]) -> anyhow::Result<Option<Vec<String>>> {
    if !io::stdin().is_terminal() {
        return Ok(None);
    }

    let terminal = ratatui::try_init()?;
    let result = SelectorApp::new(items).run(terminal);
    ratatui::restore();

    match result? {
        Some(selected) => Ok(Some(selected)),
        None => anyhow::bail!("cancelled"),
    }
}

/// Detect whether credentials for a provider are available locally.
/// Only checks files and env vars — no subprocess execution or network calls.
pub fn detect_credentials(provider: &Provider) -> bool {
    match provider {
        Provider::Claude => {
            let claude_dir = std::env::var("CLAUDE_CONFIG_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".claude"));
            claude_dir.join(".credentials.json").exists()
        }
        Provider::Codex => {
            let codex_dir = std::env::var("CODEX_HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".codex"));
            codex_dir.join("auth.json").exists()
        }
        Provider::Copilot => std::env::var("GITHUB_TOKEN").is_ok() || which_exists("gh"),
        Provider::Gemini => {
            let gemini_dir = dirs::home_dir().unwrap_or_default().join(".gemini");
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
        Provider::OpenAi => std::env::var("OPENAI_API_KEY").is_ok(),
        Provider::AzureOpenAi => std::env::var("AZURE_OPENAI_API_KEY").is_ok(),
        Provider::DeepSeek => std::env::var("DEEPSEEK_API_KEY").is_ok(),
        Provider::Fireworks => std::env::var("FIREWORKS_API_KEY").is_ok(),
        Provider::DeepInfra => std::env::var("DEEPINFRA_API_KEY").is_ok(),
        Provider::Moonshot => std::env::var("MOONSHOT_API_KEY").is_ok(),
        Provider::Venice => std::env::var("VENICE_API_KEY").is_ok(),
        Provider::Codebuff => std::env::var("CODEBUFF_API_KEY").is_ok(),
        Provider::Crof => std::env::var("CROF_API_KEY").is_ok(),
        Provider::Doubao => std::env::var("ARK_API_KEY").is_ok(),
        Provider::GroqCloud => std::env::var("GROQ_API_KEY").is_ok(),
        Provider::LlmProxy => std::env::var("LLM_PROXY_API_KEY").is_ok(),
        Provider::ClawRouter => std::env::var("CLAWROUTER_API_KEY").is_ok(),
        Provider::LiteLlm => std::env::var("LITELLM_API_KEY").is_ok(),
        Provider::Deepgram => std::env::var("DEEPGRAM_API_KEY").is_ok(),
        Provider::Poe => std::env::var("POE_API_KEY").is_ok(),
        Provider::Chutes => std::env::var("CHUTES_API_KEY").is_ok(),
        Provider::NeuralWatt => std::env::var("NEURALWATT_API_KEY").is_ok(),
        Provider::ZenMux => std::env::var("ZENMUX_MANAGEMENT_API_KEY").is_ok(),
        Provider::Xai => std::env::var("XAI_MANAGEMENT_API_KEY").is_ok(),
        Provider::IbmBob => std::env::var("BOBSHELL_API_KEY").is_ok(),
        Provider::ElevenLabs => std::env::var("ELEVENLABS_API_KEY").is_ok(),
        _ => false, // Stubs
    }
}

fn which_exists(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(cmd).is_file()))
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
pub fn build_selectable_list_from_config(
    config: &crate::core::config::AppConfig,
) -> Vec<SelectableProvider> {
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
        assert_eq!(items.len(), 34);
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
        assert!(!ids.contains(&"azure_openai"));
        assert!(!ids.contains(&"doubao"));
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
        assert!(detected.len() <= 34);
    }

    fn test_items(n: usize) -> Vec<SelectableProvider> {
        (0..n)
            .map(|i| SelectableProvider {
                id: format!("provider-{i}"),
                display_name: format!("Provider {i}"),
                auth_hint: "TEST_KEY".to_string(),
                detected: false,
            })
            .collect()
    }

    #[test]
    fn selector_app_starts_with_first_item_selected() {
        let items = test_items(5);
        let app = SelectorApp::new(&items);
        assert_eq!(app.state.selected(), Some(0));
    }

    #[test]
    fn selector_app_space_toggles_current_selection() {
        let items = test_items(3);
        let mut app = SelectorApp::new(&items);
        assert!(!app.checked[0]);
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.checked[0]);
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(!app.checked[0]);
    }

    #[test]
    fn selector_app_down_then_space_toggles_second_item() {
        let items = test_items(3);
        let mut app = SelectorApp::new(&items);
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(!app.checked[0]);
        assert!(app.checked[1]);
    }

    #[test]
    fn selector_app_select_all_toggles_everything() {
        let items = test_items(4);
        let mut app = SelectorApp::new(&items);
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(app.checked.iter().all(|&c| c));
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(app.checked.iter().all(|&c| !c));
    }

    #[test]
    fn selector_app_enter_confirms_without_cancelling() {
        let items = test_items(2);
        let mut app = SelectorApp::new(&items);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.should_exit);
        assert!(!app.cancelled);
    }

    #[test]
    fn selector_app_escape_cancels() {
        let items = test_items(2);
        let mut app = SelectorApp::new(&items);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.should_exit);
        assert!(app.cancelled);
    }
}
