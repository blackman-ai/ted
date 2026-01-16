// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { app, BrowserWindow, ipcMain, dialog } from 'electron';
import path from 'path';
import fs from 'fs/promises';
import { appendFileSync, writeFileSync } from 'fs';
import os from 'os';
import { TedRunner } from './ted/runner';
import { FileApplier } from './operations/file-applier';
import { AutoCommit } from './git/auto-commit';
import { TedEvent, ConversationHistoryEvent } from './types/protocol';

// Debug logging to file for Claude Code integration
const LOG_FILE = '/tmp/teddy-debug.log';
function log(...args: any[]) {
  const timestamp = new Date().toISOString();
  const message = `[${timestamp}] ${args.map(a => typeof a === 'object' ? JSON.stringify(a, null, 2) : a).join(' ')}\n`;
  appendFileSync(LOG_FILE, message);
  console.log(...args);
}

// Clear log file on startup
writeFileSync(LOG_FILE, `=== Teddy started at ${new Date().toISOString()} ===\n`);

let mainWindow: BrowserWindow | null = null;
let tedRunner: TedRunner | null = null;
let fileApplier: FileApplier | null = null;
let autoCommit: AutoCommit | null = null;
let currentProjectRoot: string | null = null;

// Conversation history for multi-turn chats
interface HistoryMessage {
  role: 'user' | 'assistant';
  content: string;
}
let conversationHistory: HistoryMessage[] = [];
let historyFilePath: string | null = null;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 1000,
    minHeight: 600,
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
    titleBarStyle: 'hiddenInset',
    backgroundColor: '#1e1e1e',
  });

  // Load the app
  if (process.env.NODE_ENV === 'development') {
    mainWindow.loadURL('http://localhost:5173');
    mainWindow.webContents.openDevTools();
  } else {
    mainWindow.loadFile(path.join(__dirname, '../dist/index.html'));
  }

  mainWindow.on('closed', () => {
    mainWindow = null;
    cleanupTed();
  });
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

app.on('activate', () => {
  if (mainWindow === null) {
    createWindow();
  }
});

// IPC Handlers

/**
 * Open folder picker dialog
 */
ipcMain.handle('dialog:openFolder', async () => {
  const result = await dialog.showOpenDialog({
    properties: ['openDirectory', 'createDirectory'],
    title: 'Open Project Folder',
  });

  if (!result.canceled && result.filePaths.length > 0) {
    return result.filePaths[0];
  }

  return null;
});

/**
 * Set the current project
 */
ipcMain.handle('project:set', async (_, projectPath: string) => {
  currentProjectRoot = projectPath;

  // Initialize helpers
  fileApplier = new FileApplier({ projectRoot: projectPath });
  autoCommit = new AutoCommit({ projectRoot: projectPath });

  // Initialize git repo if needed
  await autoCommit.init();

  return { success: true, path: projectPath };
});

/**
 * Get current project info
 */
ipcMain.handle('project:get', async () => {
  return {
    path: currentProjectRoot,
    hasProject: currentProjectRoot !== null,
  };
});

/**
 * Run Ted with a prompt
 */
ipcMain.handle('ted:run', async (_, prompt: string, options?: {
  trust?: boolean;
  provider?: string;
  model?: string;
  caps?: string[];
  newConversation?: boolean; // Set to true to start fresh conversation
}) => {
  log('[TED:RUN] Starting with prompt:', prompt.substring(0, 100));
  log('[TED:RUN] Options:', options);
  log('[TED:RUN] Current history length:', conversationHistory.length);

  if (!currentProjectRoot) {
    log('[TED:RUN] ERROR: No project selected');
    throw new Error('No project selected');
  }

  if (tedRunner?.isRunning()) {
    log('[TED:RUN] ERROR: Ted is already running');
    throw new Error('Ted is already running');
  }

  cleanupTed();

  // Clear history if starting a new conversation
  if (options?.newConversation) {
    log('[TED:RUN] Clearing history for new conversation');
    conversationHistory = [];
  }

  // Write history to temp file if we have previous turns
  if (conversationHistory.length > 0) {
    historyFilePath = path.join(os.tmpdir(), `ted-history-${Date.now()}.json`);
    await fs.writeFile(historyFilePath, JSON.stringify(conversationHistory), 'utf-8');
    log('[TED:RUN] Wrote history file:', historyFilePath, 'with', conversationHistory.length, 'messages');
    log('[TED:RUN] History content:', JSON.stringify(conversationHistory, null, 2));
  } else {
    historyFilePath = null;
    log('[TED:RUN] No history to write');
  }

  tedRunner = new TedRunner({
    workingDirectory: currentProjectRoot,
    trust: options?.trust ?? false,
    provider: options?.provider,
    model: options?.model,
    caps: options?.caps,
    historyFile: historyFilePath ?? undefined,
  });

  // Forward events to renderer
  tedRunner.on('event', (event: TedEvent) => {
    log('[TED:EVENT]', event.type, event.type === 'message' ? (event as any).data?.content?.substring(0, 50) : '');
    mainWindow?.webContents.send('ted:event', event);
  });

  tedRunner.on('stderr', (text: string) => {
    log('[TED:STDERR]', text);
    mainWindow?.webContents.send('ted:stderr', text);
  });

  tedRunner.on('error', (err: Error) => {
    log('[TED:ERROR]', err.message);
    mainWindow?.webContents.send('ted:error', err.message);
  });

  tedRunner.on('exit', (info) => {
    log('[TED:EXIT]', info);
    mainWindow?.webContents.send('ted:exit', info);
  });

  // Apply file operations automatically
  tedRunner.on('file_create', async (event) => {
    try {
      await fileApplier?.applyCreate(event);
      mainWindow?.webContents.send('file:changed', {
        type: 'create',
        path: event.data.path,
      });
    } catch (err) {
      console.error('Failed to apply file create:', err);
    }
  });

  tedRunner.on('file_edit', async (event) => {
    try {
      await fileApplier?.applyEdit(event);
      mainWindow?.webContents.send('file:changed', {
        type: 'edit',
        path: event.data.path,
      });
    } catch (err) {
      console.error('Failed to apply file edit:', err);
    }
  });

  tedRunner.on('file_delete', async (event) => {
    try {
      await fileApplier?.applyDelete(event);
      mainWindow?.webContents.send('file:changed', {
        type: 'delete',
        path: event.data.path,
      });
    } catch (err) {
      console.error('Failed to apply file delete:', err);
    }
  });

  // Update conversation history when Ted sends it
  tedRunner.on('conversation_history', (event: ConversationHistoryEvent) => {
    conversationHistory = event.data.messages.map(m => ({
      role: m.role,
      content: m.content,
    }));
    console.log('[HISTORY] Updated conversation history:', conversationHistory.length, 'messages');
  });

  // Auto-commit on completion
  tedRunner.on('completion', async (event) => {
    if (event.data.success && event.data.files_changed.length > 0) {
      try {
        await autoCommit?.commitChanges(
          event.data.files_changed,
          event.data.summary
        );
        mainWindow?.webContents.send('git:committed', {
          files: event.data.files_changed,
          summary: event.data.summary,
        });
      } catch (err) {
        console.error('Failed to auto-commit:', err);
      }
    }
  });

  // Start Ted
  await tedRunner.run(prompt);

  return { success: true };
});

/**
 * Stop Ted
 */
ipcMain.handle('ted:stop', async () => {
  if (tedRunner) {
    tedRunner.stop();
    return { success: true };
  }
  return { success: false };
});

/**
 * Read a file from the project
 */
ipcMain.handle('file:read', async (_, relativePath: string) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  const fs = require('fs/promises');
  const fullPath = path.join(currentProjectRoot, relativePath);
  const content = await fs.readFile(fullPath, 'utf-8');

  return { content };
});

/**
 * Write a file to the project
 */
ipcMain.handle('file:write', async (_, relativePath: string, content: string) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  const fs = require('fs/promises');
  const fullPath = path.join(currentProjectRoot, relativePath);

  // Ensure directory exists
  await fs.mkdir(path.dirname(fullPath), { recursive: true });

  // Write file
  await fs.writeFile(fullPath, content, 'utf-8');

  return { success: true };
});

/**
 * List files in project
 */
ipcMain.handle('file:list', async (_, dirPath: string = '.') => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  const fs = require('fs/promises');
  const fullPath = path.join(currentProjectRoot, dirPath);
  const entries = await fs.readdir(fullPath, { withFileTypes: true });

  return entries.map((entry: any) => ({
    name: entry.name,
    isDirectory: entry.isDirectory(),
    path: path.join(dirPath, entry.name),
  }));
});

/**
 * Cleanup Ted runner
 */
function cleanupTed() {
  if (tedRunner) {
    tedRunner.removeAllListeners();
    tedRunner.stop();
    tedRunner = null;
  }

  // Clean up temp history file
  if (historyFilePath) {
    fs.unlink(historyFilePath).catch(() => {});
    historyFilePath = null;
  }
}

/**
 * Clear conversation history (start fresh)
 */
ipcMain.handle('ted:clearHistory', async () => {
  conversationHistory = [];
  console.log('[HISTORY] Cleared conversation history');
  return { success: true };
});

/**
 * Get conversation history length (for debugging/UI)
 */
ipcMain.handle('ted:getHistoryLength', async () => {
  return { length: conversationHistory.length };
});
