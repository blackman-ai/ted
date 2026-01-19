// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import chokidar, { FSWatcher } from 'chokidar';
import path from 'path';
import { EventEmitter } from 'events';

export interface FileChangeEvent {
  type: 'add' | 'change' | 'unlink' | 'addDir' | 'unlinkDir';
  path: string;
  relativePath: string;
}

export interface WatchOptions {
  projectPath: string;
  ignored?: string[];
  persistent?: boolean;
  ignoreInitial?: boolean;
}

/**
 * File watcher for detecting external changes to project files
 *
 * Watches a project directory for file changes and emits events.
 * Automatically ignores common build artifacts and dependencies.
 */
export class FileWatcher extends EventEmitter {
  private watcher: FSWatcher | null = null;
  private projectPath: string;
  private options: WatchOptions;

  // Default patterns to ignore
  private static readonly DEFAULT_IGNORED = [
    '**/node_modules/**',
    '**/.git/**',
    '**/target/**',          // Rust build output
    '**/dist/**',
    '**/build/**',
    '**/.next/**',
    '**/.cache/**',
    '**/.DS_Store',
    '**/Thumbs.db',
    '**/*.swp',
    '**/*.swo',
    '**/.teddy/**',          // Teddy storage
    '**/.ted/**',            // Ted storage
    '**/out/**',             // Electron build output
    '**/.vscode/**',
    '**/.idea/**',
    '**/__pycache__/**',
    '**/*.pyc',
    '**/venv/**',
    '**/.venv/**',
  ];

  constructor(options: WatchOptions) {
    super();
    this.projectPath = options.projectPath;
    this.options = {
      ...options,
      ignored: [...FileWatcher.DEFAULT_IGNORED, ...(options.ignored || [])],
      persistent: options.persistent ?? true,
      ignoreInitial: options.ignoreInitial ?? true,
    };
  }

  /**
   * Start watching the project directory
   */
  async start(): Promise<void> {
    if (this.watcher) {
      console.warn('[FileWatcher] Watcher already started');
      return;
    }

    console.log('[FileWatcher] Starting watcher for:', this.projectPath);
    console.log('[FileWatcher] Ignoring patterns:', this.options.ignored);

    this.watcher = chokidar.watch(this.projectPath, {
      ignored: this.options.ignored,
      persistent: this.options.persistent,
      ignoreInitial: this.options.ignoreInitial,
      // Performance optimization
      awaitWriteFinish: {
        stabilityThreshold: 500,  // Wait 500ms after write stops
        pollInterval: 100,         // Check every 100ms
      },
      // Don't follow symlinks to avoid issues
      followSymlinks: false,
      // Depth limit to avoid watching too deep
      depth: 10,
    });

    // Set up event handlers
    this.watcher
      .on('add', (filePath) => this.handleChange('add', filePath))
      .on('change', (filePath) => this.handleChange('change', filePath))
      .on('unlink', (filePath) => this.handleChange('unlink', filePath))
      .on('addDir', (dirPath) => this.handleChange('addDir', dirPath))
      .on('unlinkDir', (dirPath) => this.handleChange('unlinkDir', dirPath))
      .on('error', (error) => {
        console.error('[FileWatcher] Error:', error);
        this.emit('error', error);
      })
      .on('ready', () => {
        console.log('[FileWatcher] Initial scan complete, ready for changes');
        this.emit('ready');
      });

    // Wait for ready
    await new Promise<void>((resolve) => {
      if (!this.watcher) {
        resolve();
        return;
      }
      this.watcher.once('ready', () => resolve());
    });
  }

  /**
   * Stop watching
   */
  async stop(): Promise<void> {
    if (!this.watcher) {
      return;
    }

    console.log('[FileWatcher] Stopping watcher');
    await this.watcher.close();
    this.watcher = null;
    this.emit('stopped');
  }

  /**
   * Get watched paths
   */
  getWatched(): Record<string, string[]> {
    if (!this.watcher) {
      return {};
    }
    return this.watcher.getWatched();
  }

  /**
   * Check if currently watching
   */
  isWatching(): boolean {
    return this.watcher !== null;
  }

  /**
   * Handle file change event
   */
  private handleChange(type: FileChangeEvent['type'], filePath: string): void {
    const relativePath = path.relative(this.projectPath, filePath);

    const event: FileChangeEvent = {
      type,
      path: filePath,
      relativePath,
    };

    console.log('[FileWatcher] Change detected:', type, relativePath);
    this.emit('change', event);
    this.emit(type, event);
  }
}
