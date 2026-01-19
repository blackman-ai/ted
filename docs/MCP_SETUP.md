# MCP Server Setup

Ted now includes a Model Context Protocol (MCP) server that exposes Ted's built-in tools to external MCP clients like Claude Desktop.

## What is MCP?

The Model Context Protocol (MCP) is an open standard for connecting AI assistants to external tools and data sources. It allows Claude Desktop and other MCP clients to use Ted's tools (file operations, shell commands, search, etc.) directly.

## Quick Start

1. **Build Ted** (if not already installed):
   ```bash
   cargo build --release
   sudo cp target/release/ted /usr/local/bin/
   ```

2. **Configure Claude Desktop**:

   Add Ted as an MCP server in your Claude Desktop configuration file:

   **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
   **Linux**: `~/.config/Claude/claude_desktop_config.json`
   **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

   ```json
   {
     "mcpServers": {
       "ted": {
         "command": "ted",
         "args": ["mcp"],
         "env": {}
       }
     }
   }
   ```

3. **Restart Claude Desktop**

4. **Verify**: Claude Desktop should now show Ted's tools in the MCP tools list

## Available Tools

The following Ted tools are exposed through MCP:

- **file_read**: Read file contents
- **file_write**: Write files
- **file_edit**: Edit files with find/replace
- **shell**: Execute shell commands
- **glob**: Find files by pattern
- **grep**: Search file contents
- **plan_update**: Update planning documents
- **database_init**: Initialize database schema
- **database_migrate**: Run database migrations
- **database_query**: Execute SQL queries
- **database_seed**: Seed database with test data

## Running with a Specific Project

To run the MCP server with a specific project directory:

```bash
ted mcp --project /path/to/your/project
```

When configured in Claude Desktop, update the args:

```json
{
  "mcpServers": {
    "ted": {
      "command": "ted",
      "args": ["mcp", "--project", "/path/to/your/project"],
      "env": {}
    }
  }
}
```

## Protocol Details

- **Protocol Version**: 2024-11-05
- **Transport**: stdio (JSON-RPC 2.0 over stdin/stdout)
- **Capabilities**: Tools
- **Server Name**: ted
- **Server Version**: (matches Ted version)

## Debugging

To see MCP server logs, check stderr output when Claude Desktop launches the server. You can also test the server manually:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | ted mcp
```

## Security Notes

- The MCP server runs with the same permissions as the user running Claude Desktop
- All file operations are constrained to the project directory (when specified)
- Shell commands execute with full user permissions - use caution
- Consider using Ted's trust mode settings to control tool permissions

## Troubleshooting

**Tools not showing up in Claude Desktop:**
1. Check that Ted is installed and in your PATH: `which ted`
2. Verify the configuration file path is correct
3. Restart Claude Desktop after configuration changes
4. Check Claude Desktop's logs for errors

**Permission errors:**
- Ensure Ted has execute permissions: `chmod +x $(which ted)`
- Verify the project directory (if specified) exists and is readable

**Connection errors:**
- Test the MCP server manually (see Debugging section)
- Check that Ted can run: `ted --version`
- Verify JSON syntax in the configuration file
