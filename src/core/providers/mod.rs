pub mod amp;
pub mod antigravity;
pub mod augment;
pub mod azure_openai;
pub mod claude;
pub mod clawrouter;
pub mod chutes;
pub mod codebuff;
pub mod codex;
pub mod copilot;
pub mod crof;
pub mod cursor;
pub mod deepgram;
pub mod deepinfra;
pub mod deepseek;
pub mod doubao;
pub mod elevenlabs;
pub mod factory;
pub mod fetch;
pub mod fireworks;
pub mod gemini;
pub mod groqcloud;
pub mod ibm_bob;
pub mod jetbrains;
pub mod kimi;
pub mod kimi_k2;
pub mod kiro;
pub mod litellm;
pub mod llm_proxy;
pub mod minimax;
pub mod moonshot;
pub mod neuralwatt;
pub mod ollama;
pub mod opencode;
pub mod openai;
pub mod openrouter;
pub mod poe;
pub mod synthetic;
pub mod venice;
pub mod vertex_ai;
pub mod warp;
pub mod xai;
pub mod zai;
pub mod zenmux;

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
    OpenAi,
    AzureOpenAi,
    DeepSeek,
    Fireworks,
    DeepInfra,
    Moonshot,
    Venice,
    Codebuff,
    Crof,
    Doubao,
    GroqCloud,
    LlmProxy,
    ClawRouter,
    LiteLlm,
    Deepgram,
    Poe,
    Chutes,
    NeuralWatt,
    ZenMux,
    Xai,
    IbmBob,
    ElevenLabs,
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
            "openai" => Some(Self::OpenAi),
            "azure_openai" | "azure-openai" => Some(Self::AzureOpenAi),
            "deepseek" => Some(Self::DeepSeek),
            "fireworks" => Some(Self::Fireworks),
            "deepinfra" => Some(Self::DeepInfra),
            "moonshot" => Some(Self::Moonshot),
            "venice" => Some(Self::Venice),
            "codebuff" => Some(Self::Codebuff),
            "crof" => Some(Self::Crof),
            "doubao" => Some(Self::Doubao),
            "groqcloud" | "groq" => Some(Self::GroqCloud),
            "llm_proxy" | "llm-proxy" => Some(Self::LlmProxy),
            "clawrouter" => Some(Self::ClawRouter),
            "litellm" => Some(Self::LiteLlm),
            "deepgram" => Some(Self::Deepgram),
            "poe" => Some(Self::Poe),
            "chutes" => Some(Self::Chutes),
            "neuralwatt" => Some(Self::NeuralWatt),
            "zenmux" => Some(Self::ZenMux),
            "xai" => Some(Self::Xai),
            "ibm_bob" | "ibm-bob" => Some(Self::IbmBob),
            "elevenlabs" => Some(Self::ElevenLabs),
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
            Self::OpenAi => "openai",
            Self::AzureOpenAi => "azure_openai",
            Self::DeepSeek => "deepseek",
            Self::Fireworks => "fireworks",
            Self::DeepInfra => "deepinfra",
            Self::Moonshot => "moonshot",
            Self::Venice => "venice",
            Self::Codebuff => "codebuff",
            Self::Crof => "crof",
            Self::Doubao => "doubao",
            Self::GroqCloud => "groqcloud",
            Self::LlmProxy => "llm_proxy",
            Self::ClawRouter => "clawrouter",
            Self::LiteLlm => "litellm",
            Self::Deepgram => "deepgram",
            Self::Poe => "poe",
            Self::Chutes => "chutes",
            Self::NeuralWatt => "neuralwatt",
            Self::ZenMux => "zenmux",
            Self::Xai => "xai",
            Self::IbmBob => "ibm_bob",
            Self::ElevenLabs => "elevenlabs",
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
            Self::OpenAi => "OpenAI",
            Self::AzureOpenAi => "Azure OpenAI",
            Self::DeepSeek => "DeepSeek",
            Self::Fireworks => "Fireworks",
            Self::DeepInfra => "DeepInfra",
            Self::Moonshot => "Moonshot",
            Self::Venice => "Venice",
            Self::Codebuff => "Codebuff",
            Self::Crof => "Crof",
            Self::Doubao => "Doubao",
            Self::GroqCloud => "GroqCloud",
            Self::LlmProxy => "LLM Proxy",
            Self::ClawRouter => "ClawRouter",
            Self::LiteLlm => "LiteLLM",
            Self::Deepgram => "Deepgram",
            Self::Poe => "Poe",
            Self::Chutes => "Chutes",
            Self::NeuralWatt => "NeuralWatt",
            Self::ZenMux => "ZenMux",
            Self::Xai => "xAI",
            Self::IbmBob => "IBM Bob",
            Self::ElevenLabs => "ElevenLabs",
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
            Provider::OpenAi,
            Provider::DeepSeek,
            Provider::Fireworks,
            Provider::DeepInfra,
            Provider::Moonshot,
            Provider::Venice,
            Provider::Codebuff,
            Provider::Crof,
            Provider::GroqCloud,
            Provider::LlmProxy,
            Provider::ClawRouter,
            Provider::LiteLlm,
            Provider::Deepgram,
            Provider::Poe,
            Provider::Chutes,
            Provider::NeuralWatt,
            Provider::ZenMux,
            Provider::Xai,
            Provider::IbmBob,
            Provider::ElevenLabs,
            // Stubs
            Provider::Cursor,
            Provider::Ollama,
            Provider::Augment,
            Provider::OpenCode,
            Provider::Factory,
            Provider::Amp,
            Provider::VertexAi,
            Provider::AzureOpenAi,
            Provider::Doubao,
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
                | Self::AzureOpenAi
                | Self::Doubao
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
            Self::OpenAi => "OPENAI_API_KEY",
            Self::DeepSeek => "DEEPSEEK_API_KEY",
            Self::Fireworks => "FIREWORKS_API_KEY + account slug",
            Self::DeepInfra => "DEEPINFRA_API_KEY",
            Self::Moonshot => "MOONSHOT_API_KEY",
            Self::Venice => "VENICE_API_KEY",
            Self::Codebuff => "CODEBUFF_API_KEY",
            Self::Crof => "CROF_API_KEY",
            Self::GroqCloud => "GROQ_API_KEY",
            Self::LlmProxy => "LLM_PROXY_API_KEY + base URL",
            Self::ClawRouter => "CLAWROUTER_API_KEY",
            Self::LiteLlm => "LITELLM_API_KEY + base URL",
            Self::Deepgram => "DEEPGRAM_API_KEY",
            Self::Poe => "POE_API_KEY",
            Self::Chutes => "CHUTES_API_KEY",
            Self::NeuralWatt => "NEURALWATT_API_KEY",
            Self::ZenMux => "ZENMUX_MANAGEMENT_API_KEY",
            Self::Xai => "XAI_MANAGEMENT_API_KEY + team ID",
            Self::IbmBob => "BOBSHELL_API_KEY",
            Self::ElevenLabs => "ELEVENLABS_API_KEY",
            Self::Cursor | Self::Ollama | Self::Augment | Self::OpenCode | Self::Factory
            | Self::Amp | Self::VertexAi | Self::AzureOpenAi | Self::Doubao => "planned",
        }
    }

    /// Canonical environment variable containing this provider's API secret.
    /// Providers authenticated through local files or subprocesses return `None`.
    pub fn api_key_env_var(&self) -> Option<&'static str> {
        match self {
            Self::Copilot => Some("GITHUB_TOKEN"),
            Self::Warp => Some("WARP_TOKEN"),
            Self::Kimi => Some("KIMI_TOKEN"),
            Self::KimiK2 => Some("KIMI_K2_API_KEY"),
            Self::OpenRouter => Some("OPENROUTER_API_KEY"),
            Self::MiniMax => Some("MINIMAX_API_TOKEN"),
            Self::Zai => Some("Z_AI_API_KEY"),
            Self::Synthetic => Some("SYNTHETIC_API_KEY"),
            Self::OpenAi => Some("OPENAI_API_KEY"),
            Self::AzureOpenAi => Some("AZURE_OPENAI_API_KEY"),
            Self::DeepSeek => Some("DEEPSEEK_API_KEY"),
            Self::Fireworks => Some("FIREWORKS_API_KEY"),
            Self::DeepInfra => Some("DEEPINFRA_API_KEY"),
            Self::Moonshot => Some("MOONSHOT_API_KEY"),
            Self::Venice => Some("VENICE_API_KEY"),
            Self::Codebuff => Some("CODEBUFF_API_KEY"),
            Self::Crof => Some("CROF_API_KEY"),
            Self::Doubao => Some("ARK_API_KEY"),
            Self::GroqCloud => Some("GROQ_API_KEY"),
            Self::LlmProxy => Some("LLM_PROXY_API_KEY"),
            Self::ClawRouter => Some("CLAWROUTER_API_KEY"),
            Self::LiteLlm => Some("LITELLM_API_KEY"),
            Self::Deepgram => Some("DEEPGRAM_API_KEY"),
            Self::Poe => Some("POE_API_KEY"),
            Self::Chutes => Some("CHUTES_API_KEY"),
            Self::NeuralWatt => Some("NEURALWATT_API_KEY"),
            Self::ZenMux => Some("ZENMUX_MANAGEMENT_API_KEY"),
            Self::Xai => Some("XAI_MANAGEMENT_API_KEY"),
            Self::IbmBob => Some("BOBSHELL_API_KEY"),
            Self::ElevenLabs => Some("ELEVENLABS_API_KEY"),
            Self::Claude
            | Self::Codex
            | Self::Gemini
            | Self::Kiro
            | Self::JetBrains
            | Self::Antigravity
            | Self::Cursor
            | Self::Ollama
            | Self::Augment
            | Self::OpenCode
            | Self::Factory
            | Self::Amp
            | Self::VertexAi => None,
        }
    }
}

/// Fetch one provider through its canonical implementation.
pub async fn fetch(provider: Provider) -> anyhow::Result<fetch::FetchResult> {
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
        Provider::OpenAi => openai::fetch().await,
        Provider::AzureOpenAi => azure_openai::fetch().await,
        Provider::DeepSeek => deepseek::fetch().await,
        Provider::Fireworks => fireworks::fetch().await,
        Provider::DeepInfra => deepinfra::fetch().await,
        Provider::Moonshot => moonshot::fetch().await,
        Provider::Venice => venice::fetch().await,
        Provider::Codebuff => codebuff::fetch().await,
        Provider::Crof => crof::fetch().await,
        Provider::Doubao => doubao::fetch().await,
        Provider::GroqCloud => groqcloud::fetch().await,
        Provider::LlmProxy => llm_proxy::fetch().await,
        Provider::ClawRouter => clawrouter::fetch().await,
        Provider::LiteLlm => litellm::fetch().await,
        Provider::Deepgram => deepgram::fetch().await,
        Provider::Poe => poe::fetch().await,
        Provider::Chutes => chutes::fetch().await,
        Provider::NeuralWatt => neuralwatt::fetch().await,
        Provider::ZenMux => zenmux::fetch().await,
        Provider::Xai => xai::fetch().await,
        Provider::IbmBob => ibm_bob::fetch().await,
        Provider::ElevenLabs => elevenlabs::fetch().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_stubs_reject_dispatch_before_credentials_or_network() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};

        let mut context = Context::from_waker(Waker::noop());
        for (provider, expected_error) in [
            (
                Provider::AzureOpenAi,
                "Azure OpenAI read-only usage monitoring is not yet implemented",
            ),
            (
                Provider::Doubao,
                "Doubao read-only usage monitoring is not yet implemented",
            ),
        ] {
            assert!(provider.is_stub());
            let mut request = std::pin::pin!(fetch(provider));
            // Dispatch must fail on its first poll without credentials or an I/O runtime.
            match request.as_mut().poll(&mut context) {
                Poll::Ready(Err(error)) => assert_eq!(error.to_string(), expected_error),
                _ => panic!("{} must immediately reject usage polling", provider.id()),
            }
        }
    }

    #[test]
    fn api_key_env_var_uses_each_providers_canonical_secret() {
        assert_eq!(Provider::Copilot.api_key_env_var(), Some("GITHUB_TOKEN"));
        assert_eq!(
            Provider::OpenRouter.api_key_env_var(),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(
            Provider::Xai.api_key_env_var(),
            Some("XAI_MANAGEMENT_API_KEY")
        );
    }

    #[test]
    fn api_key_env_var_is_none_for_file_or_cli_authentication() {
        assert_eq!(Provider::Claude.api_key_env_var(), None);
        assert_eq!(Provider::Codex.api_key_env_var(), None);
        assert_eq!(Provider::Gemini.api_key_env_var(), None);
        assert_eq!(Provider::Kiro.api_key_env_var(), None);
    }
}
