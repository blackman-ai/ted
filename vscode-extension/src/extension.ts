// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import * as vscode from 'vscode';
import { TedRunner } from './ted-runner';
import { ChatViewProvider } from './chat-view';

let tedRunner: TedRunner | null = null;
let chatViewProvider: ChatViewProvider | null = null;

export function activate(context: vscode.ExtensionContext) {
  console.log('[Ted] Extension activating...');

  // Initialize Ted runner
  tedRunner = new TedRunner(context);

  // Initialize chat view provider
  chatViewProvider = new ChatViewProvider(context, tedRunner);

  // Register chat view
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider('ted.chatView', chatViewProvider)
  );

  // Register commands
  context.subscriptions.push(
    vscode.commands.registerCommand('ted.openChat', () => {
      vscode.commands.executeCommand('ted.chatView.focus');
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ted.askQuestion', async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showWarningMessage('No active editor');
        return;
      }

      const selection = editor.selection;
      const selectedText = editor.document.getText(selection);

      if (!selectedText) {
        vscode.window.showWarningMessage('Please select some code first');
        return;
      }

      // Get the question from the user
      const question = await vscode.window.showInputBox({
        prompt: 'What would you like to know about this code?',
        placeHolder: 'e.g., What does this function do?',
      });

      if (!question) return;

      // Build the prompt with context
      const filePath = editor.document.uri.fsPath;
      const relativePath = vscode.workspace.asRelativePath(filePath);
      const lineStart = selection.start.line + 1;
      const lineEnd = selection.end.line + 1;

      const prompt = `Looking at this code from ${relativePath} (lines ${lineStart}-${lineEnd}):\n\n\`\`\`\n${selectedText}\n\`\`\`\n\n${question}`;

      // Focus the chat view and send the message
      await vscode.commands.executeCommand('ted.chatView.focus');
      chatViewProvider?.sendMessage(prompt);
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ted.editCode', async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showWarningMessage('No active editor');
        return;
      }

      const filePath = editor.document.uri.fsPath;
      const relativePath = vscode.workspace.asRelativePath(filePath);

      // Get the edit instruction from the user
      const instruction = await vscode.window.showInputBox({
        prompt: `What changes would you like Ted to make to ${relativePath}?`,
        placeHolder: 'e.g., Add error handling to this function',
      });

      if (!instruction) return;

      // Build the prompt
      const selection = editor.selection;
      let prompt: string;

      if (!selection.isEmpty) {
        const selectedText = editor.document.getText(selection);
        const lineStart = selection.start.line + 1;
        const lineEnd = selection.end.line + 1;
        prompt = `In ${relativePath} (lines ${lineStart}-${lineEnd}), please ${instruction}. Here's the current code:\n\n\`\`\`\n${selectedText}\n\`\`\``;
      } else {
        prompt = `In the file ${relativePath}, please ${instruction}`;
      }

      // Focus the chat view and send the message
      await vscode.commands.executeCommand('ted.chatView.focus');
      chatViewProvider?.sendMessage(prompt);
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ted.stopTask', () => {
      tedRunner?.stop();
      vscode.window.showInformationMessage('Ted: Task stopped');
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ted.clearHistory', () => {
      tedRunner?.clearHistory();
      chatViewProvider?.clearChat();
      vscode.window.showInformationMessage('Ted: Conversation history cleared');
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ted.setProvider', async () => {
      const providers = [
        { label: 'Anthropic Claude', value: 'anthropic' },
        { label: 'Ollama (Local)', value: 'ollama' },
        { label: 'OpenRouter', value: 'openrouter' },
      ];

      const selected = await vscode.window.showQuickPick(providers, {
        placeHolder: 'Select AI provider',
      });

      if (selected) {
        const config = vscode.workspace.getConfiguration('ted');
        await config.update('provider', selected.value, vscode.ConfigurationTarget.Global);
        vscode.window.showInformationMessage(`Ted: Provider set to ${selected.label}`);
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ted.attachSession', async () => {
      if (!tedRunner) return;

      try {
        const sessions = await tedRunner.listRecentSessions(20);
        if (sessions.length === 0) {
          vscode.window.showInformationMessage('Ted: No recent sessions found');
          return;
        }

        const selected = await vscode.window.showQuickPick(
          sessions.map((session) => ({
            label: `${session.id}  ${session.summary}`,
            description: `${session.date} • ${session.directory}`,
            session,
          })),
          { placeHolder: 'Select a Ted session to attach in VS Code' }
        );

        if (!selected) return;

        tedRunner.setResumeSession(selected.session.id);
        const metadata = await tedRunner.getSessionAttachMetadata(selected.session.id);
        const caps = Array.isArray(metadata?.capabilities?.caps)
          ? metadata.capabilities.caps.join(', ')
          : 'unknown';
        const model = metadata?.capabilities?.model || 'unknown';
        vscode.window.showInformationMessage(
          `Ted: Attached to session ${selected.session.id} (model: ${model}, caps: ${caps})`
        );
      } catch (error: any) {
        vscode.window.showErrorMessage(`Ted attach failed: ${error?.message || error}`);
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('ted.resumeInCli', async () => {
      if (!tedRunner) return;
      const currentSessionId = tedRunner.sessionId;
      if (!currentSessionId) {
        vscode.window.showWarningMessage(
          'No active Ted session in VS Code. Use "Ted: Attach Session" first.'
        );
        return;
      }

      const terminal = vscode.window.createTerminal('Ted CLI');
      terminal.show(true);
      terminal.sendText(`ted chat --resume ${currentSessionId}`);
    })
  );

  // Show status bar item
  const statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Right,
    100
  );
  statusBarItem.text = '$(hubot) Ted';
  statusBarItem.tooltip = 'Open Ted AI Assistant';
  statusBarItem.command = 'ted.openChat';
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  // Listen to Ted runner status
  tedRunner.onStatusChange((status) => {
    if (status === 'running') {
      statusBarItem.text = '$(sync~spin) Ted';
      statusBarItem.tooltip = 'Ted is thinking...';
    } else {
      statusBarItem.text = '$(hubot) Ted';
      statusBarItem.tooltip = 'Open Ted AI Assistant';
    }
  });

  console.log('[Ted] Extension activated');
}

export function deactivate() {
  console.log('[Ted] Extension deactivating...');
  tedRunner?.dispose();
  tedRunner = null;
  chatViewProvider = null;
}
