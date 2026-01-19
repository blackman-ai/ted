// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';
import path from 'path';
import { TedParser } from './parser';
import { TedEvent } from '../types/protocol';

export interface TedRunnerOptions {
  tedBinaryPath?: string;  // Path to Ted binary (defaults to bundled)
  workingDirectory: string; // Project directory
  trust?: boolean;          // Auto-approve tool uses
  provider?: string;        // LLM provider (ollama, anthropic)
  model?: string;           // Model to use
  caps?: string[];          // Active caps
  historyFile?: string;     // Path to conversation history JSON file
  reviewMode?: boolean;     // Review mode - emit file events but don't execute modifications
  sessionId?: string;       // Resume a specific session by ID
  projectHasFiles?: boolean; // Whether the project has existing files (for enforcement)
}

/**
 * Ted subprocess runner
 *
 * Spawns Ted as a child process, manages its lifecycle,
 * and emits parsed JSONL events.
 */
export class TedRunner extends EventEmitter {
  private process: ChildProcess | null = null;
  private parser: TedParser;
  private options: TedRunnerOptions;

  constructor(options: TedRunnerOptions) {
    super();
    this.options = options;
    this.parser = new TedParser();

    // Forward parser events
    this.parser.on('event', (event) => this.emit('event', event));
    this.parser.on('error', (err) => this.emit('parse_error', err));

    // Forward specific event types
    this.parser.on('plan', (event) => this.emit('plan', event));
    this.parser.on('file_create', (event) => this.emit('file_create', event));
    this.parser.on('file_edit', (event) => this.emit('file_edit', event));
    this.parser.on('file_delete', (event) => this.emit('file_delete', event));
    this.parser.on('command', (event) => this.emit('command', event));
    this.parser.on('status', (event) => this.emit('status', event));
    this.parser.on('completion', (event) => this.emit('completion', event));
    this.parser.on('message', (event) => this.emit('message', event));
    this.parser.on('conversation_history', (event) => this.emit('conversation_history', event));
  }

  /**
   * Start Ted with a prompt
   */
  async run(prompt: string): Promise<void> {
    if (this.process) {
      throw new Error('Ted is already running');
    }

    const tedPath = this.getTedBinaryPath();
    const args = this.buildArgs(prompt);

    this.process = spawn(tedPath, args, {
      cwd: this.options.workingDirectory,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...process.env,
        // Ensure Ted uses embedded mode
        TED_EMBEDDED: '1',
      }
    });

    // Handle stdout (JSONL events)
    this.process.stdout?.on('data', (data: Buffer) => {
      const text = data.toString('utf-8');
      this.parser.feed(text);
    });

    // Handle stderr (logs, errors)
    this.process.stderr?.on('data', (data: Buffer) => {
      const text = data.toString('utf-8');
      this.emit('stderr', text);
    });

    // Handle process exit
    this.process.on('exit', (code, signal) => {
      this.emit('exit', { code, signal });
      this.cleanup();
    });

    // Handle process errors
    this.process.on('error', (err) => {
      this.emit('error', err);
      this.cleanup();
    });
  }

  /**
   * Stop the Ted process
   */
  stop(): void {
    if (this.process) {
      this.process.kill('SIGTERM');

      // Force kill after timeout
      setTimeout(() => {
        if (this.process && !this.process.killed) {
          this.process.kill('SIGKILL');
        }
      }, 5000);
    }
  }

  /**
   * Check if Ted is currently running
   */
  isRunning(): boolean {
    return this.process !== null && !this.process.killed;
  }

  /**
   * Get the path to the Ted binary
   */
  private getTedBinaryPath(): string {
    const fs = require('fs');

    console.log('[TED_RUNNER] getTedBinaryPath called');
    console.log('[TED_RUNNER] __dirname:', __dirname);
    console.log('[TED_RUNNER] NODE_ENV:', process.env.NODE_ENV);
    console.log('[TED_RUNNER] tedBinaryPath option:', this.options.tedBinaryPath);

    if (this.options.tedBinaryPath) {
      console.log('[TED_RUNNER] Using provided tedBinaryPath:', this.options.tedBinaryPath);
      return this.options.tedBinaryPath;
    }

    // In development: use cargo-built binary from parent ted repo
    // __dirname is out/main/ so we go up to teddy/, then up to ted/
    // Note: Check if we're in dev mode by looking for local cargo builds first
    // The old check for '.app' in resourcesPath was wrong because Electron's dev
    // resourcesPath also contains '.app' (the Electron framework bundle)
    const isDevelopment = process.env.NODE_ENV !== 'production';

    console.log('[TED_RUNNER] isDevelopment:', isDevelopment);

    if (isDevelopment) {
      // Try debug build first (faster compilation), fall back to release
      const debugBinary = path.join(__dirname, '../../../target/debug/ted');
      const releaseBinary = path.join(__dirname, '../../../target/release/ted');

      console.log('[TED_RUNNER] Checking debug binary:', debugBinary);
      console.log('[TED_RUNNER] Debug binary exists:', fs.existsSync(debugBinary));

      if (fs.existsSync(debugBinary)) {
        console.log('[TED_RUNNER] Using debug binary');
        return debugBinary;
      }

      console.log('[TED_RUNNER] Checking release binary:', releaseBinary);
      console.log('[TED_RUNNER] Release binary exists:', fs.existsSync(releaseBinary));

      if (fs.existsSync(releaseBinary)) {
        console.log('[TED_RUNNER] Using release binary');
        return releaseBinary;
      }

      // Neither exists, fall through to PATH
      console.log('[TED_RUNNER] No local binary found, falling back to PATH');
    }

    // In production: try bundled binary first, fall back to PATH
    const resourcePath = process.resourcesPath || path.join(__dirname, '../../');
    const binaryName = process.platform === 'win32' ? 'ted.exe' : 'ted';
    const bundledPath = path.join(resourcePath, 'bin', binaryName);

    console.log('[TED_RUNNER] resourcesPath:', process.resourcesPath);
    console.log('[TED_RUNNER] bundledPath:', bundledPath);
    console.log('[TED_RUNNER] bundledPath exists:', fs.existsSync(bundledPath));

    if (fs.existsSync(bundledPath)) {
      console.log('[TED_RUNNER] Using bundled binary');
      return bundledPath;
    }

    // Fall back to ted in PATH (for development/testing when binary not bundled)
    console.log('[TED_RUNNER] Using ted from PATH');
    return 'ted';
  }

  /**
   * Build command-line arguments for Ted
   */
  private buildArgs(prompt: string): string[] {
    const args: string[] = ['chat', '--embedded'];

    if (this.options.trust) {
      args.push('--trust');
    }

    if (this.options.provider) {
      args.push('--provider', this.options.provider);
    }

    if (this.options.model) {
      args.push('--model', this.options.model);
    }

    if (this.options.caps && this.options.caps.length > 0) {
      for (const cap of this.options.caps) {
        args.push('--cap', cap);
      }
    }

    // Add history file if provided
    if (this.options.historyFile) {
      args.push('--history', this.options.historyFile);
    }

    // Add review mode flag if enabled
    if (this.options.reviewMode) {
      args.push('--review-mode');
    }

    // Add session ID for resuming sessions
    if (this.options.sessionId) {
      args.push('--resume', this.options.sessionId);
    }

    // Tell Ted if the project has existing files (for enforcement logic)
    if (this.options.projectHasFiles) {
      args.push('--project-has-files');
    }

    // Add the prompt as the final argument
    args.push(prompt);

    return args;
  }

  /**
   * Cleanup resources
   */
  private cleanup(): void {
    this.process = null;
    this.parser.flush();
  }
}

/**
 * Typed event interface for TedRunner
 */
export interface TedRunnerEvents {
  on(event: 'event', listener: (event: TedEvent) => void): this;
  on(event: 'plan', listener: (event: Extract<TedEvent, { type: 'plan' }>) => void): this;
  on(event: 'file_create', listener: (event: Extract<TedEvent, { type: 'file_create' }>) => void): this;
  on(event: 'file_edit', listener: (event: Extract<TedEvent, { type: 'file_edit' }>) => void): this;
  on(event: 'file_delete', listener: (event: Extract<TedEvent, { type: 'file_delete' }>) => void): this;
  on(event: 'command', listener: (event: Extract<TedEvent, { type: 'command' }>) => void): this;
  on(event: 'status', listener: (event: Extract<TedEvent, { type: 'status' }>) => void): this;
  on(event: 'completion', listener: (event: Extract<TedEvent, { type: 'completion' }>) => void): this;
  on(event: 'message', listener: (event: Extract<TedEvent, { type: 'message' }>) => void): this;
  on(event: 'conversation_history', listener: (event: Extract<TedEvent, { type: 'conversation_history' }>) => void): this;
  on(event: 'stderr', listener: (text: string) => void): this;
  on(event: 'error', listener: (err: Error) => void): this;
  on(event: 'parse_error', listener: (err: Error) => void): this;
  on(event: 'exit', listener: (info: { code: number | null; signal: NodeJS.Signals | null }) => void): this;
}

export interface TedRunner extends TedRunnerEvents {}
