---
name: aitracker
description: Use this skill when you need commands and workflows for the `ait` CLI to track AI provider usage, limits, credits, and token costs.
---

# ait — AI Provider Usage Tracking CLI

`ait` is a terminal CLI for tracking AI provider usage, rate limits, credits, and token costs. The binary is built with `cargo build` from the `ait/` crate directory.

## Quick Reference

```sh
# Default command — fetch usage for all enabled providers
ait

# Equivalent explicit form
ait usage

# Query a single provider
ait usage --provider claude
ait usage -p codex

# Detailed cost breakdown (by-model + daily)
ait usage --all
ait usage -a

# Include health/status info
ait usage --status

# Override auth source
ait usage --source oauth

# JSON output
ait --json
ait --json --pretty

# Disable colors
ait --no-color

# Verbose logging (to stderr)
ait -v
```

## Commands

### `ait` / `ait usage`

Fetches and displays provider usage. This is the default when no subcommand is given.

| Flag              | Short | Description                                            |
| ----------------- | ----- | ------------------------------------------------------ |
| `--provider <ID>` | `-p`  | Query a specific provider instead of all enabled       |
| `--all`           | `-a`  | Show detailed cost breakdown (by-model + daily totals) |
| `--source <MODE>` |       | Override auth source: `auto`, `oauth`, `cli`, `api`    |
| `--status`        |       | Include provider health status from statuspage.io      |

### `ait config init`

Generate the config file at `~/.config/ait/config.toml` (respects `$XDG_CONFIG_HOME`). Opens an interactive provider selector in TTY mode; in non-TTY mode auto-enables providers with detected credentials.

### `ait config edit`

Re-open the interactive provider selector with currently-enabled providers pre-checked. Updates the config in place, preserving `settings`, `source`, and `api_key` fields.

### `ait config add <provider>`

Enable a provider non-interactively. Creates the config file if it doesn't exist. Rejects unknown IDs and stub (not-yet-supported) providers.

```sh
ait config add gemini       # → "Enabled provider: gemini"
ait config add fakeprovider # → "Unknown provider: fakeprovider" (exit 1)
ait config add cursor       # → "Provider 'cursor' is not yet supported (stub)" (exit 1)
ait config add gemini       # (already enabled) → "Provider 'gemini' is already enabled" (exit 1)
```

### `ait config remove <provider>`

Disable a provider non-interactively. Rejects unknown IDs, stubs, and already-disabled providers.

```sh
ait config remove gemini    # → "Disabled provider: gemini"
ait config remove gemini    # (already disabled) → "Provider 'gemini' is already disabled" (exit 1)
```

### `ait config check`

Validate the existing config file and list any issues.

## Global Flags

These flags work with any command:

| Flag             | Short | Description                             |
| ---------------- | ----- | --------------------------------------- |
| `--format <FMT>` | `-f`  | Output format: `text` (default), `json` |
| `--json`         | `-j`  | Shorthand for `--format json`           |
| `--pretty`       |       | Pretty-print JSON output                |
| `--no-color`     |       | Disable ANSI colors                     |
| `--verbose`      | `-v`  | Verbose logging to stderr               |

## Provider IDs

Use these IDs with `--provider`:

| ID            | Provider    | Auth                                  |
| ------------- | ----------- | ------------------------------------- |
| `claude`      | Claude      | OAuth (`~/.claude/.credentials.json`) |
| `codex`       | Codex       | OAuth (`~/.codex/auth.json`)          |
| `copilot`     | Copilot     | `GITHUB_TOKEN` or `gh auth token`     |
| `gemini`      | Gemini      | OAuth (`~/.gemini/oauth_creds.json`)  |
| `warp`        | Warp        | `WARP_TOKEN`                          |
| `kimi`        | Kimi        | `KIMI_TOKEN`                          |
| `kimi_k2`     | Kimi K2     | `KIMI_K2_API_KEY`                     |
| `openrouter`  | OpenRouter  | `OPENROUTER_API_KEY`                  |
| `minimax`     | MiniMax     | `MINIMAX_API_TOKEN`                   |
| `zai`         | Zai         | `Z_AI_API_KEY`                        |
| `kiro`        | Kiro        | `kiro-cli` subprocess                 |
| `jetbrains`   | JetBrains   | Local IDE config                      |
| `antigravity` | Antigravity | Language server auto-detection        |
| `synthetic`   | Synthetic   | `SYNTHETIC_API_KEY`                   |
| `vertex_ai`   | Vertex AI   | Detected from Claude session logs     |

## Configuration

Config file: `~/.config/ait/config.toml` (or `$XDG_CONFIG_HOME/ait/config.toml`)

```toml
[settings]
default_format = "text"   # "text" or "json"
color = "auto"            # "auto", "always", or "never"

[[providers]]
id = "claude"
enabled = true
source = "auto"           # "auto", "oauth", "cli", or "api"
# api_key = "sk-..."      # optional, overrides auto-detection
```

## Common Workflows

```sh
# First-time setup
ait config init          # creates config with auto-detected providers
ait                      # verify it works

# Toggle providers on/off
ait config edit          # interactive selector
ait config add gemini    # scriptable: enable one provider
ait config remove gemini # scriptable: disable one provider

# Check a single provider quickly
ait -p claude

# Get machine-readable output for scripting
ait --json --pretty

# Full cost breakdown
ait usage --all

# Debug auth issues
ait -v -p claude         # verbose stderr output

# Validate after manual config edits
ait config check
```

## Environment Variables

| Variable            | Purpose                                  |
| ------------------- | ---------------------------------------- |
| `XDG_CONFIG_HOME`   | Config directory (default: `~/.config`)  |
| `XDG_CACHE_HOME`    | Cache directory (default: `~/.cache`)    |
| `NO_COLOR`          | Disable colors (standard)                |
| `CLAUDE_CONFIG_DIR` | Custom Claude config directory           |
| `CODEX_HOME`        | Custom Codex config directory            |
| `MINIMAX_API_HOST`  | Custom MiniMax API host                  |
| `Z_AI_API_HOST`     | Custom Zai API host                      |
| `Z_AI_QUOTA_URL`    | Full URL override for Zai quota endpoint |
