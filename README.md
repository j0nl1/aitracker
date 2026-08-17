# ait

A fast, terminal-native CLI for tracking AI provider usage, rate limits, credits, and token costs — all in one place.


```
 Claude (oauth)
  Session   72% remaining [████████░░░░]
            Resets in 2h 15m
  Weekly    41% remaining [█████░░░░░░░]
            Resets Tomorrow at 1:00 AM
  Sonnet    88% remaining [██████████░░]
  Account   user@example.com
  Plan      Pro
  Credits   $12.34 / $50.00 remaining (Monthly)
  Cost      $47.21 today | $2,971.38 (30d)

 Codex (oauth)
  Session   25% remaining [███░░░░░░░░░]
            Resets in 45m
  Cost      $8.42 today | $312.50 (30d)

 Copilot (api)
  Premium   680 / 1000 remaining [████████░░░░]
            Resets Feb 28
  Status    Operational
```

## Features

- **43 providers** — Claude, Codex, Copilot, Gemini, Warp, OpenRouter, Kiro, JetBrains, OpenAI, DeepSeek, Groq, xAI, and more
- **Rate limit tracking** — session, weekly, and model-specific windows with reset countdowns
- **Token cost analysis** — parses JSONL session logs, calculates costs per model per day
- **Credit/balance monitoring** — remaining credits, spending limits, billing periods
- **Status page polling** — live operational status for Claude, Codex, and Copilot
- **Concurrent fetching** — all providers queried in parallel via tokio
- **Incremental cost cache** — sub-second repeat scans even with gigabytes of session logs
- **JSON output** — machine-readable output for scripts and dashboards

## Installation

### From crates.io

```sh
cargo install aitracker
```

### From source

```sh
git clone https://github.com/j0nl1/aitracker.git
cargo install --path .
```

### Requirements

- Rust 1.70+
- Active credentials for the providers you want to track (OAuth tokens, API keys, etc.)

## Quick start

```sh
# Initialize config with default providers
ait config init

# Check usage for all enabled providers
ait

# Show detailed cost breakdown
ait usage --all

# Query a single provider
ait usage --provider claude

# Include health status
ait usage --status

# JSON output
ait --json
ait --json --pretty

# Install project skill (interactive or flags)
ait install-skill
```

## Commands

### `ait` / `ait usage`

Fetch and display provider usage. This is the default command.

```
ait usage [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-p, --provider <ID>` | Query a specific provider (default: all enabled) |
| `-a, --all` | Show detailed cost breakdown (by-model + daily) |
| `--source <MODE>` | Override auth source (`auto`, `oauth`, `cli`, `api`) |
| `--status` | Include provider health status |

### `ait config`

Manage configuration.

```
ait config init              # Generate default config file
ait config edit              # Edit enabled providers interactively
ait config check             # Validate existing config
ait config add <provider>    # Enable a provider (non-interactive)
ait config remove <provider> # Disable a provider (non-interactive)
```

### `ait install-skill`

Install the embedded `./.skill` directory into your agents skills directory.

```sh
ait install-skill [--source <path>] [--providers <csv|*>] [--scope <project|user>] [--method <symlink|copy>] [--project-root <path>] [--force]
```

### Global flags

| Flag | Description |
|------|-------------|
| `-f, --format <FMT>` | Output format (`text`, `json`) |
| `-j, --json` | Shorthand for `--format json` |
| `--pretty` | Pretty-print JSON output |
| `--no-color` | Disable ANSI colors |
| `-v, --verbose` | Verbose logging to stderr |

## Providers

### Fully supported

| Provider | ID | Auth method | What it tracks |
|----------|----|-------------|----------------|
| Claude | `claude` | OAuth (auto-discovered) | Session/weekly/Sonnet rate limits, monthly credits, token costs |
| Codex | `codex` | OAuth (auto-discovered) | Session/weekly rate limits, credit balance, token costs |
| Copilot | `copilot` | `GITHUB_TOKEN` or `gh auth token` | Premium/chat quotas, monthly remaining |
| Warp | `warp` | `WARP_TOKEN` | Request limits, bonus grants |
| Kimi | `kimi` | `KIMI_TOKEN` | Usage limits, remaining credits |
| Kimi K2 | `kimi_k2` | `KIMI_K2_API_KEY` | Credit balance |
| OpenRouter | `openrouter` | `OPENROUTER_API_KEY` | Total credits, usage, rate limits |
| MiniMax | `minimax` | `MINIMAX_API_TOKEN` | Per-model plan usage |
| Zai | `zai` | `Z_AI_API_KEY` | Token and time limits |
| Gemini | `gemini` | OAuth (auto-discovered) | Pro/Flash model quotas |
| JetBrains | `jetbrains` | Local IDE config files | AI quota from IDE settings |
| Kiro | `kiro` | `kiro-cli` subprocess | Credits percentage, usage |
| Antigravity | `antigravity` | Auto-detected language server | Model quota info |
| Synthetic | `synthetic` | `SYNTHETIC_API_KEY` | Multiple quota entries |
| Vertex AI | `vertex_ai` | — | Token costs (detected from Claude session logs) |
| OpenAI | `openai` | `OPENAI_API_KEY` | Legacy credit-grant balance |
| Azure OpenAI | `azure_openai` | `AZURE_OPENAI_API_KEY` + endpoint + deployment | Deployment reachability probe (no billing data) |
| DeepSeek | `deepseek` | `DEEPSEEK_API_KEY` | Account balance |
| Fireworks | `fireworks` | `FIREWORKS_API_KEY` + `FIREWORKS_ACCOUNT_SLUG` | Last-30-day rated spend |
| DeepInfra | `deepinfra` | `DEEPINFRA_API_KEY` | Prepaid balance, current-month spend, spending limit |
| Moonshot | `moonshot` | `MOONSHOT_API_KEY` (+ `MOONSHOT_REGION`) | Account balance |
| Venice | `venice` | `VENICE_API_KEY` | DIEM or USD balance |
| Codebuff | `codebuff` | `CODEBUFF_API_KEY` | Credit balance |
| Crof | `crof` | `CROF_API_KEY` | Dollar credits + daily request quota |
| Doubao | `doubao` | `ARK_API_KEY` | Ark request-limit probe |
| GroqCloud | `groqcloud` | `GROQ_API_KEY` | Enterprise Prometheus request/token rates |
| LLM Proxy | `llm_proxy` | `LLM_PROXY_API_KEY` + `LLM_PROXY_BASE_URL` | Aggregate proxy quota stats |
| ClawRouter | `clawrouter` | `CLAWROUTER_API_KEY` | Policy budget, spend, routed-provider usage |
| LiteLLM | `litellm` | `LITELLM_API_KEY` + `LITELLM_BASE_URL` | Personal/team budget and spend |
| Deepgram | `deepgram` | `DEEPGRAM_API_KEY` | Usage across speech/agent/token/TTS metrics |
| Poe | `poe` | `POE_API_KEY` | Current point balance |
| Chutes | `chutes` | `CHUTES_API_KEY` | Rolling and monthly quota windows |
| NeuralWatt | `neuralwatt` | `NEURALWATT_API_KEY` | Prepaid credit balance |
| ZenMux | `zenmux` | `ZENMUX_MANAGEMENT_API_KEY` | 5-hour/7-day quota windows + PAYG balance |
| xAI | `xai` | `XAI_MANAGEMENT_API_KEY` + `XAI_TEAM_ID` | Prepaid credit balance |
| IBM Bob | `ibm_bob` | `BOBSHELL_API_KEY` | Monthly Bobcoin budget across teams |
| ElevenLabs | `elevenlabs` | `ELEVENLABS_API_KEY` | Character credits + voice slot usage |

### Planned

| Provider | ID | Status |
|----------|----|--------|
| Cursor | `cursor` | Requires browser cookies |
| Ollama | `ollama` | Requires browser cookies |
| Augment | `augment` | Requires browser cookies |
| OpenCode | `opencode` | Requires browser cookies |
| Factory | `factory` | Requires browser cookies |
| Amp | `amp` | Requires browser cookies |

## Configuration

Config lives at `~/.config/ait/config.toml` (respects `$XDG_CONFIG_HOME`).

```toml
[settings]
default_format = "text"   # "text" or "json"
color = "auto"            # "auto", "always", or "never"

[[providers]]
id = "claude"
enabled = true
source = "auto"           # "auto", "oauth", "cli", "api"

[[providers]]
id = "codex"
enabled = true

[[providers]]
id = "copilot"
enabled = false

[[providers]]
id = "openrouter"
enabled = false
```

Run `ait config init` to generate a default config, then enable/disable providers with `ait config add <id>` / `ait config remove <id>` or interactively with `ait config edit`.

## Token cost scanning

`ait` parses JSONL session logs from Claude Code and Codex to calculate per-model, per-day token costs.

**Supported log locations:**

- Claude: `~/.claude/projects/*/*.jsonl` (and subagent dirs)
- Codex: `~/.codex/sessions/**/*.jsonl` (YYYY/MM/DD structure)

**How it works:**

1. Discovers all JSONL session files
2. Parses usage records (input/output/cache tokens per model per day)
3. Applies built-in pricing tables to compute costs
4. Caches results — only re-parses changed files on subsequent runs

The cache lives at `~/.cache/ait/cost-cache.json`. First scan of large session directories may take several seconds; subsequent runs are near-instant.

**Vertex AI detection:** Requests routed through Vertex AI are automatically identified (via `_vrtx_` markers or `@` in model names) and attributed to the Vertex AI provider.

## Environment variables

### Provider authentication

| Variable | Provider |
|----------|----------|
| `GITHUB_TOKEN` | Copilot |
| `WARP_TOKEN` | Warp |
| `KIMI_TOKEN` | Kimi |
| `KIMI_K2_API_KEY` | Kimi K2 |
| `OPENROUTER_API_KEY` | OpenRouter |
| `MINIMAX_API_TOKEN` | MiniMax |
| `Z_AI_API_KEY` | Zai |
| `SYNTHETIC_API_KEY` | Synthetic |
| `OPENAI_API_KEY` | OpenAI |
| `AZURE_OPENAI_API_KEY` | Azure OpenAI |
| `DEEPSEEK_API_KEY` | DeepSeek |
| `FIREWORKS_API_KEY` | Fireworks |
| `DEEPINFRA_API_KEY` | DeepInfra |
| `MOONSHOT_API_KEY` | Moonshot |
| `VENICE_API_KEY` | Venice |
| `CODEBUFF_API_KEY` | Codebuff |
| `CROF_API_KEY` | Crof |
| `ARK_API_KEY` | Doubao |
| `GROQ_API_KEY` | GroqCloud |
| `LLM_PROXY_API_KEY` | LLM Proxy |
| `CLAWROUTER_API_KEY` | ClawRouter |
| `LITELLM_API_KEY` | LiteLLM |
| `DEEPGRAM_API_KEY` | Deepgram |
| `POE_API_KEY` | Poe |
| `CHUTES_API_KEY` | Chutes |
| `NEURALWATT_API_KEY` | NeuralWatt |
| `ZENMUX_MANAGEMENT_API_KEY` | ZenMux |
| `XAI_MANAGEMENT_API_KEY` | xAI |
| `BOBSHELL_API_KEY` | IBM Bob |
| `ELEVENLABS_API_KEY` | ElevenLabs |

### Provider configuration

| Variable | Description |
|----------|-------------|
| `CODEX_HOME` | Custom Codex config directory |
| `CLAUDE_CONFIG_DIR` | Custom Claude config directory |
| `MINIMAX_API_HOST` | Custom MiniMax API host |
| `Z_AI_API_HOST` | Custom Zai API host |
| `AZURE_OPENAI_ENDPOINT` | Azure OpenAI resource endpoint (required) |
| `AZURE_OPENAI_DEPLOYMENT_NAME` | Azure OpenAI deployment name (required) |
| `AZURE_OPENAI_API_VERSION` | Azure OpenAI API version (default `2024-10-21`) |
| `FIREWORKS_ACCOUNT_SLUG` | Fireworks account slug (required) |
| `MOONSHOT_REGION` | `international` (default) or `china` |
| `GROQ_API_URL` | Custom GroqCloud API base URL |
| `LLM_PROXY_BASE_URL` | Self-hosted LLM Proxy base URL (required) |
| `CLAWROUTER_BASE_URL` | Self-hosted ClawRouter base URL |
| `LITELLM_BASE_URL` | Self-hosted LiteLLM proxy base URL (required) |
| `DEEPGRAM_PROJECT_ID` | Restrict Deepgram usage to a single project |
| `DEEPGRAM_API_URL` | Custom Deepgram API base URL |
| `CHUTES_API_URL` | Custom Chutes API base URL |
| `NEURALWATT_API_URL` | Custom NeuralWatt API base URL |
| `XAI_TEAM_ID` | xAI team ID (required) |
| `ELEVENLABS_API_URL` | Custom ElevenLabs API base URL |

### General

| Variable | Description |
|----------|-------------|
| `XDG_CONFIG_HOME` | Config directory (default: `~/.config`) |
| `XDG_CACHE_HOME` | Cache directory (default: `~/.cache`) |
| `NO_COLOR` | Disable colors ([standard](https://no-color.org/)) |

## Project structure

```
src/
├── main.rs                     # CLI entry point (clap)
├── cli/
│   ├── usage_cmd.rs            # Provider dispatch + concurrent fetch
│   ├── config_cmd.rs           # Config init/edit/check/add/remove
│   ├── selector.rs             # Interactive provider selector
│   ├── renderer.rs             # Text output with color bars
│   └── output.rs               # Output format detection
└── core/
    ├── config.rs               # TOML config parsing
    ├── auth.rs                 # OAuth/JWT credential reading
    ├── formatter.rs            # Percent bars, countdowns, credits
    ├── status.rs               # Statuspage.io polling
    ├── process.rs              # Subprocess runner
    ├── models/
    │   ├── usage.rs            # UsageSnapshot, RateWindow
    │   ├── credits.rs          # CreditsSnapshot
    │   ├── cost.rs             # CostSummary, TokenCostSnapshot
    │   └── status.rs           # StatusInfo, StatusIndicator
    ├── cost/
    │   ├── scanner.rs          # JSONL parsing + cost calculation
    │   ├── pricing.rs          # Per-model pricing tables
    │   └── cache.rs            # Incremental scan cache
    └── providers/
        ├── claude.rs           # Anthropic OAuth API
        ├── codex.rs            # OpenAI/ChatGPT API
        ├── copilot.rs          # GitHub Copilot API
        ├── gemini.rs           # Google Gemini OAuth
        ├── warp.rs             # Warp GraphQL
        ├── openrouter.rs       # OpenRouter REST API
        ├── kimi.rs             # Kimi billing API
        ├── kimi_k2.rs          # Kimi K2 credits API
        ├── minimax.rs          # MiniMax OpenPlatform
        ├── zai.rs              # Zai quota API
        ├── jetbrains.rs        # JetBrains IDE config
        ├── kiro.rs             # Kiro CLI subprocess
        ├── antigravity.rs      # Antigravity language server
        ├── synthetic.rs        # Synthetic quotas API
        ├── vertex_ai.rs        # Vertex AI (stub)
        ├── openai.rs           # OpenAI legacy credit-grant balance
        ├── deepseek.rs         # DeepSeek balance API
        ├── groqcloud.rs        # GroqCloud Prometheus metrics
        ├── xai.rs              # xAI Management API
        └── ...                 # 20+ more API-key providers, plus stub providers
```

## Development

```sh
# Run tests (311 tests)
cargo test

# Build release binary
cargo build --release

# Run directly
cargo run -- usage --provider claude
cargo run -- usage --all
```

### Adding a provider

1. Create `src/core/providers/<name>.rs` with a `pub async fn fetch() -> Result<FetchResult>`
2. Add the variant to `Provider` enum in `src/core/providers/mod.rs`
3. Wire it into `dispatch_fetch()` in `src/cli/usage_cmd.rs`
4. Add a default config entry in `src/core/config.rs`

## Acknowledgements

Inspired by [CodexBar](https://github.com/steipete/CodexBar), built for environments where a macOS menu bar isn't available — VMs, remote servers, SSH sessions, and headless setups.

## License

MIT
