// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { contextBridge, ipcRenderer } from 'electron';
import { TedEvent } from './types/protocol';

/**
 * Preload script - exposes safe IPC API to renderer
 */

export interface RecentProject {
  path: string;
  name: string;
  lastOpened: number;
}

export interface ProjectContext {
  fileTree: string[];
  readme: string | null;
  packageJson: any | null;
  lastScanned: number;
}

export interface HardwareInfo {
  tier: string;
  tierDescription: string;
  cpuBrand: string;
  cpuCores: number;
  ramGb: number;
  hasSsd: boolean;
  architecture: string;
  isSbc: boolean;
  cpuYear: number | null;
  recommendedModels: string[];
  expectedResponseTime: [number, number];
  capabilities: string[];
  limitations: string[];
}

export interface TedSettings {
  provider: string;
  model: string;
  anthropicApiKey: string;
  anthropicModel: string;
  ollamaBaseUrl: string;
  ollamaModel: string;
  openrouterApiKey: string;
  openrouterModel: string;
  vercelToken: string;
  netlifyToken: string;
  hardware: HardwareInfo | null;
}

export interface SessionInfo {
  id: string;
  projectPath: string;
  name: string;
  lastActive: number;
  created: number;
  messageCount: number;
  summary?: string;
}

export interface DeploymentOptions {
  projectPath: string;
  vercelToken: string;
  projectName?: string;
  envVars?: Record<string, string>;
}

export interface DeploymentResult {
  success: boolean;
  url?: string;
  deploymentId?: string;
  error?: string;
}

export interface DeploymentStatus {
  id: string;
  url: string;
  state: 'INITIALIZING' | 'ANALYZING' | 'BUILDING' | 'DEPLOYING' | 'READY' | 'ERROR' | 'CANCELED';
  readyState: 'INITIALIZING' | 'ANALYZING' | 'BUILDING' | 'DEPLOYING' | 'READY' | 'ERROR' | 'CANCELED';
  createdAt: number;
}

export interface NetlifyDeploymentOptions {
  projectPath: string;
  netlifyToken: string;
  siteName?: string;
  envVars?: Record<string, string>;
}

export interface NetlifyDeploymentResult {
  success: boolean;
  url?: string;
  deployId?: string;
  siteId?: string;
  error?: string;
}

export interface NetlifyDeploymentStatus {
  id: string;
  url: string;
  state: 'new' | 'pending_review' | 'accepted' | 'building' | 'enqueued' | 'uploading' | 'uploaded' | 'preparing' | 'prepared' | 'processing' | 'processed' | 'ready' | 'error' | 'retrying';
  siteId: string;
  siteName: string;
  createdAt: string;
  errorMessage?: string;
}

export interface TeddyAPI {
  // Dialog
  openFolderDialog: () => Promise<string | null>;

  // Project
  setProject: (path: string) => Promise<{ success: boolean; path: string; historyLength: number }>;
  getProject: () => Promise<{ path: string | null; hasProject: boolean }>;
  getRecentProjects: () => Promise<RecentProject[]>;
  getLastProject: () => Promise<{ path: string; name: string } | null>;
  clearLastProject: () => Promise<{ success: boolean }>;
  removeRecentProject: (path: string) => Promise<{ success: boolean }>;
  getProjectContext: () => Promise<ProjectContext | null>;

  // Review mode - when enabled, file operations are not auto-applied
  setReviewMode: (enabled: boolean) => Promise<{ success: boolean }>;
  getReviewMode: () => Promise<{ enabled: boolean }>;

  // Session management
  listSessions: () => Promise<SessionInfo[]>;
  createSession: () => Promise<{ id: string; projectPath: string }>;
  switchSession: (sessionId: string) => Promise<{ id: string; historyLength: number }>;
  getCurrentSession: () => Promise<SessionInfo | null>;
  deleteSession: (sessionId: string) => Promise<{ success: boolean }>;

  // Ted
  runTed: (prompt: string, options?: {
    trust?: boolean;
    provider?: string;
    model?: string;
    caps?: string[];
    newConversation?: boolean; // Set to true to start fresh conversation
  }) => Promise<{ success: boolean }>;
  stopTed: () => Promise<{ success: boolean }>;
  clearHistory: () => Promise<{ success: boolean }>;
  getHistoryLength: () => Promise<{ length: number }>;

  // File operations
  readFile: (path: string) => Promise<{ content: string }>;
  writeFile: (path: string, content: string) => Promise<{ success: boolean }>;
  deleteFile: (path: string) => Promise<{ success: boolean }>;
  listFiles: (dirPath?: string) => Promise<Array<{
    name: string;
    isDirectory: boolean;
    path: string;
  }>>;

  // Shell operations (run commands directly without Ted)
  runShell: (command: string) => Promise<{ success: boolean; pid?: number }>;
  killShell: (pid: number) => Promise<{ success: boolean }>;

  // Cache operations
  clearCache: () => Promise<{ success: boolean; error?: string }>;

  // Settings
  getSettings: () => Promise<TedSettings>;
  saveSettings: (settings: TedSettings) => Promise<{ success: boolean }>;
  detectHardware: () => Promise<HardwareInfo>;

  // Deployment - Vercel
  deployVercel: (options: DeploymentOptions) => Promise<DeploymentResult>;
  verifyVercelToken: (token: string) => Promise<{ valid: boolean; error?: string }>;
  getVercelDeploymentStatus: (deploymentId: string, token: string) => Promise<DeploymentStatus>;

  // Deployment - Netlify
  deployNetlify: (options: NetlifyDeploymentOptions) => Promise<NetlifyDeploymentResult>;
  verifyNetlifyToken: (token: string) => Promise<{ valid: boolean; error?: string }>;
  getNetlifyDeploymentStatus: (deployId: string, token: string) => Promise<NetlifyDeploymentStatus>;

  // Cloudflare Tunnel
  tunnelIsInstalled: () => Promise<{ installed: boolean }>;
  tunnelGetInstallInstructions: () => Promise<{ instructions: string }>;
  tunnelAutoInstall: () => Promise<{ success: boolean; path?: string; error?: string }>;
  tunnelStart: (options: { port: number; subdomain?: string }) => Promise<DeploymentResult>;
  tunnelStop: (port: number) => Promise<{ success: boolean }>;
  tunnelGetUrl: (port: number) => Promise<{ url: string | null }>;

  // teddy.rocks Share Service
  shareStart: (options: { port: number; projectName?: string; customSlug?: string }) => Promise<{
    success: boolean;
    slug?: string;
    previewUrl?: string;
    tunnelUrl?: string;
    error?: string;
  }>;
  shareStop: (port: number) => Promise<{ success: boolean }>;
  shareGet: (port: number) => Promise<{ slug: string; previewUrl: string } | null>;
  shareGetAll: () => Promise<Array<{ port: number; slug: string; previewUrl: string }>>;
  shareGenerateSlug: (projectName?: string) => Promise<string>;
  shareCheckSlug: (slug: string) => Promise<boolean>;

  // Event listeners
  onTedEvent: (callback: (event: TedEvent) => void) => () => void;
  onTedStderr: (callback: (text: string) => void) => () => void;
  onTedError: (callback: (error: string) => void) => () => void;
  onTedExit: (callback: (info: { code: number | null; signal: string | null }) => void) => () => void;
  onFileChanged: (callback: (info: { type: string; path: string }) => void) => () => void;
  onFileExternalChange: (callback: (event: { type: string; path: string; relativePath: string }) => void) => () => void;
  onGitCommitted: (callback: (info: { files: string[]; summary: string }) => void) => () => void;
}

const api: TeddyAPI = {
  // Dialog
  openFolderDialog: () => ipcRenderer.invoke('dialog:openFolder'),

  // Project
  setProject: (path: string) => ipcRenderer.invoke('project:set', path),
  getProject: () => ipcRenderer.invoke('project:get'),
  getRecentProjects: () => ipcRenderer.invoke('project:getRecent'),
  getLastProject: () => ipcRenderer.invoke('project:getLast'),
  clearLastProject: () => ipcRenderer.invoke('project:clearLast'),
  removeRecentProject: (path: string) => ipcRenderer.invoke('project:removeRecent', path),
  getProjectContext: () => ipcRenderer.invoke('project:getContext'),

  // Review mode
  setReviewMode: (enabled: boolean) => ipcRenderer.invoke('review:set', enabled),
  getReviewMode: () => ipcRenderer.invoke('review:get'),

  // Session management
  listSessions: () => ipcRenderer.invoke('session:list'),
  createSession: () => ipcRenderer.invoke('session:create'),
  switchSession: (sessionId: string) => ipcRenderer.invoke('session:switch', sessionId),
  getCurrentSession: () => ipcRenderer.invoke('session:getCurrent'),
  deleteSession: (sessionId: string) => ipcRenderer.invoke('session:delete', sessionId),

  // Ted
  runTed: (prompt: string, options) => ipcRenderer.invoke('ted:run', prompt, options),
  stopTed: () => ipcRenderer.invoke('ted:stop'),
  clearHistory: () => ipcRenderer.invoke('ted:clearHistory'),
  getHistoryLength: () => ipcRenderer.invoke('ted:getHistoryLength'),

  // File operations
  readFile: (path: string) => ipcRenderer.invoke('file:read', path),
  writeFile: (path: string, content: string) => ipcRenderer.invoke('file:write', path, content),
  deleteFile: (path: string) => ipcRenderer.invoke('file:delete', path),
  listFiles: (dirPath?: string) => ipcRenderer.invoke('file:list', dirPath),

  // Shell operations
  runShell: (command: string) => ipcRenderer.invoke('shell:run', command),
  killShell: (pid: number) => ipcRenderer.invoke('shell:kill', pid),

  // Cache operations
  clearCache: () => ipcRenderer.invoke('cache:clear'),

  // Settings
  getSettings: () => ipcRenderer.invoke('settings:get'),
  saveSettings: (settings: TedSettings) => ipcRenderer.invoke('settings:save', settings),
  detectHardware: () => ipcRenderer.invoke('settings:detectHardware'),

  // Deployment - Vercel
  deployVercel: (options: DeploymentOptions) => ipcRenderer.invoke('deploy:vercel', options),
  verifyVercelToken: (token: string) => ipcRenderer.invoke('deploy:verifyVercelToken', token),
  getVercelDeploymentStatus: (deploymentId: string, token: string) =>
    ipcRenderer.invoke('deploy:getVercelStatus', deploymentId, token),

  // Deployment - Netlify
  deployNetlify: (options: NetlifyDeploymentOptions) => ipcRenderer.invoke('deploy:netlify', options),
  verifyNetlifyToken: (token: string) => ipcRenderer.invoke('deploy:verifyNetlifyToken', token),
  getNetlifyDeploymentStatus: (deployId: string, token: string) =>
    ipcRenderer.invoke('deploy:getNetlifyStatus', deployId, token),

  // Cloudflare Tunnel
  tunnelIsInstalled: () => ipcRenderer.invoke('tunnel:isInstalled'),
  tunnelGetInstallInstructions: () => ipcRenderer.invoke('tunnel:getInstallInstructions'),
  tunnelAutoInstall: () => ipcRenderer.invoke('tunnel:autoInstall'),
  tunnelStart: (options) => ipcRenderer.invoke('tunnel:start', options),
  tunnelStop: (port: number) => ipcRenderer.invoke('tunnel:stop', port),
  tunnelGetUrl: (port: number) => ipcRenderer.invoke('tunnel:getUrl', port),

  // teddy.rocks Share Service
  shareStart: (options) => ipcRenderer.invoke('share:start', options),
  shareStop: (port: number) => ipcRenderer.invoke('share:stop', port),
  shareGet: (port: number) => ipcRenderer.invoke('share:get', port),
  shareGetAll: () => ipcRenderer.invoke('share:getAll'),
  shareGenerateSlug: (projectName?: string) => ipcRenderer.invoke('share:generateSlug', projectName),
  shareCheckSlug: (slug: string) => ipcRenderer.invoke('share:checkSlug', slug),

  // Event listeners
  onTedEvent: (callback) => {
    const listener = (_: any, event: TedEvent) => callback(event);
    ipcRenderer.on('ted:event', listener);
    return () => ipcRenderer.removeListener('ted:event', listener);
  },

  onTedStderr: (callback) => {
    const listener = (_: any, text: string) => callback(text);
    ipcRenderer.on('ted:stderr', listener);
    return () => ipcRenderer.removeListener('ted:stderr', listener);
  },

  onTedError: (callback) => {
    const listener = (_: any, error: string) => callback(error);
    ipcRenderer.on('ted:error', listener);
    return () => ipcRenderer.removeListener('ted:error', listener);
  },

  onTedExit: (callback) => {
    const listener = (_: any, info: { code: number | null; signal: string | null }) => callback(info);
    ipcRenderer.on('ted:exit', listener);
    return () => ipcRenderer.removeListener('ted:exit', listener);
  },

  onFileChanged: (callback) => {
    const listener = (_: any, info: { type: string; path: string }) => callback(info);
    ipcRenderer.on('file:changed', listener);
    return () => ipcRenderer.removeListener('file:changed', listener);
  },

  onFileExternalChange: (callback) => {
    const listener = (_: any, event: { type: string; path: string; relativePath: string }) => callback(event);
    ipcRenderer.on('file:externalChange', listener);
    return () => ipcRenderer.removeListener('file:externalChange', listener);
  },

  onGitCommitted: (callback) => {
    const listener = (_: any, info: { files: string[]; summary: string }) => callback(info);
    ipcRenderer.on('git:committed', listener);
    return () => ipcRenderer.removeListener('git:committed', listener);
  },
};

// Expose API to renderer
contextBridge.exposeInMainWorld('teddy', api);

// Type declaration for window.teddy
declare global {
  interface Window {
    teddy: TeddyAPI;
  }
}
