# Ted

**AI coding assistant for your terminal.**

Ted is a fast, portable CLI tool that brings Claude's coding capabilities directly to your command line. It features a unique "caps" system for stackable AI personas, persistent context management, and powerful tool use for file operations and shell commands.

[![codecov](https://codecov.io/github/blackman-ai/ted/graph/badge.svg?token=fCa2LiHAMq)](https://codecov.io/github/blackman-ai/ted)

## Features

- **Interactive Chat** - Conversational coding assistance with streaming responses
- **Tool Use** - Read, write, and edit files; run shell commands; search with glob/grep
- **Caps System** - Stackable AI personas (rust-expert, security-analyst, etc.) that shape Claude's behavior
- **Session Management** - Automatic session history with resume capability
- **Context Persistence** - WAL-based context storage with automatic background compaction
- **Trust Mode** - Skip permission prompts for automated workflows

## Installation

### Quick Install (Recommended)

**macOS / Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/blackman-ai/ted/master/install.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/blackman-ai/ted/master/install.ps1 | iex
```

### Other Methods

**Cargo (Rust users):**
```bash
cargo install --git https://github.com/blackman-ai/ted.git
```

**From Source:**
```bash
git clone https://github.com/blackman-ai/ted.git
cd ted
cargo build --release
# Binary at target/release/ted
```

**Pre-built Binaries:**

Download from [GitHub Releases](https://github.com/blackman-ai/ted/releases):
- macOS: `ted-x86_64-apple-darwin.tar.gz` (Intel) or `ted-aarch64-apple-darwin.tar.gz` (Apple Silicon)
- Linux: `ted-x86_64-unknown-linux-gnu.tar.gz` or `ted-aarch64-unknown-linux-gnu.tar.gz`
- Windows: `ted-x86_64-pc-windows-msvc.zip`

### Requirements

- An Anthropic API key

## Quick Start

1. **Set your API key:**
   ```bash
   export ANTHROPIC_API_KEY="your-api-key"
   ```

2. **Start chatting:**
   ```bash
   ted
   ```

3. **Or ask a single question:**
   ```bash
   ted ask "How do I reverse a string in Rust?"
   ```

## Usage

### Interactive Mode

```bash
ted                           # Start interactive chat
ted chat -c rust-expert       # Start with a specific cap
ted chat -m claude-3-5-haiku-20241022  # Use a different model
ted chat --trust              # Auto-approve all tool actions
ted chat --resume abc123      # Resume a previous session
```

### Single Question Mode

```bash
ted ask "Explain this error message"
ted ask -f src/main.rs "What does this code do?"
```

### In-Chat Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/settings` | Open TUI settings editor |
| `/caps` | Show active caps |
| `/cap add <name>` | Add a cap |
| `/cap remove <name>` | Remove a cap |
| `/cap create <name>` | Create a new custom cap |
| `/model` | Show/switch model |
| `/sessions` | List recent sessions |
| `/switch <n>` | Switch to a different session |
| `/new` | Start a new session |
| `/stats` | Show context statistics |
| `/clear` | Clear conversation context |
| `exit` | Exit ted |

Press **Ctrl+C** to interrupt a running command without exiting the chat.

## Caps System

Caps are stackable AI personas that modify Claude's behavior. They can define system prompts, tool permissions, and model preferences.

### Built-in Caps

| Cap | Description |
|-----|-------------|
| `base` | Minimal defaults (always loaded) |
| `rust-expert` | Rust development best practices |
| `python-senior` | Senior Python developer |
| `typescript-expert` | TypeScript/Node.js expertise |
| `security-analyst` | Security-focused code review |
| `code-reviewer` | Thorough code review persona |
| `documentation` | Documentation writing |

### Using Caps

```bash
# Start with caps
ted chat -c rust-expert -c security-analyst

# Manage caps during chat
/cap add code-reviewer
/cap remove security-analyst
/cap set rust-expert,documentation

# Create custom caps
/cap create my-persona
ted caps create my-persona
```

### Custom Caps

Caps are TOML files stored in `~/.ted/caps/`. Example:

```toml
name = "my-persona"
description = "My custom persona"
version = "1.0.0"
priority = 10
extends = ["base"]

system_prompt = """
You are an expert in my specific domain...
"""

[tool_permissions]
enable = ["file_read", "file_edit", "shell"]
require_shell_confirmation = true
```

## Tools

Ted provides Claude with these built-in tools:

| Tool | Description |
|------|-------------|
| `file_read` | Read file contents |
| `file_write` | Create new files |
| `file_edit` | Edit existing files (find/replace) |
| `shell` | Execute shell commands |
| `glob` | Find files by pattern |
| `grep` | Search file contents |

Tools require permission by default. Use `--trust` to auto-approve, or configure per-cap permissions.

## Configuration

Settings are stored in `~/.ted/settings.json`:

```json
{
  "providers": {
    "anthropic": {
      "default_model": "claude-sonnet-4-20250514"
    }
  },
  "defaults": {
    "caps": ["base"],
    "temperature": 0.7,
    "stream": true
  },
  "context": {
    "max_warm_chunks": 100,
    "cold_retention_days": 30
  }
}
```

### Settings Commands

```bash
ted settings              # Open TUI settings editor
ted settings show         # Show current settings as JSON
ted settings set model claude-3-5-haiku-20241022
ted settings get model
ted settings reset        # Reset to defaults
```

## Session History

Ted automatically tracks your sessions:

```bash
ted history list          # List recent sessions
ted history search "auth" # Search sessions
ted history show abc123   # Show session details
ted chat --resume abc123  # Resume a session
```

## Context Management

Ted uses a WAL (Write-Ahead Log) based context system with automatic compaction:

```bash
ted context stats         # Show storage statistics
ted context usage         # Show per-session disk usage
ted context prune --days 7 --force  # Delete old sessions
ted context clear --force # Clear all context data
```

## Custom Commands

Place executable scripts in `~/.ted/commands/` or `./.ted/commands/`:

```bash
~/.ted/commands/deploy.sh    # Run with: ted deploy
./.ted/commands/test-all.py  # Project-local command
```

Scripts receive context via environment variables:
- `TED_WORKING_DIR` - Current working directory
- `TED_PROJECT_ROOT` - Detected project root
- `TED_SESSION_ID` - Current session ID
- `TED_CAPS` - Comma-separated active caps

## Project Initialization

Initialize ted in a project to enable local caps and commands:

```bash
ted init
```

This creates:
```
.ted/
  caps/       # Project-specific caps
  commands/   # Project-specific commands
  config.json # Project configuration
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Your Anthropic API key (required) |
| `TED_HOME` | Override default config directory (~/.ted) |
| `EDITOR` | Editor for `ted caps edit` |

## Models

Ted supports these Claude models:

| Model | Description |
|-------|-------------|
| `claude-sonnet-4-20250514` | Best quality, recommended (default) |
| `claude-3-5-sonnet-20241022` | Previous Sonnet, good balance |
| `claude-3-5-haiku-20241022` | Fastest, highest rate limits, cheapest |

Switch models with `/model <name>` or `-m` flag.

## Updating Ted

Ted can update itself to the latest version:

```bash
ted update            # Download and install the latest version
ted update --check    # Check for updates without installing
ted update --version v0.2.0  # Install a specific version
```

## License

AGPL-3.0-or-later

Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

## Contributing

Contributions welcome! Please read the contributing guidelines before submitting PRs.

## Support

- [GitHub Issues](https://github.com/blackman-ai/ted/issues)
- [Documentation](https://github.com/blackman-ai/ted/wiki)
