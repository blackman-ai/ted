// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { app, BrowserWindow, ipcMain, dialog } from 'electron';
import path from 'path';
import fs from 'fs/promises';
import { appendFileSync, writeFileSync, existsSync, readdirSync } from 'fs';
import os from 'os';
import crypto from 'crypto';
import { TedRunner } from './ted/runner';
import { FileApplier } from './operations/file-applier';
import { AutoCommit } from './git/auto-commit';
import { TedEvent, ConversationHistoryEvent } from './types/protocol';
import { storage, ProjectContext } from './storage/config';
import { loadTedSettings, saveTedSettings, detectHardware } from './settings/ted-settings';
import { deployProject, verifyToken, getDeploymentStatus } from './deploy/vercel';
import { deployNetlifyProject, verifyNetlifyToken, getNetlifyDeploymentStatus } from './deploy/netlify';

// Lazy-loaded module references to avoid electron app access at module load time
let cloudfareTunnel: typeof import('./deploy/cloudflare-tunnel') | null = null;
async function getCloudflareTunnel() {
  if (!cloudfareTunnel) {
    cloudfareTunnel = await import('./deploy/cloudflare-tunnel');
  }
  return cloudfareTunnel;
}

let shareModule: typeof import('./share') | null = null;
async function getShareModule() {
  if (!shareModule) {
    shareModule = await import('./share');
  }
  return shareModule;
}

let fileWatcherModule: typeof import('./file-watcher') | null = null;
async function getFileWatcherModule() {
  if (!fileWatcherModule) {
    fileWatcherModule = await import('./file-watcher');
  }
  return fileWatcherModule;
}

// Type imports for file watcher
type FileWatcherType = import('./file-watcher').FileWatcher;
type FileChangeEventType = import('./file-watcher').FileChangeEvent;

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
let fileWatcher: FileWatcherType | null = null;
let currentProjectRoot: string | null = null;

// Review mode - when enabled, file operations are sent to renderer for review instead of auto-applying
let reviewModeEnabled: boolean = true; // Default to review mode ON

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

app.whenReady().then(async () => {
  // Initialize storage before creating window
  await storage.init();
  log('[STORAGE] Initialized ~/.teddy storage');
  createWindow();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// Cleanup shares and tunnels when app is quitting
app.on('before-quit', async () => {
  log('[APP] Cleaning up before quit...');
  try {
    const share = await getShareModule();
    await share.stopAllShares();
    log('[APP] All shares stopped');
  } catch (err) {
    log('[APP] Error stopping shares:', err);
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

  // Stop previous file watcher if exists
  if (fileWatcher) {
    await fileWatcher.stop();
    fileWatcher = null;
  }

  // Start file watcher for this project
  try {
    const fwModule = await getFileWatcherModule();
    fileWatcher = new fwModule.FileWatcher({ projectPath });

    // Forward file change events to renderer
    fileWatcher.on('change', (event: FileChangeEventType) => {
      log('[FILE_WATCHER] File changed:', event.type, event.relativePath);
      mainWindow?.webContents.send('file:externalChange', event);
    });

    fileWatcher.on('error', (error) => {
      log('[FILE_WATCHER] Error:', error);
    });

    // Start watching
    await fileWatcher.start();
    log('[FILE_WATCHER] Started watching:', projectPath);
  } catch (err) {
    log('[FILE_WATCHER] Failed to start watcher:', err);
  }

  // Add to recent projects
  await storage.addRecentProject(projectPath);
  log('[PROJECT] Set project and added to recent:', projectPath);

  // Load saved conversation history for this project
  const savedHistory = await storage.loadProjectHistory(projectPath);
  if (savedHistory && savedHistory.length > 0) {
    conversationHistory = savedHistory;
    log('[PROJECT] Loaded saved history:', conversationHistory.length, 'messages');
  } else {
    conversationHistory = [];
  }

  // Scan and cache project context
  const context = await scanProjectContext(projectPath);
  await storage.saveProjectContext(projectPath, context);
  log('[PROJECT] Scanned and cached project context');

  return { success: true, path: projectPath, historyLength: conversationHistory.length };
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
 * Get recent projects list
 */
ipcMain.handle('project:getRecent', async () => {
  const recentProjects = await storage.getRecentProjects();
  // Filter out projects that no longer exist
  const validProjects = recentProjects.filter(p => existsSync(p.path));
  return validProjects;
});

/**
 * Get last opened project (for auto-load)
 */
ipcMain.handle('project:getLast', async () => {
  const lastPath = await storage.getLastProject();
  if (lastPath) {
    return { path: lastPath, name: path.basename(lastPath) };
  }
  return null;
});

/**
 * Clear last opened project (when user explicitly wants to change projects)
 */
ipcMain.handle('project:clearLast', async () => {
  await storage.clearLastProject();
  return { success: true };
});

/**
 * Remove a project from recent list
 */
ipcMain.handle('project:removeRecent', async (_, projectPath: string) => {
  await storage.removeRecentProject(projectPath);
  return { success: true };
});

/**
 * Get project context (file tree, readme, etc.)
 */
ipcMain.handle('project:getContext', async () => {
  if (!currentProjectRoot) {
    return null;
  }

  // Try to load cached context first
  let context = await storage.loadProjectContext(currentProjectRoot);

  // If no cache or stale (older than 5 minutes), rescan
  if (!context || Date.now() - context.lastScanned > 5 * 60 * 1000) {
    context = await scanProjectContext(currentProjectRoot);
    await storage.saveProjectContext(currentProjectRoot, context);
  }

  return context;
});

/**
 * Session management
 */

// Store session ID for current project
let currentSessionId: string | null = null;

/**
 * Get sessions for current project
 */
ipcMain.handle('session:list', async () => {
  if (!currentProjectRoot) {
    return [];
  }

  const sessions = await storage.getProjectSessions(currentProjectRoot);
  return sessions;
});

/**
 * Create a new session
 */
ipcMain.handle('session:create', async () => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  // Generate new session ID
  currentSessionId = crypto.randomUUID();

  // Clear conversation history for new session
  conversationHistory = [];

  // Store session info
  await storage.createSession(currentSessionId, currentProjectRoot);

  log('[SESSION] Created new session:', currentSessionId);
  return {
    id: currentSessionId,
    projectPath: currentProjectRoot,
  };
});

/**
 * Switch to an existing session
 */
ipcMain.handle('session:switch', async (_, sessionId: string) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  currentSessionId = sessionId;

  // Load conversation history for this session
  const history = await storage.loadSessionHistory(sessionId);
  conversationHistory = history || [];

  log('[SESSION] Switched to session:', sessionId, 'history length:', conversationHistory.length);
  return {
    id: currentSessionId,
    historyLength: conversationHistory.length,
  };
});

/**
 * Get current session
 */
ipcMain.handle('session:getCurrent', async () => {
  if (!currentSessionId) {
    return null;
  }

  const sessionInfo = await storage.getSession(currentSessionId);
  return sessionInfo;
});

/**
 * Delete a session
 */
ipcMain.handle('session:delete', async (_, sessionId: string) => {
  await storage.deleteSession(sessionId);

  // If we deleted the current session, clear it
  if (currentSessionId === sessionId) {
    currentSessionId = null;
    conversationHistory = [];
  }

  log('[SESSION] Deleted session:', sessionId);
  return { success: true };
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

  // If this is a new conversation (no history), inject project context into the prompt
  let finalPrompt = prompt;
  let projectHasFiles = false;
  const context = await storage.loadProjectContext(currentProjectRoot);
  if (context && context.fileTree.length > 0) {
    projectHasFiles = true;
    if (conversationHistory.length === 0) {
      const contextPrefix = buildContextPrefix(context);
      finalPrompt = contextPrefix + prompt;
      log('[TED:RUN] Injected project context into prompt');
    }
  }
  log('[TED:RUN] Project has files:', projectHasFiles);

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
    reviewMode: reviewModeEnabled,  // Pass review mode to Ted - it will emit events but skip file execution
    sessionId: currentSessionId ?? undefined,  // Pass session ID to resume
    projectHasFiles,  // Tell Ted if project has files (for enforcement logic)
  });

  // Forward events to renderer
  tedRunner.on('event', (event: TedEvent) => {
    if (event.type === 'file_edit' || event.type === 'file_create' || event.type === 'file_delete') {
      log('[TED:EVENT]', event.type, 'data:', JSON.stringify(event.data));
    } else {
      log('[TED:EVENT]', event.type, event.type === 'message' ? (event as any).data?.content?.substring(0, 50) : '');
    }
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

  // Apply file operations - either auto-apply or send to renderer for review
  tedRunner.on('file_create', async (event) => {
    if (reviewModeEnabled) {
      // In review mode, just forward the event - don't apply
      log('[FILE_CREATE] Review mode - forwarding to renderer:', event.data.path);
      // The event is already forwarded via 'event' listener above
    } else {
      // Auto-apply when review mode is off
      try {
        await fileApplier?.applyCreate(event);
        mainWindow?.webContents.send('file:changed', {
          type: 'create',
          path: event.data.path,
        });
      } catch (err) {
        console.error('Failed to apply file create:', err);
      }
    }
  });

  tedRunner.on('file_edit', async (event) => {
    if (reviewModeEnabled) {
      // In review mode, just forward the event - don't apply
      log('[FILE_EDIT] Review mode - forwarding to renderer:', event.data.path);
    } else {
      try {
        await fileApplier?.applyEdit(event);
        mainWindow?.webContents.send('file:changed', {
          type: 'edit',
          path: event.data.path,
        });
      } catch (err) {
        console.error('Failed to apply file edit:', err);
      }
    }
  });

  tedRunner.on('file_delete', async (event) => {
    if (reviewModeEnabled) {
      // In review mode, just forward the event - don't apply
      log('[FILE_DELETE] Review mode - forwarding to renderer:', event.data.path);
    } else {
      try {
        await fileApplier?.applyDelete(event);
        mainWindow?.webContents.send('file:changed', {
          type: 'delete',
          path: event.data.path,
        });
      } catch (err) {
        console.error('Failed to apply file delete:', err);
      }
    }
  });

  // Update conversation history when Ted sends it and persist to disk
  tedRunner.on('conversation_history', async (event: ConversationHistoryEvent) => {
    conversationHistory = event.data.messages.map(m => ({
      role: m.role,
      content: m.content,
    }));
    log('[HISTORY] Updated conversation history:', conversationHistory.length, 'messages');

    // Persist to disk for this project
    if (currentProjectRoot) {
      await storage.saveProjectHistory(currentProjectRoot, conversationHistory);
      log('[HISTORY] Saved to disk');
    }
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

  // Start Ted with the final prompt (which may include context)
  await tedRunner.run(finalPrompt);

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
ipcMain.handle('file:read', async (_, filePath: string) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  const fs = require('fs/promises');
  // Handle both absolute and relative paths
  const fullPath = path.isAbsolute(filePath) ? filePath : path.join(currentProjectRoot, filePath);
  const content = await fs.readFile(fullPath, 'utf-8');

  return { content };
});

/**
 * Write a file to the project
 */
ipcMain.handle('file:write', async (_, filePath: string, content: string) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  const fs = require('fs/promises');
  // Handle both absolute and relative paths
  const fullPath = path.isAbsolute(filePath) ? filePath : path.join(currentProjectRoot, filePath);

  // Ensure directory exists
  await fs.mkdir(path.dirname(fullPath), { recursive: true });

  // Write file
  await fs.writeFile(fullPath, content, 'utf-8');

  return { success: true };
});

/**
 * Delete a file from the project
 */
ipcMain.handle('file:delete', async (_, filePath: string) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  // Handle both absolute and relative paths
  const fullPath = path.isAbsolute(filePath) ? filePath : path.join(currentProjectRoot, filePath);

  // Security check: ensure path is within project root
  if (!fullPath.startsWith(currentProjectRoot)) {
    throw new Error('Path escapes project root');
  }

  await fs.unlink(fullPath);

  return { success: true };
});

/**
 * List files in project
 */
ipcMain.handle('file:list', async (_, dirPath: string = '.') => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

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
 * Set review mode (enable/disable file operation review)
 */
ipcMain.handle('review:set', async (_, enabled: boolean) => {
  reviewModeEnabled = enabled;
  log('[REVIEW_MODE] Set to:', enabled);
  return { success: true };
});

/**
 * Get current review mode status
 */
ipcMain.handle('review:get', async () => {
  return { enabled: reviewModeEnabled };
});

/**
 * Clear conversation history (start fresh)
 */
ipcMain.handle('ted:clearHistory', async () => {
  conversationHistory = [];
  log('[HISTORY] Cleared conversation history');

  // Also clear from disk
  if (currentProjectRoot) {
    await storage.clearProjectHistory(currentProjectRoot);
    log('[HISTORY] Cleared from disk');
  }

  return { success: true };
});

/**
 * Get conversation history length (for debugging/UI)
 */
ipcMain.handle('ted:getHistoryLength', async () => {
  return { length: conversationHistory.length };
});

// Track running shell processes
const runningShells: Map<number, import('child_process').ChildProcess> = new Map();

/**
 * Run a shell command directly (without Ted)
 */
ipcMain.handle('shell:run', async (_, command: string) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  const { spawn } = await import('child_process');

  log('[SHELL:RUN]', command);

  const child = spawn('sh', ['-c', command], {
    cwd: currentProjectRoot,
    detached: true,
    stdio: 'ignore',
  });

  child.unref();

  if (child.pid) {
    runningShells.set(child.pid, child);
    log('[SHELL:STARTED] PID:', child.pid);
  }

  return { success: true, pid: child.pid };
});

/**
 * Kill a running shell process
 */
ipcMain.handle('shell:kill', async (_, pid: number) => {
  log('[SHELL:KILL] PID:', pid);

  try {
    process.kill(pid, 'SIGTERM');
    runningShells.delete(pid);
    return { success: true };
  } catch (err) {
    log('[SHELL:KILL] Failed:', err);
    return { success: false };
  }
});

/**
 * Get Ted settings
 */
ipcMain.handle('settings:get', async () => {
  log('[SETTINGS:GET]');
  try {
    const settings = await loadTedSettings();
    return settings;
  } catch (err) {
    log('[SETTINGS:GET] Failed:', err);
    throw err;
  }
});

/**
 * Save Ted settings
 */
ipcMain.handle('settings:save', async (_, settings) => {
  log('[SETTINGS:SAVE]', settings);
  try {
    await saveTedSettings(settings);
    return { success: true };
  } catch (err) {
    log('[SETTINGS:SAVE] Failed:', err);
    throw err;
  }
});

/**
 * Detect hardware profile
 */
ipcMain.handle('settings:detectHardware', async () => {
  log('[SETTINGS:DETECT_HARDWARE]');
  try {
    const hardware = await detectHardware();
    return hardware;
  } catch (err) {
    log('[SETTINGS:DETECT_HARDWARE] Failed:', err);
    throw err;
  }
});

/**
 * Clear HTTP cache for preview iframe
 */
ipcMain.handle('cache:clear', async () => {
  log('[CACHE:CLEAR]');
  try {
    if (mainWindow) {
      await mainWindow.webContents.session.clearCache();
      log('[CACHE:CLEAR] Success');
      return { success: true };
    }
    return { success: false, error: 'No main window' };
  } catch (err) {
    log('[CACHE:CLEAR] Failed:', err);
    return { success: false, error: String(err) };
  }
});

/**
 * Deploy project to Vercel
 */
ipcMain.handle('deploy:vercel', async (_, options) => {
  log('[DEPLOY:VERCEL]', { projectPath: options.projectPath });
  try {
    // Send progress update to renderer
    if (mainWindow) {
      mainWindow.webContents.send('deploy:progress', { status: 'starting', message: 'Preparing deployment...' });
    }

    const result = await deployProject(options);

    if (result.success && mainWindow) {
      mainWindow.webContents.send('deploy:success', { url: result.url, deploymentId: result.deploymentId });
    } else if (!result.success && mainWindow) {
      mainWindow.webContents.send('deploy:error', { error: result.error });
    }

    return result;
  } catch (err) {
    log('[DEPLOY:VERCEL] Failed:', err);
    if (mainWindow) {
      mainWindow.webContents.send('deploy:error', { error: String(err) });
    }
    throw err;
  }
});

/**
 * Verify Vercel token
 */
ipcMain.handle('deploy:verifyVercelToken', async (_, token) => {
  log('[DEPLOY:VERIFY_TOKEN]');
  try {
    const result = await verifyToken(token);
    return result;
  } catch (err) {
    log('[DEPLOY:VERIFY_TOKEN] Failed:', err);
    throw err;
  }
});

/**
 * Get Vercel deployment status
 */
ipcMain.handle('deploy:getVercelStatus', async (_, deploymentId, token) => {
  log('[DEPLOY:GET_STATUS]', deploymentId);
  try {
    const status = await getDeploymentStatus(deploymentId, token);
    return status;
  } catch (err) {
    log('[DEPLOY:GET_STATUS] Failed:', err);
    throw err;
  }
});

/**
 * Deploy project to Netlify
 */
ipcMain.handle('deploy:netlify', async (_, options) => {
  log('[DEPLOY:NETLIFY]', { projectPath: options.projectPath });
  try {
    // Send progress update to renderer
    if (mainWindow) {
      mainWindow.webContents.send('deploy:progress', { status: 'starting', message: 'Preparing Netlify deployment...' });
    }

    const result = await deployNetlifyProject(options);

    if (result.success && mainWindow) {
      mainWindow.webContents.send('deploy:success', { url: result.url, deployId: result.deployId, siteId: result.siteId });
    } else if (!result.success && mainWindow) {
      mainWindow.webContents.send('deploy:error', { error: result.error });
    }

    return result;
  } catch (err) {
    log('[DEPLOY:NETLIFY] Failed:', err);
    if (mainWindow) {
      mainWindow.webContents.send('deploy:error', { error: String(err) });
    }
    throw err;
  }
});

/**
 * Verify Netlify token
 */
ipcMain.handle('deploy:verifyNetlifyToken', async (_, token) => {
  log('[DEPLOY:VERIFY_NETLIFY_TOKEN]');
  try {
    const result = await verifyNetlifyToken(token);
    return result;
  } catch (err) {
    log('[DEPLOY:VERIFY_NETLIFY_TOKEN] Failed:', err);
    throw err;
  }
});

/**
 * Get Netlify deployment status
 */
ipcMain.handle('deploy:getNetlifyStatus', async (_, deployId, token) => {
  log('[DEPLOY:GET_NETLIFY_STATUS]', deployId);
  try {
    const status = await getNetlifyDeploymentStatus(deployId, token);
    return status;
  } catch (err) {
    log('[DEPLOY:GET_NETLIFY_STATUS] Failed:', err);
    throw err;
  }
});

/**
 * Check if cloudflared is installed
 */
ipcMain.handle('tunnel:isInstalled', async () => {
  log('[TUNNEL:IS_INSTALLED]');
  try {
    const tunnel = await getCloudflareTunnel();
    const installed = tunnel.isCloudflaredInstalled();
    return { installed };
  } catch (err) {
    log('[TUNNEL:IS_INSTALLED] Failed:', err);
    throw err;
  }
});

/**
 * Get cloudflared installation instructions
 */
ipcMain.handle('tunnel:getInstallInstructions', async () => {
  log('[TUNNEL:GET_INSTALL_INSTRUCTIONS]');
  try {
    const tunnel = await getCloudflareTunnel();
    const instructions = tunnel.getInstallInstructions();
    return { instructions };
  } catch (err) {
    log('[TUNNEL:GET_INSTALL_INSTRUCTIONS] Failed:', err);
    throw err;
  }
});

/**
 * Auto-install cloudflared
 */
ipcMain.handle('tunnel:autoInstall', async () => {
  log('[TUNNEL:AUTO_INSTALL]');
  try {
    const tunnel = await getCloudflareTunnel();
    const result = await tunnel.autoInstallCloudflared();
    return result;
  } catch (err) {
    log('[TUNNEL:AUTO_INSTALL] Failed:', err);
    throw err;
  }
});

/**
 * Start Cloudflare Tunnel
 */
ipcMain.handle('tunnel:start', async (_, options) => {
  log('[TUNNEL:START]', { port: options.port });
  try {
    const tunnel = await getCloudflareTunnel();
    const result = await tunnel.startTunnel(options);

    if (result.success && mainWindow) {
      mainWindow.webContents.send('tunnel:started', { port: options.port, url: result.url });
    }

    return result;
  } catch (err) {
    log('[TUNNEL:START] Failed:', err);
    throw err;
  }
});

/**
 * Stop Cloudflare Tunnel
 */
ipcMain.handle('tunnel:stop', async (_, port) => {
  log('[TUNNEL:STOP]', port);
  try {
    const tunnel = await getCloudflareTunnel();
    const result = await tunnel.stopTunnel(port);

    if (result.success && mainWindow) {
      mainWindow.webContents.send('tunnel:stopped', { port });
    }

    return result;
  } catch (err) {
    log('[TUNNEL:STOP] Failed:', err);
    throw err;
  }
});

/**
 * Get tunnel URL for a port
 */
ipcMain.handle('tunnel:getUrl', async (_, port) => {
  log('[TUNNEL:GET_URL]', port);
  try {
    const tunnel = await getCloudflareTunnel();
    const url = tunnel.getTunnelUrl(port);
    return { url };
  } catch (err) {
    log('[TUNNEL:GET_URL] Failed:', err);
    throw err;
  }
});

/**
 * teddy.rocks Share Service
 */

/**
 * Start sharing a port via teddy.rocks
 */
ipcMain.handle('share:start', async (_, options: { port: number; projectName?: string; customSlug?: string }) => {
  log('[SHARE:START]', options);
  try {
    const share = await getShareModule();
    const result = await share.startShare(options);

    if (result.success && mainWindow) {
      mainWindow.webContents.send('share:started', {
        port: options.port,
        slug: result.slug,
        previewUrl: result.previewUrl,
      });
    }

    return result;
  } catch (err) {
    log('[SHARE:START] Failed:', err);
    throw err;
  }
});

/**
 * Stop sharing a port
 */
ipcMain.handle('share:stop', async (_, port: number) => {
  log('[SHARE:STOP]', port);
  try {
    const share = await getShareModule();
    const success = await share.stopShare(port);

    if (success && mainWindow) {
      mainWindow.webContents.send('share:stopped', { port });
    }

    return { success };
  } catch (err) {
    log('[SHARE:STOP] Failed:', err);
    throw err;
  }
});

/**
 * Get active share for a port
 */
ipcMain.handle('share:get', async (_, port: number) => {
  log('[SHARE:GET]', port);
  const share = await getShareModule();
  return share.getActiveShare(port);
});

/**
 * Get all active shares
 */
ipcMain.handle('share:getAll', async () => {
  log('[SHARE:GET_ALL]');
  const share = await getShareModule();
  return share.getAllActiveShares();
});

/**
 * Generate a slug for preview
 */
ipcMain.handle('share:generateSlug', async (_, projectName?: string) => {
  const share = await getShareModule();
  return share.generateSlug(projectName);
});

/**
 * Check if a slug is available
 */
ipcMain.handle('share:checkSlug', async (_, slug: string) => {
  const share = await getShareModule();
  return await share.checkSlugAvailability(slug);
});

/**
 * Scan project to build context (file tree, readme, package.json)
 */
async function scanProjectContext(projectRoot: string): Promise<ProjectContext> {
  const context: ProjectContext = {
    fileTree: [],
    readme: null,
    packageJson: null,
    lastScanned: Date.now(),
  };

  // Directories to skip
  const skipDirs = new Set([
    'node_modules', '.git', 'dist', 'build', 'target', '.next',
    '__pycache__', '.venv', 'venv', '.idea', '.vscode', 'coverage',
  ]);

  // File extensions to include
  const includeExts = new Set([
    '.js', '.jsx', '.ts', '.tsx', '.py', '.rs', '.go', '.java',
    '.html', '.css', '.scss', '.json', '.yaml', '.yml', '.toml',
    '.md', '.txt', '.sql', '.sh', '.prisma',
  ]);

  // Recursively scan directory (max depth 4)
  function scanDir(dir: string, relativePath: string, depth: number): void {
    if (depth > 4) return;

    try {
      const entries = readdirSync(dir, { withFileTypes: true });

      for (const entry of entries) {
        const entryPath = path.join(relativePath, entry.name);

        if (entry.isDirectory()) {
          if (!skipDirs.has(entry.name) && !entry.name.startsWith('.')) {
            context.fileTree.push(entryPath + '/');
            scanDir(path.join(dir, entry.name), entryPath, depth + 1);
          }
        } else {
          const ext = path.extname(entry.name).toLowerCase();
          if (includeExts.has(ext) || entry.name === 'Makefile' || entry.name === 'Cargo.toml') {
            context.fileTree.push(entryPath);
          }
        }
      }
    } catch (err) {
      // Skip directories we can't read
    }
  }

  scanDir(projectRoot, '', 0);

  // Read README if exists
  const readmeNames = ['README.md', 'readme.md', 'README.txt', 'readme.txt', 'README'];
  for (const name of readmeNames) {
    const readmePath = path.join(projectRoot, name);
    if (existsSync(readmePath)) {
      try {
        const content = require('fs').readFileSync(readmePath, 'utf-8');
        // Limit to first 2000 chars
        context.readme = content.length > 2000 ? content.slice(0, 2000) + '...' : content;
        break;
      } catch (err) {
        // Skip
      }
    }
  }

  // Read package.json if exists
  const packagePath = path.join(projectRoot, 'package.json');
  if (existsSync(packagePath)) {
    try {
      const content = require('fs').readFileSync(packagePath, 'utf-8');
      context.packageJson = JSON.parse(content);
    } catch (err) {
      // Skip
    }
  }

  log('[CONTEXT] Scanned project:', context.fileTree.length, 'files');
  return context;
}

/**
 * Build a context prefix to inject into the first prompt of a conversation.
 * This gives Ted awareness of the project structure without explicit exploration.
 */
function buildContextPrefix(context: ProjectContext): string {
  const parts: string[] = [];

  parts.push('[PROJECT CONTEXT]');
  parts.push('This project has the following files:');
  parts.push('');

  // Show file tree (limit to first 50 files to avoid overwhelming)
  const filesToShow = context.fileTree.slice(0, 50);
  for (const file of filesToShow) {
    parts.push(`  ${file}`);
  }
  if (context.fileTree.length > 50) {
    parts.push(`  ... and ${context.fileTree.length - 50} more files`);
  }
  parts.push('');

  // Include README summary if available
  if (context.readme) {
    parts.push('README:');
    parts.push(context.readme);
    parts.push('');
  }

  // Include package.json info if available
  if (context.packageJson) {
    const pkg = context.packageJson;
    parts.push('package.json info:');
    if (pkg.name) parts.push(`  Name: ${pkg.name}`);
    if (pkg.description) parts.push(`  Description: ${pkg.description}`);
    if (pkg.scripts) {
      parts.push('  Scripts:');
      for (const [name, cmd] of Object.entries(pkg.scripts).slice(0, 10)) {
        parts.push(`    ${name}: ${cmd}`);
      }
    }
    parts.push('');
  }

  parts.push('[USER REQUEST]');
  parts.push('');

  return parts.join('\n');
}
