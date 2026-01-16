// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { contextBridge, ipcRenderer } from 'electron';
import { TedEvent } from './types/protocol';

/**
 * Preload script - exposes safe IPC API to renderer
 */

export interface TeddyAPI {
  // Dialog
  openFolderDialog: () => Promise<string | null>;

  // Project
  setProject: (path: string) => Promise<{ success: boolean; path: string }>;
  getProject: () => Promise<{ path: string | null; hasProject: boolean }>;

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
  listFiles: (dirPath?: string) => Promise<Array<{
    name: string;
    isDirectory: boolean;
    path: string;
  }>>;

  // Event listeners
  onTedEvent: (callback: (event: TedEvent) => void) => () => void;
  onTedStderr: (callback: (text: string) => void) => () => void;
  onTedError: (callback: (error: string) => void) => () => void;
  onTedExit: (callback: (info: { code: number | null; signal: string | null }) => void) => () => void;
  onFileChanged: (callback: (info: { type: string; path: string }) => void) => () => void;
  onGitCommitted: (callback: (info: { files: string[]; summary: string }) => void) => () => void;
}

const api: TeddyAPI = {
  // Dialog
  openFolderDialog: () => ipcRenderer.invoke('dialog:openFolder'),

  // Project
  setProject: (path: string) => ipcRenderer.invoke('project:set', path),
  getProject: () => ipcRenderer.invoke('project:get'),

  // Ted
  runTed: (prompt: string, options) => ipcRenderer.invoke('ted:run', prompt, options),
  stopTed: () => ipcRenderer.invoke('ted:stop'),
  clearHistory: () => ipcRenderer.invoke('ted:clearHistory'),
  getHistoryLength: () => ipcRenderer.invoke('ted:getHistoryLength'),

  // File operations
  readFile: (path: string) => ipcRenderer.invoke('file:read', path),
  writeFile: (path: string, content: string) => ipcRenderer.invoke('file:write', path, content),
  listFiles: (dirPath?: string) => ipcRenderer.invoke('file:list', dirPath),

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
