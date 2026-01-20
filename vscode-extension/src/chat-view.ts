// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import * as vscode from 'vscode';
import { TedRunner, TedEvent, TedStatus } from './ted-runner';

interface Message {
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
}

export class ChatViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = 'ted.chatView';

  private _view?: vscode.WebviewView;
  private context: vscode.ExtensionContext;
  private tedRunner: TedRunner;
  private messages: Message[] = [];
  private currentAssistantMessage: string = '';

  constructor(context: vscode.ExtensionContext, tedRunner: TedRunner) {
    this.context = context;
    this.tedRunner = tedRunner;

    // Listen to Ted events
    tedRunner.onEvent((event) => this.handleTedEvent(event));
    tedRunner.onOutput((text) => this.handleTedOutput(text));
    tedRunner.onStatusChange((status) => this.handleStatusChange(status));
  }

  public resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ) {
    this._view = webviewView;

    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [this.context.extensionUri],
    };

    webviewView.webview.html = this.getHtmlContent();

    // Handle messages from the webview
    webviewView.webview.onDidReceiveMessage((data) => {
      switch (data.type) {
        case 'sendMessage':
          this.handleUserMessage(data.message);
          break;
        case 'stopTask':
          this.tedRunner.stop();
          break;
        case 'clearHistory':
          this.clearChat();
          break;
      }
    });

    // Send initial state
    this.updateWebview();
  }

  /**
   * Send a message programmatically (from commands)
   */
  public sendMessage(message: string) {
    this.handleUserMessage(message);
  }

  /**
   * Clear the chat
   */
  public clearChat() {
    this.messages = [];
    this.currentAssistantMessage = '';
    this.tedRunner.clearHistory();
    this.updateWebview();
  }

  /**
   * Handle user message from webview
   */
  private handleUserMessage(message: string) {
    if (!message.trim()) return;

    // Add user message
    this.messages.push({
      role: 'user',
      content: message,
      timestamp: Date.now(),
    });

    // Reset assistant message buffer
    this.currentAssistantMessage = '';

    this.updateWebview();

    // Run Ted
    this.tedRunner.run(message);
  }

  /**
   * Handle Ted events
   */
  private handleTedEvent(event: TedEvent) {
    switch (event.type) {
      case 'text':
      case 'text_delta':
        // Append to current assistant message
        if (event.content) {
          this.currentAssistantMessage += event.content;
          this.updateWebview();
        }
        break;

      case 'message_start':
        // Start of new assistant message
        this.currentAssistantMessage = '';
        break;

      case 'message_stop':
      case 'message_end':
        // End of assistant message - save it
        if (this.currentAssistantMessage) {
          this.messages.push({
            role: 'assistant',
            content: this.currentAssistantMessage,
            timestamp: Date.now(),
          });
          this.currentAssistantMessage = '';
        }
        this.updateWebview();
        break;

      case 'tool_use':
        // Show tool usage in the chat
        const toolInfo = `Using tool: **${event.tool_name}**`;
        this.currentAssistantMessage += `\n\n${toolInfo}\n`;
        this.updateWebview();
        break;

      case 'tool_result':
        // Tool completed
        if (event.tool_result) {
          const resultPreview =
            typeof event.tool_result === 'string'
              ? event.tool_result.slice(0, 200)
              : JSON.stringify(event.tool_result).slice(0, 200);
          this.currentAssistantMessage += `\n\`\`\`\n${resultPreview}${resultPreview.length >= 200 ? '...' : ''}\n\`\`\`\n`;
          this.updateWebview();
        }
        break;

      case 'file_write':
      case 'file_edit':
        const filePath = event.path || event.file_path;
        this.messages.push({
          role: 'system',
          content: `File ${event.type === 'file_write' ? 'created' : 'edited'}: ${filePath}`,
          timestamp: Date.now(),
        });
        this.updateWebview();
        break;

      case 'error':
        this.messages.push({
          role: 'system',
          content: `Error: ${event.error || event.content}`,
          timestamp: Date.now(),
        });
        this.updateWebview();
        break;

      case 'exit':
        // Ensure any pending assistant message is saved
        if (this.currentAssistantMessage) {
          this.messages.push({
            role: 'assistant',
            content: this.currentAssistantMessage,
            timestamp: Date.now(),
          });
          this.currentAssistantMessage = '';
        }
        this.updateWebview();
        break;
    }
  }

  /**
   * Handle raw Ted output
   */
  private handleTedOutput(text: string) {
    // Could show in output channel or add to chat
    console.log('[Ted Output]', text);
  }

  /**
   * Handle status change
   */
  private handleStatusChange(status: TedStatus) {
    this._view?.webview.postMessage({
      type: 'statusChange',
      status,
    });
  }

  /**
   * Update the webview with current state
   */
  private updateWebview() {
    this._view?.webview.postMessage({
      type: 'updateMessages',
      messages: this.messages,
      currentMessage: this.currentAssistantMessage,
      status: this.tedRunner.status,
    });
  }

  /**
   * Get the HTML content for the webview
   */
  private getHtmlContent(): string {
    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Ted Chat</title>
  <style>
    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }

    body {
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
      background: var(--vscode-sideBar-background);
      height: 100vh;
      display: flex;
      flex-direction: column;
    }

    .header {
      padding: 12px 16px;
      border-bottom: 1px solid var(--vscode-sideBar-border);
      display: flex;
      justify-content: space-between;
      align-items: center;
    }

    .header-title {
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--vscode-charts-green);
    }

    .status-dot.running {
      background: var(--vscode-charts-yellow);
      animation: pulse 1s infinite;
    }

    @keyframes pulse {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.5; }
    }

    .header-actions {
      display: flex;
      gap: 8px;
    }

    .icon-button {
      background: none;
      border: none;
      color: var(--vscode-foreground);
      cursor: pointer;
      padding: 4px;
      border-radius: 4px;
      opacity: 0.7;
    }

    .icon-button:hover {
      opacity: 1;
      background: var(--vscode-toolbar-hoverBackground);
    }

    .messages {
      flex: 1;
      overflow-y: auto;
      padding: 16px;
    }

    .message {
      margin-bottom: 16px;
      padding: 12px;
      border-radius: 8px;
      max-width: 100%;
    }

    .message.user {
      background: var(--vscode-input-background);
      border: 1px solid var(--vscode-input-border);
    }

    .message.assistant {
      background: var(--vscode-editor-background);
      border: 1px solid var(--vscode-editorWidget-border);
    }

    .message.system {
      background: var(--vscode-editorInfo-background);
      border-left: 3px solid var(--vscode-editorInfo-foreground);
      font-size: 0.9em;
      opacity: 0.8;
    }

    .message-role {
      font-weight: 600;
      margin-bottom: 8px;
      font-size: 0.85em;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      opacity: 0.7;
    }

    .message-content {
      white-space: pre-wrap;
      word-wrap: break-word;
      line-height: 1.5;
    }

    .message-content code {
      background: var(--vscode-textCodeBlock-background);
      padding: 2px 6px;
      border-radius: 4px;
      font-family: var(--vscode-editor-font-family);
      font-size: 0.9em;
    }

    .message-content pre {
      background: var(--vscode-textCodeBlock-background);
      padding: 12px;
      border-radius: 6px;
      overflow-x: auto;
      margin: 8px 0;
    }

    .message-content pre code {
      padding: 0;
      background: none;
    }

    .input-area {
      padding: 16px;
      border-top: 1px solid var(--vscode-sideBar-border);
    }

    .input-container {
      display: flex;
      gap: 8px;
    }

    .input-field {
      flex: 1;
      padding: 10px 12px;
      border: 1px solid var(--vscode-input-border);
      background: var(--vscode-input-background);
      color: var(--vscode-input-foreground);
      border-radius: 6px;
      font-family: inherit;
      font-size: inherit;
      resize: none;
      min-height: 40px;
      max-height: 200px;
    }

    .input-field:focus {
      outline: none;
      border-color: var(--vscode-focusBorder);
    }

    .send-button {
      padding: 10px 16px;
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border: none;
      border-radius: 6px;
      cursor: pointer;
      font-weight: 500;
    }

    .send-button:hover {
      background: var(--vscode-button-hoverBackground);
    }

    .send-button:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }

    .empty-state {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      height: 100%;
      opacity: 0.6;
      text-align: center;
      padding: 20px;
    }

    .empty-state-icon {
      font-size: 48px;
      margin-bottom: 16px;
    }

    .typing-indicator {
      display: inline-flex;
      gap: 4px;
      padding: 8px 12px;
    }

    .typing-indicator span {
      width: 6px;
      height: 6px;
      background: var(--vscode-foreground);
      border-radius: 50%;
      animation: bounce 1.4s infinite ease-in-out;
    }

    .typing-indicator span:nth-child(1) { animation-delay: -0.32s; }
    .typing-indicator span:nth-child(2) { animation-delay: -0.16s; }

    @keyframes bounce {
      0%, 80%, 100% { transform: scale(0); }
      40% { transform: scale(1); }
    }
  </style>
</head>
<body>
  <div class="header">
    <div class="header-title">
      <span class="status-dot" id="statusDot"></span>
      <span>Ted</span>
    </div>
    <div class="header-actions">
      <button class="icon-button" onclick="stopTask()" title="Stop">⬛</button>
      <button class="icon-button" onclick="clearHistory()" title="Clear">🗑️</button>
    </div>
  </div>

  <div class="messages" id="messages">
    <div class="empty-state" id="emptyState">
      <div class="empty-state-icon">🤖</div>
      <p>Ask Ted to help with your code</p>
      <p style="font-size: 0.85em; margin-top: 8px;">
        Try: "Create a new React component" or "Fix the bug in this file"
      </p>
    </div>
  </div>

  <div class="input-area">
    <div class="input-container">
      <textarea
        class="input-field"
        id="inputField"
        placeholder="Ask Ted..."
        rows="1"
        onkeydown="handleKeyDown(event)"
        oninput="autoResize(this)"
      ></textarea>
      <button class="send-button" id="sendButton" onclick="sendMessage()">Send</button>
    </div>
  </div>

  <script>
    const vscode = acquireVsCodeApi();
    let currentStatus = 'idle';
    let messages = [];
    let currentMessage = '';

    function sendMessage() {
      const input = document.getElementById('inputField');
      const message = input.value.trim();
      if (!message) return;

      vscode.postMessage({ type: 'sendMessage', message });
      input.value = '';
      autoResize(input);
    }

    function stopTask() {
      vscode.postMessage({ type: 'stopTask' });
    }

    function clearHistory() {
      vscode.postMessage({ type: 'clearHistory' });
    }

    function handleKeyDown(event) {
      if (event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        sendMessage();
      }
    }

    function autoResize(textarea) {
      textarea.style.height = 'auto';
      textarea.style.height = Math.min(textarea.scrollHeight, 200) + 'px';
    }

    function escapeHtml(text) {
      const div = document.createElement('div');
      div.textContent = text;
      return div.innerHTML;
    }

    function formatContent(content) {
      // Simple markdown-like formatting
      let html = escapeHtml(content);

      // Code blocks
      html = html.replace(/\`\`\`([\\s\\S]*?)\`\`\`/g, '<pre><code>$1</code></pre>');

      // Inline code
      html = html.replace(/\`([^\`]+)\`/g, '<code>$1</code>');

      // Bold
      html = html.replace(/\\*\\*([^*]+)\\*\\*/g, '<strong>$1</strong>');

      return html;
    }

    function renderMessages() {
      const container = document.getElementById('messages');
      const emptyState = document.getElementById('emptyState');

      if (messages.length === 0 && !currentMessage) {
        emptyState.style.display = 'flex';
        container.innerHTML = '';
        container.appendChild(emptyState);
        return;
      }

      emptyState.style.display = 'none';

      let html = '';

      for (const msg of messages) {
        const roleLabel = msg.role === 'user' ? 'You' : msg.role === 'assistant' ? 'Ted' : 'System';
        html += \`
          <div class="message \${msg.role}">
            <div class="message-role">\${roleLabel}</div>
            <div class="message-content">\${formatContent(msg.content)}</div>
          </div>
        \`;
      }

      // Show current streaming message
      if (currentMessage) {
        html += \`
          <div class="message assistant">
            <div class="message-role">Ted</div>
            <div class="message-content">\${formatContent(currentMessage)}</div>
          </div>
        \`;
      } else if (currentStatus === 'running') {
        html += \`
          <div class="message assistant">
            <div class="message-role">Ted</div>
            <div class="typing-indicator">
              <span></span>
              <span></span>
              <span></span>
            </div>
          </div>
        \`;
      }

      container.innerHTML = html;
      container.scrollTop = container.scrollHeight;
    }

    function updateStatus(status) {
      currentStatus = status;
      const dot = document.getElementById('statusDot');
      const button = document.getElementById('sendButton');

      if (status === 'running') {
        dot.classList.add('running');
        button.disabled = true;
        button.textContent = '...';
      } else {
        dot.classList.remove('running');
        button.disabled = false;
        button.textContent = 'Send';
      }
    }

    // Handle messages from the extension
    window.addEventListener('message', event => {
      const data = event.data;

      switch (data.type) {
        case 'updateMessages':
          messages = data.messages || [];
          currentMessage = data.currentMessage || '';
          updateStatus(data.status);
          renderMessages();
          break;

        case 'statusChange':
          updateStatus(data.status);
          break;
      }
    });

    // Initial render
    renderMessages();
  </script>
</body>
</html>`;
  }
}
