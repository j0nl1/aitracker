pub mod amp;
pub mod antigravity;
pub mod augment;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod factory;
pub mod fetch;
pub mod gemini;
pub mod jetbrains;
pub mod kimi;
pub mod kimi_k2;
pub mod kiro;
pub mod minimax;
pub mod ollama;
pub mod opencode;
pub mod openrouter;
pub mod synthetic;
pub mod vertex_ai;
pub mod warp;
pub mod zai;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Claude,
    Codex,
    Copilot,
    Warp,
    Kimi,
    KimiK2,
    OpenRouter,
    MiniMax,
    Zai,
    Ollama,
    Gemini,
    Kiro,
    Augment,
    JetBrains,
    Cursor,
    OpenCode,
    Factory,
    Amp,
    Antigravity,
    Synthetic,
    VertexAi,
}

impl Provider {
    pub fn from_id(id: &str) -> Option<Self> {
        match id.to_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "copilot" => Some(Self::Copilot),
            "warp" => Some(Self::Warp),
            "kimi" => Some(Self::Kimi),
            "kimi_k2" | "kimi-k2" | "kimik2" => Some(Self::KimiK2),
            "openrouter" => Some(Self::OpenRouter),
            "minimax" => Some(Self::MiniMax),
            "zai" => Some(Self::Zai),
            "ollama" => Some(Self::Ollama),
            "gemini" => Some(Self::Gemini),
            "kiro" => Some(Self::Kiro),
            "augment" => Some(Self::Augment),
            "jetbrains" => Some(Self::JetBrains),
            "cursor" => Some(Self::Cursor),
            "opencode" => Some(Self::OpenCode),
            "factory" => Some(Self::Factory),
            "amp" => Some(Self::Amp),
            "antigravity" => Some(Self::Antigravity),
            "synthetic" => Some(Self::Synthetic),
            "vertex_ai" | "vertex-ai" | "vertexai" => Some(Self::VertexAi),
            _ => None,
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Warp => "warp",
            Self::Kimi => "kimi",
            Self::KimiK2 => "kimi_k2",
            Self::OpenRouter => "openrouter",
            Self::MiniMax => "minimax",
            Self::Zai => "zai",
            Self::Ollama => "ollama",
            Self::Gemini => "gemini",
            Self::Kiro => "kiro",
            Self::Augment => "augment",
            Self::JetBrains => "jetbrains",
            Self::Cursor => "cursor",
            Self::OpenCode => "opencode",
            Self::Factory => "factory",
            Self::Amp => "amp",
            Self::Antigravity => "antigravity",
            Self::Synthetic => "synthetic",
            Self::VertexAi => "vertex_ai",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Copilot => "Copilot",
            Self::Warp => "Warp",
            Self::Kimi => "Kimi",
            Self::KimiK2 => "Kimi K2",
            Self::OpenRouter => "OpenRouter",
            Self::MiniMax => "MiniMax",
            Self::Zai => "Zai",
            Self::Ollama => "Ollama",
            Self::Gemini => "Gemini",
            Self::Kiro => "Kiro",
            Self::Augment => "Augment",
            Self::JetBrains => "JetBrains",
            Self::Cursor => "Cursor",
            Self::OpenCode => "OpenCode",
            Self::Factory => "Factory",
            Self::Amp => "Amp",
            Self::Antigravity => "Antigravity",
            Self::Synthetic => "Synthetic",
            Self::VertexAi => "Vertex AI",
        }
    }

    pub fn session_label(&self) -> &'static str {
        match self {
            Self::Gemini => "Pro",
            _ => "Session",
        }
    }

    pub fn weekly_label(&self) -> &'static str {
        match self {
            Self::Gemini => "Flash",
            _ => "Weekly",
        }
    }

    pub fn tertiary_label(&self) -> &'static str {
        match self {
            Self::Claude => "Sonnet",
            _ => "Model",
        }
    }

    pub fn status_page_url(&self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("https://status.anthropic.com"),
            Self::Codex => Some("https://status.openai.com"),
            Self::Copilot => Some("https://www.githubstatus.com"),
            _ => None,
        }
    }

    pub fn is_supported(&self) -> bool {
        true
    }

    /// All provider variants in display order (supported first, stubs last).
    pub fn all() -> &'static [Provider] {
        &[
            // Supported
            Provider::Claude,
            Provider::Codex,
            Provider::Copilot,
            Provider::Gemini,
            Provider::Warp,
            Provider::Kimi,
            Provider::KimiK2,
            Provider::OpenRouter,
            Provider::MiniMax,
            Provider::Zai,
            Provider::Kiro,
            Provider::JetBrains,
            Provider::Antigravity,
            Provider::Synthetic,
            // Stubs
            Provider::Cursor,
            Provider::Ollama,
            Provider::Augment,
            Provider::OpenCode,
            Provider::Factory,
            Provider::Amp,
            Provider::VertexAi,
        ]
    }

    pub fn is_stub(&self) -> bool {
        matches!(
            self,
            Self::Cursor
                | Self::Ollama
                | Self::Augment
                | Self::OpenCode
                | Self::Factory
                | Self::Amp
                | Self::VertexAi
        )
    }

    pub fn auth_hint(&self) -> &'static str {
        match self {
            Self::Claude => "auto-detected (~/.claude/)",
            Self::Codex => "auto-detected (~/.codex/)",
            Self::Copilot => "GITHUB_TOKEN or gh CLI",
            Self::Gemini => "auto-detected (~/.gemini/)",
            Self::Warp => "WARP_TOKEN",
            Self::Kimi => "KIMI_TOKEN",
            Self::KimiK2 => "KIMI_K2_API_KEY",
            Self::OpenRouter => "OPENROUTER_API_KEY",
            Self::MiniMax => "MINIMAX_API_TOKEN",
            Self::Zai => "Z_AI_API_KEY",
            Self::Kiro => "kiro-cli",
            Self::JetBrains => "IDE config files",
            Self::Antigravity => "language server process",
            Self::Synthetic => "SYNTHETIC_API_KEY",
            Self::Cursor | Self::Ollama | Self::Augment | Self::OpenCode | Self::Factory
            | Self::Amp | Self::VertexAi => "planned",
        }
    }
}
