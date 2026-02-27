# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2] - 2026-02-27

### Added

- New `ait install-skill` command to install the embedded project skill (`.skill/`) into agents skills directories, with interactive and flag-driven flows via `skillinstaller`.

### Changed

- Updated `.skill/SKILL.md` to include valid YAML front matter (`name` and `description`) for proper skill metadata parsing.
- Updated README with usage docs for `ait install-skill`.

## [0.2.1] - 2026-02-27

### Security

- **Endpoint validation**: All providers with configurable endpoints (Codex, MiniMax, Zai) now enforce HTTPS, preventing credential exfiltration over plain HTTP or other schemes.
- **Antigravity TLS guard**: Added localhost assertion before disabling TLS certificate verification, preventing regressions if the hardcoded URL is ever changed.
- **Kiro subprocess**: Replaced `Command::new("which")` with the project's own `process::which()` utility, eliminating an unnecessary subprocess call.

### Deprecated

- `Z_AI_QUOTA_URL` environment variable is now **deprecated and ignored**. Use `Z_AI_API_HOST` instead to customize the Zai API host. The full URL override was removed because it allowed sending credentials to arbitrary endpoints.

### Changed

- Custom endpoint overrides now print a warning to stderr when active (Codex, MiniMax, Zai).

## [0.2.0] - 2025-07-15

Initial public release.
