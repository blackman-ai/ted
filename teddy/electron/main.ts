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
import {
  type TedSettings,
  loadTedSettings,
  saveTedSettings,
  detectHardware,
  ensureLocalModelInstalled,
  setupRecommendedLocalModel,
} from './settings/ted-settings';
import { deployProject, verifyToken, getDeploymentStatus } from './deploy/vercel';
import { deployNetlifyProject, verifyNetlifyToken, getNetlifyDeploymentStatus } from './deploy/netlify';
import { detectScaffold, generateScaffoldPrompt } from './scaffolds';
import { writeSystemPromptFile, cleanupSystemPromptFile, TeddyPromptOptions, SessionState } from './ted/system-prompts';

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

let dockerModule: typeof import('./docker') | null = null;
async function getDockerModule() {
  if (!dockerModule) {
    dockerModule = await import('./docker');
  }
  return dockerModule;
}

// Track running shell processes (preview servers, etc.) for cleanup
const runningShells: Map<number, import('child_process').ChildProcess> = new Map();

// Type imports for file watcher
type FileWatcherType = import('./file-watcher').FileWatcher;
type FileChangeEventType = import('./file-watcher').FileChangeEvent;

// Debug logging to file for Claude Code integration
const LOG_FILE = '/tmp/teddy-debug.log';
const REDACTED = '[REDACTED]';

function log(...args: any[]) {
  const timestamp = new Date().toISOString();
  const message = `[${timestamp}] ${args.map(a => typeof a === 'object' ? JSON.stringify(a, null, 2) : a).join(' ')}\n`;
  appendFileSync(LOG_FILE, message);
  console.log(...args);
}

function redactSecret(value: unknown): unknown {
  if (typeof value !== 'string' || value.length === 0) {
    return value;
  }
  return REDACTED;
}

function sanitizeSettingsForLog(settings: any): any {
  if (!settings || typeof settings !== 'object') {
    return settings;
  }

  return {
    ...settings,
    anthropicApiKey: redactSecret(settings.anthropicApiKey),
    openrouterApiKey: redactSecret(settings.openrouterApiKey),
    blackmanApiKey: redactSecret(settings.blackmanApiKey),
    vercelToken: redactSecret(settings.vercelToken),
    netlifyToken: redactSecret(settings.netlifyToken),
  };
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
let reviewModeEnabled: boolean = false; // Default to auto-apply for non-technical users

// Conversation history for multi-turn chats
interface HistoryMessage {
  role: 'user' | 'assistant';
  content: string;
}
interface ConversationMemoryRecord {
  id: string;
  timestamp: string;
  summary: string;
  files_changed: string[];
  tags: string[];
  content: string;
}
let conversationHistory: HistoryMessage[] = [];
let historyFilePath: string | null = null;
let systemPromptFilePath: string | null = null; // Teddy's opinionated system prompt

// Session state tracking - Teddy tracks this and passes it to Ted explicitly
// This replaces the old inference-based enforcement logic in Ted
let filesCreatedThisSession: string[] = [];
let filesEditedThisSession: string[] = [];
let conversationTurns: number = 0;
const MAX_MEMORY_RESULTS = 200;
const MAX_FILE_SEARCH_RESULTS = 500;
const FILE_TREE_EXCLUDED_NAMES = new Set(['node_modules', 'target', 'dist', 'build']);

function hasBuildIntent(prompt: string): boolean {
  const normalized = prompt.trim().toLowerCase();
  if (!normalized) {
    return false;
  }

  const conversationalPhrases = new Set([
    'hi',
    'hello',
    'hey',
    'yo',
    'sup',
    'thanks',
    'thank you',
    'ok',
    'okay',
    'are you there',
    'you there',
    'test',
    'ping',
    'um what',
    'what?',
  ]);
  if (conversationalPhrases.has(normalized)) {
    return false;
  }

  const buildKeywords = [
    'build',
    'make',
    'create',
    'generate',
    'scaffold',
    'start',
    'setup',
    'set up',
    'app',
    'application',
    'website',
    'site',
    'blog',
    'landing page',
    'portfolio',
    'dashboard',
    'game',
    'api',
    'script',
    'tool',
    'project',
    'page',
    'fix',
    'edit',
    'update',
    'change',
    'run',
    'install',
  ];

  return buildKeywords.some((keyword) => normalized.includes(keyword));
}

function summarizeHistory(messages: HistoryMessage[]): string {
  const assistantReply = messages
    .filter((m) => m.role === 'assistant')
    .map((m) => m.content.trim())
    .find(Boolean);
  const userPrompt = messages
    .filter((m) => m.role === 'user')
    .map((m) => m.content.trim())
    .find(Boolean);

  const seed = assistantReply || userPrompt || 'Conversation';
  const singleLine = seed.replace(/\s+/g, ' ').trim();
  return singleLine.length <= 120 ? singleLine : `${singleLine.slice(0, 117)}...`;
}

function formatMemoryContent(messages: HistoryMessage[]): string {
  return messages
    .slice(-12)
    .map((m) => `${m.role === 'user' ? 'User' : 'Assistant'}: ${m.content.trim()}`)
    .join('\n\n');
}

function extractFilesFromHistory(messages: HistoryMessage[]): string[] {
  const fileSet = new Set<string>();
  const filePattern = /\b(?:\.{0,2}\/)?[A-Za-z0-9._/-]+\.[A-Za-z0-9]{1,8}\b/g;

  for (const message of messages) {
    for (const match of message.content.matchAll(filePattern)) {
      const candidate = match[0];
      if (!candidate.includes('://') && candidate.length < 180) {
        fileSet.add(candidate);
      }
    }
  }

  return Array.from(fileSet).slice(0, 20);
}

function extractTags(messages: HistoryMessage[], filesChanged: string[]): string[] {
  const combined = messages.map((m) => m.content.toLowerCase()).join('\n');
  const tags = new Set<string>();
  const tagKeywords: Array<[string, string[]]> = [
    ['bugfix', ['fix', 'bug', 'error', 'exception']],
    ['refactor', ['refactor', 'cleanup', 'restructure']],
    ['tests', ['test', 'coverage', 'assert']],
    ['docs', ['docs', 'readme', 'documentation']],
    ['ui', ['ui', 'component', 'css', 'style', 'layout']],
    ['backend', ['api', 'server', 'database', 'sql']],
    ['performance', ['perf', 'optimize', 'latency', 'memory']],
  ];

  for (const [tag, keywords] of tagKeywords) {
    if (keywords.some((keyword) => combined.includes(keyword))) {
      tags.add(tag);
    }
  }

  if (filesChanged.some((f) => f.endsWith('.rs'))) {
    tags.add('rust');
  }
  if (filesChanged.some((f) => f.endsWith('.ts') || f.endsWith('.tsx'))) {
    tags.add('typescript');
  }
  if (filesChanged.some((f) => f.endsWith('.js') || f.endsWith('.jsx'))) {
    tags.add('javascript');
  }

  return Array.from(tags).slice(0, 8);
}

function scoreMemory(memory: ConversationMemoryRecord, query: string): number {
  const normalizedQuery = query.toLowerCase().trim();
  if (!normalizedQuery) {
    return 0;
  }

  const queryTerms = normalizedQuery.split(/\s+/).filter(Boolean);
  const summary = memory.summary.toLowerCase();
  const content = memory.content.toLowerCase();
  const tags = memory.tags.map((tag) => tag.toLowerCase());
  const files = memory.files_changed.map((file) => file.toLowerCase());

  let score = 0;
  if (summary.includes(normalizedQuery)) score += 25;
  if (content.includes(normalizedQuery)) score += 12;
  if (tags.some((tag) => tag.includes(normalizedQuery))) score += 10;
  if (files.some((file) => file.includes(normalizedQuery))) score += 8;

  for (const term of queryTerms) {
    if (summary.includes(term)) score += 8;
    if (content.includes(term)) score += 3;
    if (tags.some((tag) => tag.includes(term))) score += 4;
    if (files.some((file) => file.includes(term))) score += 3;
  }

  return score;
}

function normalizeRelativePath(value: string): string {
  return value.replace(/\\/g, '/');
}

function resolveProjectPath(projectRoot: string, requestedPath: string): string {
  const normalizedInput = requestedPath.trim().length > 0 ? requestedPath : '.';
  const resolvedProjectRoot = path.resolve(projectRoot);
  const resolvedPath = path.resolve(resolvedProjectRoot, normalizedInput);

  if (
    resolvedPath !== resolvedProjectRoot &&
    !resolvedPath.startsWith(`${resolvedProjectRoot}${path.sep}`)
  ) {
    throw new Error('Path escapes project root');
  }

  return resolvedPath;
}

function toProjectRelativePath(projectRoot: string, targetPath: string): string {
  const resolvedProjectRoot = path.resolve(projectRoot);
  const relativePath = path.relative(resolvedProjectRoot, targetPath);
  if (!relativePath || relativePath === '.') {
    return '.';
  }
  return normalizeRelativePath(relativePath);
}

function shouldSkipTreeEntry(name: string): boolean {
  return name.startsWith('.') || FILE_TREE_EXCLUDED_NAMES.has(name);
}

async function loadProjectMemories(limit: number): Promise<ConversationMemoryRecord[]> {
  if (!currentProjectRoot) {
    return [];
  }

  const boundedLimit = Math.max(1, Math.min(limit, MAX_MEMORY_RESULTS));
  const sessions = await storage.getProjectSessions(currentProjectRoot);
  const memories: ConversationMemoryRecord[] = [];

  for (const session of sessions) {
    const history = await storage.loadSessionHistory(session.id);
    if (!history || history.length === 0) {
      continue;
    }

    const filesChanged = extractFilesFromHistory(history);
    const summary = session.summary?.trim() || summarizeHistory(history);
    const content = formatMemoryContent(history);
    memories.push({
      id: session.id,
      timestamp: new Date(session.lastActive || session.created || Date.now()).toISOString(),
      summary,
      files_changed: filesChanged,
      tags: extractTags(history, filesChanged),
      content,
    });
  }

  if (memories.length === 0 && conversationHistory.length > 0) {
    const filesChanged = extractFilesFromHistory(conversationHistory);
    memories.push({
      id: currentSessionId || `live-${Date.now()}`,
      timestamp: new Date().toISOString(),
      summary: summarizeHistory(conversationHistory),
      files_changed: filesChanged,
      tags: extractTags(conversationHistory, filesChanged),
      content: formatMemoryContent(conversationHistory),
    });
  }

  memories.sort((a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp));
  return memories.slice(0, boundedLimit);
}

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
    mainWindow.loadURL('http://localhost:5174');  // Must match electron.vite.config.ts port
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

  // Review mode migration:
  // - Existing installs had review mode default ON.
  // - For non-technical users, default to auto-apply unless they've explicitly set a preference.
  try {
    const config = await storage.getConfig();
    if (config.reviewModeUserConfigured) {
      reviewModeEnabled = config.reviewModeEnabled;
    } else {
      reviewModeEnabled = false;
      await storage.updateConfig({
        reviewModeEnabled: false,
        reviewModeUserConfigured: false,
      });
    }
    log('[REVIEW_MODE] Initialized:', reviewModeEnabled, 'userConfigured:', config.reviewModeUserConfigured);
  } catch (err) {
    log('[REVIEW_MODE] Failed to load from config, using default:', reviewModeEnabled, err);
  }

  createWindow();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// Cleanup shares, tunnels, and shell processes when app is quitting
app.on('before-quit', async () => {
  log('[APP] Cleaning up before quit...');

  // Kill all running shell processes (preview servers, etc.)
  for (const [pid] of runningShells) {
    try {
      process.kill(-pid, 'SIGTERM'); // Kill process group
      log('[APP] Killed shell process group:', pid);
    } catch (err) {
      // Process might already be dead
    }
  }
  runningShells.clear();

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

  // Clear session state when switching projects
  filesCreatedThisSession = [];
  filesEditedThisSession = [];
  conversationTurns = 0;

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

  // Clear conversation history and session state for new session
  conversationHistory = [];
  filesCreatedThisSession = [];
  filesEditedThisSession = [];
  conversationTurns = 0;

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

  // Clear session state when switching sessions (we don't track file operations per-session yet)
  filesCreatedThisSession = [];
  filesEditedThisSession = [];
  conversationTurns = 0;

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
 * Get recent conversation memories for current project
 */
ipcMain.handle('memory:getRecent', async (_, limit?: number) => {
  try {
    return await loadProjectMemories(limit ?? 50);
  } catch (err) {
    log('[MEMORY:GET_RECENT] Failed:', err);
    return [];
  }
});

/**
 * Search conversation memories using lexical scoring
 */
ipcMain.handle('memory:search', async (_, query: string, limit?: number) => {
  try {
    const q = String(query || '').trim();
    const maxResults = Math.max(1, Math.min(limit ?? 20, MAX_MEMORY_RESULTS));
    const memories = await loadProjectMemories(MAX_MEMORY_RESULTS);

    if (!q) {
      return memories.slice(0, maxResults);
    }

    return memories
      .map((memory) => ({ memory, score: scoreMemory(memory, q) }))
      .filter((entry) => entry.score > 0)
      .sort((a, b) => b.score - a.score || Date.parse(b.memory.timestamp) - Date.parse(a.memory.timestamp))
      .slice(0, maxResults)
      .map((entry) => entry.memory);
  } catch (err) {
    log('[MEMORY:SEARCH] Failed:', err);
    return [];
  }
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

  // Clear history and session state if starting a new conversation
  if (options?.newConversation) {
    log('[TED:RUN] Clearing history and session state for new conversation');
    conversationHistory = [];
    filesCreatedThisSession = [];
    filesEditedThisSession = [];
    conversationTurns = 0;
  }

  // If this is a new conversation (no history), inject project context into the prompt
  let finalPrompt = prompt;
  let projectHasFiles = false;
  let filesInContext: string[] = [];
  const buildIntent = hasBuildIntent(prompt);
  log('[TED:RUN] Build intent detected:', buildIntent);
  log('[TED:RUN] Tools enabled for turn:', buildIntent);
  const context = await storage.loadProjectContext(currentProjectRoot);
  if (context && context.fileTree.length > 0) {
    projectHasFiles = true;
    if (conversationHistory.length === 0 && buildIntent) {
      const { prefix, filesIncluded } = buildContextPrefix(context, currentProjectRoot);
      finalPrompt = prefix + prompt;
      filesInContext = filesIncluded;
      log('[TED:RUN] Injected project context into prompt, files included:', filesIncluded.length);
    } else if (conversationHistory.length === 0) {
      log('[TED:RUN] Conversation mode in existing project - skipping heavy project context injection');
    }
  } else {
    // If nothing has been built yet, stale assistant chatter tends to derail small local models.
    // Reset history so empty-project builder guidance is applied consistently.
    if (
      conversationHistory.length > 0 &&
      filesCreatedThisSession.length === 0 &&
      filesEditedThisSession.length === 0
    ) {
      log(
        '[TED:RUN] Resetting stale history for empty project with no created files:',
        conversationHistory.length,
        'messages'
      );
      conversationHistory = [];
      await storage.saveProjectHistory(currentProjectRoot, conversationHistory);
    }

    if (conversationHistory.length === 0 && buildIntent) {
      // EMPTY PROJECT: Inject comprehensive app-building guidance for Teddy
      // This is what makes Teddy work like Lovable/Replit - it builds complete apps

      // Try to detect the best scaffold for the user's request
      const scaffold = detectScaffold(prompt);
      let scaffoldSection = '';

      if (scaffold) {
        scaffoldSection = generateScaffoldPrompt(scaffold, prompt);
        log('[TED:RUN] Detected scaffold:', scaffold.name, 'for request');
      }

      const emptyProjectPrefix = `[NEW PROJECT - EMPTY DIRECTORY]
This is a brand new, empty project directory. There are NO existing files.

You are Teddy, an AI app builder. You BUILD complete, working applications - not just give advice.

## CRITICAL: DO NOT SEARCH FOR FILES
There are no files here. Do NOT use glob or file_read - they will return nothing.
Go straight to CREATING files with file_write.

## YOUR WORKFLOW
1. START CREATING FILES IMMEDIATELY using file_write
2. Create ALL necessary files for a complete, working app
3. Tell the user how to run it
4. Do NOT ask clarifying questions unless absolutely necessary

## FILE QUALITY REQUIREMENTS
Every file must be COMPLETE and FUNCTIONAL:
- Full implementations, not placeholders
- Real styling, not bare HTML
- Working logic, not TODO comments
- Proper error handling

## HOW TO CREATE FILES
Call file_write for each file. Example:
- file_write with path="index.html" and content="<!DOCTYPE html>..."
- file_write with path="styles.css" and content="* { box-sizing... }"
- file_write with path="app.js" and content="// App logic..."

## REMEMBER
- You are a BUILDER - create files, don't explain how
- Use file_write for each file, not code blocks in chat
- Every app should work immediately
- Include good styling - make it look professional
${scaffoldSection}

User request: `;
      finalPrompt = emptyProjectPrefix + prompt;
      log('[TED:RUN] Injected empty project guidance into prompt');
    } else if (conversationHistory.length === 0) {
      log('[TED:RUN] Conversation mode for non-build request in empty project');
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

  // Generate Teddy's opinionated system prompt
  // This injects Teddy-specific guidance (tech stack preferences, multi-file coordination, etc.)
  // without modifying Ted's core - Ted remains agnostic, Teddy provides the opinions

  // Load settings to get user preferences
  const tedSettings = await loadTedSettings();
  const userExperienceLevel = tedSettings.experienceLevel || 'beginner';
  const hardwareTier = (tedSettings.hardware?.tier?.toLowerCase() || 'medium') as TeddyPromptOptions['hardwareTier'];

  // Resolve provider/model actually in use for this run.
  // ChatPanel typically doesn't pass explicit options, so we derive from saved settings.
  const effectiveProvider = options?.provider || tedSettings.provider || 'anthropic';
  const effectiveModel = options?.model || (() => {
    switch (effectiveProvider) {
      case 'local':
        return tedSettings.localModel || tedSettings.model;
      case 'openrouter':
        return tedSettings.openrouterModel || tedSettings.model;
      case 'blackman':
        return tedSettings.blackmanModel || tedSettings.model;
      case 'anthropic':
      default:
        return tedSettings.anthropicModel || tedSettings.model;
    }
  })();

  // Determine model capability based on model name
  // Small models (1.5b-3b) need more explicit guidance
  // Medium models (7b-14b) can handle standard guidance
  // Large models (32b+, cloud APIs) can handle complex guidance
  const modelName = (effectiveModel || '').toLowerCase();
  let modelCapability: 'small' | 'medium' | 'large' = 'medium';
  if (modelName.includes('1.5b') || modelName.includes('3b') || modelName.includes('phi')) {
    modelCapability = 'small';
  } else if (modelName.includes('70b') || modelName.includes('claude') || modelName.includes('gpt-4')) {
    modelCapability = 'large';
  }
  log('[TED:RUN] Effective provider/model:', effectiveProvider, effectiveModel || 'default');
  log('[TED:RUN] Model capability detected:', modelCapability, 'for model:', modelName || 'default');
  log('[TED:RUN] Experience level:', userExperienceLevel, 'Hardware tier:', hardwareTier);

  const promptOptions: TeddyPromptOptions = {
    hardwareTier,
    projectHasFiles,
    modelCapability,
    experienceLevel: userExperienceLevel,
    buildIntent,
  };

  // Build session state to pass explicit facts to Ted
  // This replaces the inference-based enforcement logic that was removed from Ted
  const promptLower = prompt.toLowerCase();
  const userReportingBug = filesCreatedThisSession.length > 0 && (
    promptLower.includes('bug') ||
    promptLower.includes('broken') ||
    promptLower.includes('not working') ||
    promptLower.includes('doesn\'t work') ||
    promptLower.includes('error') ||
    promptLower.includes('fix') ||
    promptLower.includes('wrong') ||
    promptLower.includes('issue')
  );

  // Get list of project files for session context
  const projectFiles = context?.fileTree || [];

  const sessionState: SessionState = {
    projectFiles,
    filesCreatedThisSession,
    filesEditedThisSession,
    userReportingBug,
    conversationTurns,
  };

  // Increment turn counter for next time
  conversationTurns++;

  systemPromptFilePath = writeSystemPromptFile(promptOptions, sessionState);
  log('[TED:RUN] Generated Teddy system prompt file:', systemPromptFilePath);
  log('[TED:RUN] Session state:', JSON.stringify(sessionState, null, 2));

  tedRunner = new TedRunner({
    workingDirectory: currentProjectRoot,
    trust: options?.trust ?? false,
    noTools: !buildIntent,
    provider: effectiveProvider,
    model: effectiveModel,
    caps: options?.caps,
    historyFile: historyFilePath ?? undefined,
    reviewMode: reviewModeEnabled,  // Pass review mode to Ted - it will emit events but skip file execution
    sessionId: currentSessionId ?? undefined,  // Pass session ID to resume
    projectHasFiles,  // Tell Ted if project has files (for enforcement logic)
    systemPromptFile: systemPromptFilePath,  // Teddy's opinionated defaults
    filesInContext,  // Files already in context - Ted won't re-read them
    // Pass API keys from settings to Ted (via environment variables)
    anthropicApiKey: tedSettings.anthropicApiKey || undefined,
    blackmanApiKey: tedSettings.blackmanApiKey || undefined,
    blackmanBaseUrl: tedSettings.blackmanBaseUrl || undefined,
    openrouterApiKey: tedSettings.openrouterApiKey || undefined,
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
    // Track file creation for session state (used in next turn's system prompt)
    const filePath = event.data.path;
    if (!filesCreatedThisSession.includes(filePath)) {
      filesCreatedThisSession.push(filePath);
      log('[SESSION] Tracking file created:', filePath);
    }

    if (reviewModeEnabled) {
      // In review mode, just forward the event - don't apply
      log('[FILE_CREATE] Review mode - forwarding to renderer:', filePath);
      // The event is already forwarded via 'event' listener above
    } else {
      // Auto-apply when review mode is off
      try {
        await fileApplier?.applyCreate(event);
        mainWindow?.webContents.send('file:changed', {
          type: 'create',
          path: filePath,
        });
      } catch (err) {
        console.error('Failed to apply file create:', err);
      }
    }
  });

  tedRunner.on('file_edit', async (event) => {
    // Track file edit for session state (used in next turn's system prompt)
    const filePath = event.data.path;
    if (!filesEditedThisSession.includes(filePath) && !filesCreatedThisSession.includes(filePath)) {
      filesEditedThisSession.push(filePath);
      log('[SESSION] Tracking file edited:', filePath);
    }

    if (reviewModeEnabled) {
      // In review mode, just forward the event - don't apply
      log('[FILE_EDIT] Review mode - forwarding to renderer:', filePath);
    } else {
      try {
        await fileApplier?.applyEdit(event);
        mainWindow?.webContents.send('file:changed', {
          type: 'edit',
          path: filePath,
        });
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : String(err);
        console.error('Failed to apply file edit:', errorMsg);
        // Notify the user that the edit failed
        mainWindow?.webContents.send('ted:edit-failed', {
          path: filePath,
          error: errorMsg,
          operation: event.data.operation,
          // Include what the model tried to replace (truncated for display)
          oldText: event.data.old_text?.substring(0, 200),
        });
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

  const fullPath = resolveProjectPath(currentProjectRoot, filePath);
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

  const fullPath = resolveProjectPath(currentProjectRoot, filePath);

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

  const fullPath = resolveProjectPath(currentProjectRoot, filePath);

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

  const fullPath = resolveProjectPath(currentProjectRoot, dirPath);
  const baseRelativePath = toProjectRelativePath(currentProjectRoot, fullPath);
  const entries = await fs.readdir(fullPath, { withFileTypes: true });

  return entries.map((entry) => {
    const relativePath =
      baseRelativePath === '.'
        ? entry.name
        : normalizeRelativePath(path.join(baseRelativePath, entry.name));
    return {
      name: entry.name,
      isDirectory: entry.isDirectory(),
      path: relativePath,
    };
  });
});

/**
 * Search project files globally (not limited to loaded tree nodes)
 */
ipcMain.handle('file:search', async (_, query: string, limit: number = 200) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  const normalizedQuery = query.trim().toLowerCase();
  if (normalizedQuery.length === 0) {
    return [];
  }

  const boundedLimit = Math.max(1, Math.min(limit, MAX_FILE_SEARCH_RESULTS));
  const results: Array<{ name: string; isDirectory: boolean; path: string }> = [];
  const queue: string[] = ['.'];

  while (queue.length > 0 && results.length < boundedLimit) {
    const relativeDir = queue.shift() ?? '.';
    const fullDir = resolveProjectPath(currentProjectRoot, relativeDir);

    const entries = await fs.readdir(fullDir, { withFileTypes: true }).catch(() => null);
    if (!entries) {
      continue;
    }

    entries.sort((a, b) => {
      if (a.isDirectory() && !b.isDirectory()) return -1;
      if (!a.isDirectory() && b.isDirectory()) return 1;
      return a.name.localeCompare(b.name);
    });

    for (const entry of entries) {
      if (shouldSkipTreeEntry(entry.name)) {
        continue;
      }

      const relativePath =
        relativeDir === '.'
          ? entry.name
          : normalizeRelativePath(path.join(relativeDir, entry.name));
      const normalizedPath = relativePath.toLowerCase();

      if (
        entry.name.toLowerCase().includes(normalizedQuery) ||
        normalizedPath.includes(normalizedQuery)
      ) {
        results.push({
          name: entry.name,
          isDirectory: entry.isDirectory(),
          path: relativePath,
        });
        if (results.length >= boundedLimit) {
          break;
        }
      }

      if (entry.isDirectory()) {
        queue.push(relativePath);
      }
    }
  }

  return results;
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

  // Clean up temp system prompt file
  if (systemPromptFilePath) {
    cleanupSystemPromptFile(systemPromptFilePath);
    systemPromptFilePath = null;
  }
}

/**
 * Set review mode (enable/disable file operation review)
 */
ipcMain.handle('review:set', async (_, enabled: boolean) => {
  reviewModeEnabled = enabled;
  try {
    await storage.updateConfig({
      reviewModeEnabled: enabled,
      reviewModeUserConfigured: true,
    });
  } catch (err) {
    log('[REVIEW_MODE] Failed to persist setting:', err);
  }
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
  // Also clear session state so we start completely fresh
  filesCreatedThisSession = [];
  filesEditedThisSession = [];
  conversationTurns = 0;
  log('[HISTORY] Cleared conversation history and session state');

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

/**
 * Kill any process running on a specific port
 */
async function killProcessOnPort(port: number): Promise<void> {
  const { exec } = await import('child_process');
  const { promisify } = await import('util');
  const execAsync = promisify(exec);

  try {
    // Find PIDs listening on the port
    const { stdout } = await execAsync(`lsof -ti :${port}`);
    const pids = stdout.trim().split('\n').filter(Boolean);

    for (const pid of pids) {
      try {
        process.kill(parseInt(pid, 10), 'SIGTERM');
        log(`[SHELL] Killed process ${pid} on port ${port}`);
      } catch (err) {
        // Process might already be dead
      }
    }
  } catch (err) {
    // No process on port, which is fine
  }
}

/**
 * Run a shell command directly (without Ted)
 * If port is provided, kill any existing process on that port first
 */
ipcMain.handle('shell:run', async (_, command: string, port?: number) => {
  if (!currentProjectRoot) {
    throw new Error('No project selected');
  }

  // Kill any existing process on the port before starting
  if (port) {
    log('[SHELL:RUN] Killing any process on port', port);
    await killProcessOnPort(port);
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
 * Kill a running shell process and all its children
 */
ipcMain.handle('shell:kill', async (_, pid: number) => {
  log('[SHELL:KILL] PID:', pid);

  try {
    // Kill the entire process group (negative PID) since we spawn with detached: true
    // This ensures child processes like python3 http.server are also killed
    process.kill(-pid, 'SIGTERM');
    runningShells.delete(pid);
    return { success: true };
  } catch (err) {
    // If process group kill fails, try killing just the process
    try {
      process.kill(pid, 'SIGTERM');
      runningShells.delete(pid);
      return { success: true };
    } catch (innerErr) {
      log('[SHELL:KILL] Failed:', innerErr);
      return { success: false };
    }
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
  log('[SETTINGS:SAVE]', sanitizeSettingsForLog(settings));
  try {
    const requestedSettings = settings as TedSettings;
    const normalizedProvider =
      requestedSettings.provider === 'ollama' ? 'local' : requestedSettings.provider;
    const hasCustomLocalBaseUrl = Boolean(requestedSettings.localBaseUrl?.trim());
    const shouldEnsureManagedLocalModel =
      normalizedProvider === 'local' && !hasCustomLocalBaseUrl;

    if (shouldEnsureManagedLocalModel) {
      const installResult = await ensureLocalModelInstalled(
        requestedSettings.localModel,
        requestedSettings.hardware || null
      );

      if (!installResult.success || !installResult.model || !installResult.modelPath) {
        return {
          success: false,
          error: installResult.error || 'Failed to install selected local model',
        };
      }

      const updatedSettings: TedSettings = {
        ...requestedSettings,
        model: installResult.model,
        localModel: installResult.model,
        localBaseUrl: '',
        localModelPath: installResult.modelPath,
      };
      await saveTedSettings(updatedSettings);
      return {
        success: true,
        downloaded: installResult.downloaded,
        model: installResult.model,
        modelPath: installResult.modelPath,
        message: installResult.downloaded
          ? `Downloaded ${installResult.model}.`
          : `${installResult.model} already installed.`,
      };
    }

    await saveTedSettings(requestedSettings);
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
 * One-click local model setup (download + configure)
 */
ipcMain.handle('settings:setupRecommendedLocalModel', async () => {
  log('[SETTINGS:SETUP_LOCAL_MODEL]');
  try {
    const result = await setupRecommendedLocalModel();
    if (!result.success) {
      log('[SETTINGS:SETUP_LOCAL_MODEL] Failed:', result.error);
    }
    return result;
  } catch (err) {
    log('[SETTINGS:SETUP_LOCAL_MODEL] Failed:', err);
    return { success: false, error: String(err) };
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
 * Docker & PostgreSQL IPC Handlers
 */

/**
 * Get Docker status
 */
ipcMain.handle('docker:getStatus', async () => {
  log('[DOCKER:GET_STATUS]');
  try {
    const docker = await getDockerModule();
    const status = await docker.getDockerStatus();
    return status;
  } catch (err) {
    log('[DOCKER:GET_STATUS] Failed:', err);
    throw err;
  }
});

/**
 * Check if Docker is installed
 */
ipcMain.handle('docker:isInstalled', async () => {
  log('[DOCKER:IS_INSTALLED]');
  try {
    const docker = await getDockerModule();
    const installed = await docker.isDockerInstalled();
    return { installed };
  } catch (err) {
    log('[DOCKER:IS_INSTALLED] Failed:', err);
    throw err;
  }
});

/**
 * Get Docker installation instructions
 */
ipcMain.handle('docker:getInstallInstructions', async () => {
  log('[DOCKER:GET_INSTALL_INSTRUCTIONS]');
  try {
    const docker = await getDockerModule();
    const instructions = docker.getDockerInstallInstructions();
    return { instructions };
  } catch (err) {
    log('[DOCKER:GET_INSTALL_INSTRUCTIONS] Failed:', err);
    throw err;
  }
});

/**
 * Get Docker start instructions
 */
ipcMain.handle('docker:getStartInstructions', async () => {
  log('[DOCKER:GET_START_INSTRUCTIONS]');
  try {
    const docker = await getDockerModule();
    const instructions = docker.getDockerStartInstructions();
    return { instructions };
  } catch (err) {
    log('[DOCKER:GET_START_INSTRUCTIONS] Failed:', err);
    throw err;
  }
});

/**
 * Get PostgreSQL container status
 */
ipcMain.handle('postgres:getStatus', async () => {
  log('[POSTGRES:GET_STATUS]');
  try {
    const docker = await getDockerModule();
    const status = await docker.getPostgresStatus();
    return status;
  } catch (err) {
    log('[POSTGRES:GET_STATUS] Failed:', err);
    throw err;
  }
});

/**
 * Start PostgreSQL container
 */
ipcMain.handle('postgres:start', async (_, config?: { password?: string; port?: number; database?: string; user?: string }) => {
  log('[POSTGRES:START]', config);
  try {
    const docker = await getDockerModule();
    const result = await docker.startPostgresContainer(config);

    if (result.success && mainWindow) {
      mainWindow.webContents.send('postgres:started', {
        containerId: result.containerId,
        databaseUrl: result.databaseUrl,
      });
    }

    return result;
  } catch (err) {
    log('[POSTGRES:START] Failed:', err);
    throw err;
  }
});

/**
 * Stop PostgreSQL container
 */
ipcMain.handle('postgres:stop', async () => {
  log('[POSTGRES:STOP]');
  try {
    const docker = await getDockerModule();
    const result = await docker.stopPostgresContainer();

    if (result.success && mainWindow) {
      mainWindow.webContents.send('postgres:stopped', {});
    }

    return result;
  } catch (err) {
    log('[POSTGRES:STOP] Failed:', err);
    throw err;
  }
});

/**
 * Remove PostgreSQL container (preserves data)
 */
ipcMain.handle('postgres:remove', async () => {
  log('[POSTGRES:REMOVE]');
  try {
    const docker = await getDockerModule();
    const result = await docker.removePostgresContainer();
    return result;
  } catch (err) {
    log('[POSTGRES:REMOVE] Failed:', err);
    throw err;
  }
});

/**
 * Get PostgreSQL container logs
 */
ipcMain.handle('postgres:getLogs', async (_, lines?: number) => {
  log('[POSTGRES:GET_LOGS]', lines);
  try {
    const docker = await getDockerModule();
    const logs = await docker.getPostgresLogs(lines);
    return { logs };
  } catch (err) {
    log('[POSTGRES:GET_LOGS] Failed:', err);
    throw err;
  }
});

/**
 * Test PostgreSQL connection
 */
ipcMain.handle('postgres:testConnection', async () => {
  log('[POSTGRES:TEST_CONNECTION]');
  try {
    const docker = await getDockerModule();
    const result = await docker.testPostgresConnection();
    return result;
  } catch (err) {
    log('[POSTGRES:TEST_CONNECTION] Failed:', err);
    throw err;
  }
});

/**
 * Get PostgreSQL database URL
 */
ipcMain.handle('postgres:getDatabaseUrl', async () => {
  log('[POSTGRES:GET_DATABASE_URL]');
  try {
    const docker = await getDockerModule();
    const databaseUrl = docker.buildDatabaseUrl();
    return { databaseUrl };
  } catch (err) {
    log('[POSTGRES:GET_DATABASE_URL] Failed:', err);
    throw err;
  }
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

interface ContextPrefixResult {
  prefix: string;
  filesIncluded: string[];
}

/**
 * Build a context prefix to inject into the first prompt of a conversation.
 * For small projects, includes actual file contents so the model doesn't need to explore.
 * This is critical for smaller models that struggle with multi-step tool use.
 */
function buildContextPrefix(context: ProjectContext, projectRoot: string): ContextPrefixResult {
  const parts: string[] = [];
  const filesIncluded: string[] = [];

  // Source file extensions we care about
  const sourceExtensions = ['.html', '.htm', '.js', '.ts', '.css', '.json', '.jsx', '.tsx', '.vue', '.svelte'];

  // Filter to source files only
  const sourceFiles = context.fileTree.filter(file => {
    const ext = path.extname(file).toLowerCase();
    return sourceExtensions.includes(ext);
  });

  // For small projects (< 15 source files), include actual file contents
  // This eliminates the need for the model to explore with tool calls
  const isSmallProject = sourceFiles.length > 0 && sourceFiles.length <= 15;

  parts.push('[PROJECT CONTEXT]');

  if (isSmallProject) {
    parts.push('This is a small project. Here are all the source files with their complete contents:');
    parts.push('');

    // Read and include each source file
    for (const file of sourceFiles) {
      try {
        const fullPath = path.join(projectRoot, file);
        const content = require('fs').readFileSync(fullPath, 'utf-8');

        // Skip very large files (> 50KB)
        if (content.length > 50000) {
          parts.push(`=== ${file} ===`);
          parts.push(`[File too large: ${Math.round(content.length / 1024)}KB - use file_read tool]`);
          parts.push('');
          continue;
        }

        parts.push(`=== ${file} ===`);
        parts.push(content);
        parts.push('');
        filesIncluded.push(file);
      } catch (err) {
        // Skip files that can't be read
        log(`[CONTEXT] Could not read ${file}: ${err}`);
      }
    }

    parts.push('---');
    parts.push('');
    parts.push('IMPORTANT: You have ALL the source files above. Do NOT use file_read - you already have the contents.');
    parts.push('To fix issues, use file_edit with the EXACT text from the files shown above.');
    parts.push('');
  } else {
    // Large project - just show file list
    parts.push('This project has the following files:');
    parts.push('');

    const filesToShow = context.fileTree.slice(0, 50);
    for (const file of filesToShow) {
      parts.push(`  ${file}`);
    }
    if (context.fileTree.length > 50) {
      parts.push(`  ... and ${context.fileTree.length - 50} more files`);
    }
    parts.push('');
  }

  // Include README summary if available (only for large projects, small ones have the file)
  if (!isSmallProject && context.readme) {
    parts.push('README:');
    parts.push(context.readme);
    parts.push('');
  }

  // Include package.json info if available (only for large projects)
  if (!isSmallProject && context.packageJson) {
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

  return { prefix: parts.join('\n'), filesIncluded };
}
