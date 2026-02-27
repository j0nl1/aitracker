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

- **21 providers** — Claude, Codex, Copilot, Gemini, Warp, OpenRouter, Kiro, JetBrains, and more
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

### Provider configuration

| Variable | Description |
|----------|-------------|
| `CODEX_HOME` | Custom Codex config directory |
| `CLAUDE_CONFIG_DIR` | Custom Claude config directory |
| `MINIMAX_API_HOST` | Custom MiniMax API host |
| `Z_AI_API_HOST` | Custom Zai API host |

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
        └── ...                 # Stub providers
```

## Development

```sh
# Run tests (231 tests)
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
