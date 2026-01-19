# Model Context Protocol (MCP) Support

Ted now includes full support for the Model Context Protocol (MCP), allowing external MCP clients like Claude Desktop to use Ted's tools directly.

## What is MCP?

The Model Context Protocol is an open standard that enables AI assistants to connect to external tools and data sources. With MCP, Claude Desktop and other compatible clients can leverage Ted's powerful built-in tools without needing to run Ted's full CLI interface.

## Features

✅ **Full protocol support**: Implements MCP version 2024-11-05
✅ **All built-in tools exposed**: File operations, shell, search, database tools
✅ **JSON-RPC 2.0 transport**: Standard stdio-based communication
✅ **Zero configuration**: Works out of the box with Claude Desktop
✅ **Project-aware**: Can be scoped to specific project directories

## Quick Start

### 1. Install Ted

```bash
cargo build --release
sudo cp target/release/ted /usr/local/bin/
```

### 2. Configure Claude Desktop

Add Ted to your Claude Desktop configuration file:

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
**Linux**: `~/.config/Claude/claude_desktop_config.json`
**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "ted": {
      "command": "ted",
      "args": ["mcp"]
    }
  }
}
```

### 3. Restart Claude Desktop

After restarting, Claude Desktop will automatically connect to Ted's MCP server and gain access to all tools.

## Available Tools

Ted exposes 11 built-in tools through MCP:

| Tool | Description |
|------|-------------|
| `file_read` | Read file contents with line numbers |
| `file_write` | Create or overwrite files |
| `file_edit` | Edit files with find/replace operations |
| `shell` | Execute shell commands |
| `glob` | Find files matching patterns (e.g., `**/*.rs`) |
| `grep` | Search file contents for patterns |
| `plan_update` | Update planning documents |
| `database_init` | Initialize Prisma database schema |
| `database_migrate` | Run database migrations |
| `database_query` | Execute SQL queries |
| `database_seed` | Run database seed scripts |

## Usage Examples

Once configured, Claude Desktop can use Ted's tools naturally in conversation:

**Example 1: Reading files**
```
User: Read the main.rs file
Claude: [Uses file_read tool to fetch src/main.rs]
```

**Example 2: Running commands**
```
User: What Node.js version is installed?
Claude: [Uses shell tool to run: node --version]
```

**Example 3: Searching code**
```
User: Find all TODO comments in the codebase
Claude: [Uses grep tool to search for: TODO]
```

## Project-Specific Configuration

To run Ted MCP server for a specific project:

```json
{
  "mcpServers": {
    "my-project": {
      "command": "ted",
      "args": ["mcp", "--project", "/path/to/my/project"]
    }
  }
}
```

This ensures all file operations and commands run within the specified project directory.

## Architecture

```
┌─────────────────┐
│ Claude Desktop  │
└────────┬────────┘
         │ JSON-RPC 2.0
         │ over stdio
┌────────▼────────┐
│  Ted MCP Server │
├─────────────────┤
│ • Protocol      │
│ • Transport     │
│ • Tool Registry │
└────────┬────────┘
         │
┌────────▼────────┐
│  Ted Tools      │
├─────────────────┤
│ • File ops      │
│ • Shell exec    │
│ • Search        │
│ • Database      │
└─────────────────┘
```

The MCP server:
1. Listens on stdin for JSON-RPC requests
2. Translates MCP tool calls to Ted tool invocations
3. Executes tools with proper context (project dir, permissions)
4. Returns results via stdout as JSON-RPC responses

## Security

- **Same permissions as user**: MCP server runs with the same file system permissions as Claude Desktop
- **No sandbox**: Shell commands execute with full user privileges
- **Project scoping**: When using `--project`, operations are constrained to that directory
- **No network access**: MCP server only communicates via stdio, no network ports opened

⚠️ **Warning**: Claude Desktop will have full access to your file system through Ted's tools. Only use with projects and files you trust.

## Debugging

### Test the server manually

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | ted mcp
```

Expected output:
```json
{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"tools":{"list_changed":false}},"protocolVersion":"2024-11-05","serverInfo":{"name":"ted","version":"0.1.1"}}}
```

### List available tools

```bash
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | ted mcp 2>/dev/null
```

### Check Claude Desktop logs

Look for MCP-related errors in Claude Desktop's application logs. The MCP server writes debug output to stderr.

## Troubleshooting

**Problem**: Tools not showing up in Claude Desktop

**Solutions**:
- Verify `ted` is in your PATH: `which ted`
- Check the config file path is correct for your OS
- Ensure JSON syntax is valid (use a JSON validator)
- Restart Claude Desktop after config changes

**Problem**: Permission denied errors

**Solutions**:
- Check Ted has execute permissions: `chmod +x $(which ted)`
- Verify project directory (if specified) exists and is readable
- Ensure you have permissions for the operations Claude is attempting

**Problem**: Server connection errors

**Solutions**:
- Test the MCP server manually (see Debugging section)
- Check that Ted runs: `ted --version`
- Look for error messages in Claude Desktop logs
- Verify no conflicting MCP servers with the same name

## Comparison with Claude Code

| Feature | Ted MCP | Claude Code |
|---------|---------|-------------|
| **File operations** | ✅ Full support | ✅ Full support |
| **Shell execution** | ✅ Full support | ✅ Full support |
| **Database tools** | ✅ Prisma + SQLite | ❌ Not available |
| **Project awareness** | ✅ Via --project flag | ✅ Via working directory |
| **Tool permissions** | 🔶 User-level only | ✅ Per-tool approval |
| **Conversation history** | ❌ Not available | ✅ Full history |
| **Installation** | Requires Ted install | Built into Claude Desktop |

Ted's MCP server is ideal for:
- Using Ted's database tools in Claude Desktop
- Integrating Ted's caps system with Claude
- Working with projects already set up for Ted
- Advanced users who want more control over tool execution

## Learn More

- [MCP Specification](https://modelcontextprotocol.io/)
- [Ted Documentation](../README.md)
- [Claude Desktop](https://claude.ai/desktop)

## Contributing

Found a bug or want to add features to the MCP server? Contributions welcome!

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.
