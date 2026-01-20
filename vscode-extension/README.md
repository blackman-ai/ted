# Ted - AI Coding Assistant for VS Code

Ted is an AI-powered coding assistant that can edit files, run commands, and help you build applications.

## Features

- **Chat Interface**: Ask Ted questions about your code or request changes
- **Code Selection**: Select code and ask Ted to explain or modify it
- **File Operations**: Ted can create, edit, and delete files in your workspace
- **Multiple AI Providers**: Supports Anthropic Claude, Ollama (local), and OpenRouter

## Requirements

- Ted CLI must be installed. Install it with:
  ```bash
  curl -fsSL https://raw.githubusercontent.com/blackman-ai/ted/main/install.sh | bash
  ```

- For Anthropic Claude, set your API key:
  - In VS Code settings: `ted.anthropicApiKey`
  - Or as environment variable: `ANTHROPIC_API_KEY`

- For Ollama (local), ensure Ollama is running at `http://localhost:11434`

## Usage

### Open Chat
- Click the Ted icon in the activity bar (sidebar)
- Or use `Cmd+Shift+T` (Mac) / `Ctrl+Shift+T` (Windows/Linux)

### Ask About Code
1. Select some code in the editor
2. Right-click and choose "Ted: Ask about Selection"
3. Or use `Cmd+Shift+A` (Mac) / `Ctrl+Shift+A` (Windows/Linux)

### Edit Code
1. Open a file you want to modify
2. Optionally select specific code to change
3. Right-click and choose "Ted: Edit Code"
4. Or use `Cmd+Shift+E` (Mac) / `Ctrl+Shift+E` (Windows/Linux)

## Configuration

Open VS Code settings and search for "Ted" to configure:

| Setting | Description | Default |
|---------|-------------|---------|
| `ted.provider` | AI provider to use | `anthropic` |
| `ted.anthropicApiKey` | Anthropic API key | (empty) |
| `ted.anthropicModel` | Anthropic model | `claude-sonnet-4-20250514` |
| `ted.ollamaBaseUrl` | Ollama server URL | `http://localhost:11434` |
| `ted.ollamaModel` | Ollama model | `qwen2.5-coder:7b` |
| `ted.openrouterApiKey` | OpenRouter API key | (empty) |
| `ted.openrouterModel` | OpenRouter model | `anthropic/claude-3.5-sonnet` |
| `ted.trustMode` | Allow changes without confirmation | `false` |
| `ted.tedBinaryPath` | Custom path to ted binary | (auto-detected) |

## Commands

| Command | Keybinding | Description |
|---------|------------|-------------|
| Ted: Open Chat | `Cmd+Shift+T` | Open the Ted chat panel |
| Ted: Ask about Selection | `Cmd+Shift+A` | Ask Ted about selected code |
| Ted: Edit Code | `Cmd+Shift+E` | Ask Ted to edit the current file |
| Ted: Stop Current Task | - | Stop Ted if it's running |
| Ted: Clear Conversation History | - | Clear the chat history |
| Ted: Set AI Provider | - | Change the AI provider |

## License

AGPL-3.0-or-later - Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.
