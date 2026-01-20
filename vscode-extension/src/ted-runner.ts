// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';

export interface TedEvent {
  type: string;
  content?: string;
  tool_name?: string;
  tool_input?: any;
  tool_result?: any;
  error?: string;
  path?: string;
  file_path?: string;
}

export type TedStatus = 'idle' | 'running' | 'error';

export class TedRunner {
  private process: cp.ChildProcess | null = null;
  private context: vscode.ExtensionContext;
  private statusListeners: ((status: TedStatus) => void)[] = [];
  private eventListeners: ((event: TedEvent) => void)[] = [];
  private outputListeners: ((text: string) => void)[] = [];
  private _status: TedStatus = 'idle';

  constructor(context: vscode.ExtensionContext) {
    this.context = context;
  }

  get status(): TedStatus {
    return this._status;
  }

  private setStatus(status: TedStatus) {
    this._status = status;
    this.statusListeners.forEach((listener) => listener(status));
  }

  onStatusChange(listener: (status: TedStatus) => void): vscode.Disposable {
    this.statusListeners.push(listener);
    return new vscode.Disposable(() => {
      const index = this.statusListeners.indexOf(listener);
      if (index >= 0) this.statusListeners.splice(index, 1);
    });
  }

  onEvent(listener: (event: TedEvent) => void): vscode.Disposable {
    this.eventListeners.push(listener);
    return new vscode.Disposable(() => {
      const index = this.eventListeners.indexOf(listener);
      if (index >= 0) this.eventListeners.splice(index, 1);
    });
  }

  onOutput(listener: (text: string) => void): vscode.Disposable {
    this.outputListeners.push(listener);
    return new vscode.Disposable(() => {
      const index = this.outputListeners.indexOf(listener);
      if (index >= 0) this.outputListeners.splice(index, 1);
    });
  }

  private emitEvent(event: TedEvent) {
    this.eventListeners.forEach((listener) => listener(event));
  }

  private emitOutput(text: string) {
    this.outputListeners.forEach((listener) => listener(text));
  }

  /**
   * Find the ted binary path
   */
  private findTedBinary(): string | null {
    const config = vscode.workspace.getConfiguration('ted');
    const configuredPath = config.get<string>('tedBinaryPath');

    if (configuredPath && fs.existsSync(configuredPath)) {
      return configuredPath;
    }

    // Check common installation paths
    const homeDir = os.homedir();
    const possiblePaths = [
      // Installed via install script
      '/usr/local/bin/ted',
      path.join(homeDir, '.local/bin/ted'),
      // Cargo install
      path.join(homeDir, '.cargo/bin/ted'),
      // Development build
      path.join(this.context.extensionPath, '../target/release/ted'),
      path.join(this.context.extensionPath, '../target/debug/ted'),
    ];

    // On Windows, add .exe
    if (process.platform === 'win32') {
      possiblePaths.push(
        path.join(homeDir, '.cargo/bin/ted.exe'),
        path.join(process.env.PROGRAMFILES || '', 'Ted/ted.exe')
      );
    }

    for (const p of possiblePaths) {
      if (fs.existsSync(p)) {
        return p;
      }
    }

    // Try which/where
    try {
      const result = cp.execSync(
        process.platform === 'win32' ? 'where ted' : 'which ted',
        { encoding: 'utf-8' }
      );
      const tedPath = result.trim().split('\n')[0];
      if (tedPath && fs.existsSync(tedPath)) {
        return tedPath;
      }
    } catch {
      // Not found via PATH
    }

    return null;
  }

  /**
   * Get the project root (workspace folder)
   */
  private getProjectRoot(): string | null {
    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (workspaceFolders && workspaceFolders.length > 0) {
      return workspaceFolders[0].uri.fsPath;
    }
    return null;
  }

  /**
   * Build command arguments based on configuration
   */
  private buildArgs(prompt: string): string[] {
    const config = vscode.workspace.getConfiguration('ted');
    const args: string[] = [];

    // Output format
    args.push('--output-format', 'jsonl');

    // Provider and model
    const provider = config.get<string>('provider') || 'anthropic';
    args.push('--provider', provider);

    if (provider === 'anthropic') {
      const model = config.get<string>('anthropicModel');
      if (model) args.push('--model', model);
    } else if (provider === 'ollama') {
      const model = config.get<string>('ollamaModel');
      if (model) args.push('--model', model);
      const baseUrl = config.get<string>('ollamaBaseUrl');
      if (baseUrl) {
        args.push('--ollama-url', baseUrl);
      }
    } else if (provider === 'openrouter') {
      const model = config.get<string>('openrouterModel');
      if (model) args.push('--model', model);
    }

    // Trust mode
    const trustMode = config.get<boolean>('trustMode');
    if (trustMode) {
      args.push('--trust');
    }

    // The prompt
    args.push(prompt);

    return args;
  }

  /**
   * Build environment variables
   */
  private buildEnv(): NodeJS.ProcessEnv {
    const config = vscode.workspace.getConfiguration('ted');
    const env: NodeJS.ProcessEnv = { ...process.env };

    // API keys from configuration
    const anthropicKey = config.get<string>('anthropicApiKey');
    if (anthropicKey) {
      env.ANTHROPIC_API_KEY = anthropicKey;
    }

    const openrouterKey = config.get<string>('openrouterApiKey');
    if (openrouterKey) {
      env.OPENROUTER_API_KEY = openrouterKey;
    }

    return env;
  }

  /**
   * Run Ted with a prompt
   */
  async run(prompt: string): Promise<void> {
    if (this.process) {
      vscode.window.showWarningMessage('Ted is already running. Stop it first.');
      return;
    }

    const tedPath = this.findTedBinary();
    if (!tedPath) {
      vscode.window.showErrorMessage(
        'Ted binary not found. Please install Ted or configure the path in settings.',
        'Open Settings'
      ).then((selection) => {
        if (selection === 'Open Settings') {
          vscode.commands.executeCommand(
            'workbench.action.openSettings',
            'ted.tedBinaryPath'
          );
        }
      });
      return;
    }

    const projectRoot = this.getProjectRoot();
    if (!projectRoot) {
      vscode.window.showWarningMessage('Please open a folder first');
      return;
    }

    const args = this.buildArgs(prompt);
    const env = this.buildEnv();

    console.log(`[Ted] Running: ${tedPath} ${args.join(' ')}`);
    console.log(`[Ted] Working directory: ${projectRoot}`);

    this.setStatus('running');

    this.process = cp.spawn(tedPath, args, {
      cwd: projectRoot,
      env,
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    let buffer = '';

    this.process.stdout?.on('data', (data: Buffer) => {
      buffer += data.toString();

      // Process complete lines
      const lines = buffer.split('\n');
      buffer = lines.pop() || ''; // Keep incomplete line in buffer

      for (const line of lines) {
        if (!line.trim()) continue;

        try {
          const event: TedEvent = JSON.parse(line);
          this.handleEvent(event);
        } catch (err) {
          // Not JSON, treat as raw output
          this.emitOutput(line);
        }
      }
    });

    this.process.stderr?.on('data', (data: Buffer) => {
      const text = data.toString();
      console.error('[Ted stderr]', text);
      this.emitOutput(`[stderr] ${text}`);
    });

    this.process.on('error', (err) => {
      console.error('[Ted] Process error:', err);
      this.setStatus('error');
      this.emitEvent({ type: 'error', error: err.message });
      this.process = null;
    });

    this.process.on('exit', (code, signal) => {
      console.log(`[Ted] Process exited with code ${code}, signal ${signal}`);

      // Process any remaining buffer
      if (buffer.trim()) {
        try {
          const event: TedEvent = JSON.parse(buffer);
          this.handleEvent(event);
        } catch {
          this.emitOutput(buffer);
        }
      }

      this.setStatus('idle');
      this.emitEvent({ type: 'exit', content: `Exited with code ${code}` });
      this.process = null;
    });
  }

  /**
   * Handle a Ted event
   */
  private handleEvent(event: TedEvent) {
    console.log('[Ted] Event:', event.type);
    this.emitEvent(event);

    // Special handling for certain events
    switch (event.type) {
      case 'file_write':
      case 'file_edit':
        // Refresh the file in VS Code if it's open
        if (event.path || event.file_path) {
          const filePath = event.path || event.file_path;
          const uri = vscode.Uri.file(filePath!);
          vscode.workspace.openTextDocument(uri).then((doc) => {
            // Force VS Code to reload from disk
            vscode.commands.executeCommand('workbench.action.files.revert', uri);
          });
        }
        break;

      case 'error':
        vscode.window.showErrorMessage(`Ted error: ${event.error || event.content}`);
        break;
    }
  }

  /**
   * Stop the current Ted process
   */
  stop(): void {
    if (this.process) {
      this.process.kill('SIGTERM');
      setTimeout(() => {
        if (this.process && !this.process.killed) {
          this.process.kill('SIGKILL');
        }
      }, 5000);
    }
  }

  /**
   * Clear conversation history
   */
  clearHistory(): void {
    // History is managed per-process, so just ensure we're clean
    this.stop();
  }

  /**
   * Dispose of the runner
   */
  dispose(): void {
    this.stop();
    this.statusListeners = [];
    this.eventListeners = [];
    this.outputListeners = [];
  }
}
