# Changelog

All notable changes to Ted will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-12-13

### Initial Release

Ted is an AI coding assistant for your terminal, featuring a unique "caps" system for stackable AI personas, persistent context management, and powerful tool use.

### Added

#### Core Features

- **Interactive Chat Mode** - Conversational coding assistance with streaming responses
- **Single Question Mode** (`ted ask`) - Quick one-off questions without entering interactive mode
- **Trust Mode** (`--trust`) - Auto-approve all tool actions for automated workflows

#### Caps System

- Stackable AI personas that modify Ted's behavior
- Built-in caps: `base`, `rust-expert`, `python-senior`, `typescript-expert`, `security-analyst`, `code-reviewer`, `documentation`
- Custom cap creation with TOML files in `~/.ted/caps/`
- Per-cap tool permissions and model preferences
- Priority-based cap stacking with inheritance (`extends`)

#### Built-in Tools

- `file_read` - Read file contents with line number display
- `file_write` - Create new files
- `file_edit` - Edit existing files using find/replace operations
- `shell` - Execute shell commands with output capture
- `glob` - Find files by pattern matching
- `grep` - Search file contents with regex support

#### External Tools System

- Register external scripts (Node, Python, shell) as first-class tools
- JSON-RPC protocol for tool communication
- Automatic recall event integration for memory system

#### Context Management

- **Three-tier storage system**:
  - Hot tier (WAL): In-memory cache for recent entries
  - Warm tier (filesystem): Individual JSON chunk files
  - Cold tier (compressed): zstd-compressed archives for older data
- Automatic background compaction
- Priority-based retention (Critical, High, Normal, Low)
- Token counting across all storage tiers

#### Memory-Based Indexer

- **Human-like memory model**: Files decay over time but get reinforced through access
- **Scoring formula**: Combines recency (40%), frequency (30%), and centrality (30%)
- **Dependency graph analysis**: Tracks imports/exports across files
- **Git integration**: Analyzes commit history and churn rates
- **Multi-language support**:
  - Rust (use, mod, pub declarations)
  - TypeScript/JavaScript (ES modules, CommonJS, dynamic imports)
  - Python (import, from...import, relative imports)
  - Go (import statements, exported symbols)
  - Generic fallback for other languages

#### File Tree Awareness

- Automatic project structure scanning at session start
- Stored as "core memory" chunk that never gets compacted
- Configurable ignore patterns (node_modules, target, .git, etc.)
- Max depth and file limits to handle large projects

#### Session Management

- Automatic session tracking with UUID-based identification
- Session resume capability (`--resume`)
- Directory-based session lookup (recent sessions in current directory)
- Session history search and listing

#### TUI Settings Editor

- Interactive terminal UI for configuration
- Model selection and provider settings
- Default caps configuration
- Keyboard navigation with vim-style bindings

#### Self-Update System

- `ted update` to download and install latest version
- Version checking (`--check`)
- Specific version installation (`--version`)
- Platform-specific binary downloads (macOS, Linux, Windows)

#### Custom Commands

- Place scripts in `~/.ted/commands/` or `./.ted/commands/`
- Automatic discovery and execution
- Environment variables for context (TED_WORKING_DIR, TED_PROJECT_ROOT, etc.)

#### Project Initialization

- `ted init` creates local `.ted/` directory
- Project-specific caps and commands
- Local configuration override

### In-Chat Commands

- `/help` - Show available commands
- `/settings` - Open TUI settings editor
- `/caps` - Show active caps
- `/cap add|remove|set|create` - Manage caps
- `/model` - Show/switch model
- `/sessions` - List recent sessions
- `/switch` - Switch to a different session
- `/new` - Start a new session
- `/stats` - Show context statistics
- `/clear` - Clear conversation context

### Technical Details

#### Supported Models

- `claude-sonnet-4-20250514` (default)
- `claude-3-5-sonnet-20241022`
- `claude-3-5-haiku-20241022`

#### Platforms

- macOS (x86_64, aarch64)
- Linux (x86_64, aarch64)
- Windows (x86_64)

#### Configuration

- Settings stored in `~/.ted/settings.json`
- Caps stored in `~/.ted/caps/`
- Commands stored in `~/.ted/commands/`
- Context data stored in `~/.ted/context/`
- Session history stored in `~/.ted/history/`

### License

AGPL-3.0-or-later

---

[0.1.0]: https://github.com/blackman-ai/ted/releases/tag/v0.1.0
