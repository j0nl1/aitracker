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

### `ait config edit <provider>`

Configure one provider interactively. Prompts whether the provider is enabled. For providers requiring an API key, choose to enter it using a masked prompt, use the process environment (which removes any stored value), or retain the existing stored value. Stored keys live in `$XDG_CONFIG_HOME/ait/.env` (default `~/.config/ait/.env`) with owner-only permissions on Unix.

### `ait config list`

List every provider ID and whether it is enabled. `--json` includes `id`, `enabled`, and `supported` fields.

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

### `ait serve`

Start the loopback-only read-only usage broker. `AIT_BROKER_TOKEN` is required and must be at least 32 bytes. The broker loads provider credentials, refreshes enabled providers concurrently, and exposes sanitized usage at `/v1/usage`. It never publishes provider identity, credential source, or raw provider errors.

```sh
AIT_BROKER_TOKEN=<strong-random-token> ait serve
ait serve --bind 127.0.0.1:7843 --refresh-seconds 60 --provider-timeout-seconds 30
```

### `ait client usage`

Query a running broker. This command reads `AIT_BROKER_TOKEN` only from the process environment and deliberately does not load the provider-secret `.env`.

```sh
AIT_BROKER_TOKEN=<token> ait --json client usage
AIT_BROKER_TOKEN=<token> ait --json client usage --provider openrouter
```

Run the broker under a separate OS user, or in a sidecar/container that shares loopback networking but not its filesystem or environment with untrusted agent harnesses. Give the agent only the broker token and access to this client command. A same-user broker is not a credential isolation boundary because the agent may be able to read the broker's config and OAuth files directly.

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

Run `ait --json config list` for the complete provider catalog, enabled state, and support status. Azure OpenAI and Doubao are unavailable until read-only usage collection is implemented; they do not send inference probes.

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

## Configuration

Config file: `~/.config/ait/config.toml` (or `$XDG_CONFIG_HOME/ait/config.toml`)

Provider secrets file: `~/.config/ait/.env` (or `$XDG_CONFIG_HOME/ait/.env`). Usage commands load it without overriding variables already present in the process environment. The file is plaintext and must not be committed or shared; `ait` enforces mode `0600` on Unix when it writes the file.

```toml
[settings]
default_format = "text"   # "text" or "json"
color = "auto"            # "auto", "always", or "never"

[[providers]]
id = "claude"
enabled = true
source = "auto"           # "auto", "oauth", "cli", or "api"
```

## Common Workflows

```sh
# First-time setup
ait config init          # creates config with auto-detected providers
ait                      # verify it works

# Toggle providers on/off
ait config edit          # interactive selector
ait config list          # list provider IDs and enabled state
ait config edit copilot  # enable/disable and configure GITHUB_TOKEN
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
| `Z_AI_QUOTA_URL`    | Deprecated and ignored; use `Z_AI_API_HOST` |
| `AIT_BROKER_TOKEN`  | Broker/client bearer token (32+ bytes)   |
