// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import path from 'path';
import os from 'os';
import fs from 'fs/promises';
import { existsSync, mkdirSync, writeFileSync, readFileSync } from 'fs';

/**
 * Teddy configuration and persistence layer
 *
 * Storage structure:
 * ~/.teddy/
 * ├── config.json (app config + recent projects)
 * └── projects/
 *     └── {projectId}/
 *         ├── history.json (conversation history)
 *         └── context.json (project context cache)
 */

export interface RecentProject {
  path: string;
  name: string;
  lastOpened: number;
}

export interface AppConfig {
  lastProjectPath: string | null;
  recentProjects: RecentProject[];
  reviewModeEnabled: boolean;
  previewAutoStart: boolean;
  previewPort: number;
}

export interface ProjectHistory {
  messages: Array<{
    role: 'user' | 'assistant';
    content: string;
    timestamp: number;
  }>;
  lastUpdated: number;
}

export interface ProjectContext {
  fileTree: string[];
  readme: string | null;
  packageJson: any | null;
  lastScanned: number;
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

const DEFAULT_CONFIG: AppConfig = {
  lastProjectPath: null,
  recentProjects: [],
  reviewModeEnabled: true,
  previewAutoStart: true,
  previewPort: 8081,  // Avoid conflict with Blackman backend on 8080
};

class TeddyStorage {
  private configDir: string;
  private configPath: string;
  private projectsDir: string;
  private config: AppConfig | null = null;

  private sessionsDir: string;

  constructor() {
    this.configDir = path.join(os.homedir(), '.teddy');
    this.configPath = path.join(this.configDir, 'config.json');
    this.projectsDir = path.join(this.configDir, 'projects');
    this.sessionsDir = path.join(this.configDir, 'sessions');
  }

  /**
   * Initialize storage directories
   */
  async init(): Promise<void> {
    // Create directories synchronously on first run
    if (!existsSync(this.configDir)) {
      mkdirSync(this.configDir, { recursive: true });
    }
    if (!existsSync(this.projectsDir)) {
      mkdirSync(this.projectsDir, { recursive: true });
    }
    if (!existsSync(this.sessionsDir)) {
      mkdirSync(this.sessionsDir, { recursive: true });
    }

    // Load or create config
    await this.loadConfig();
  }

  /**
   * Load app config from disk
   */
  private async loadConfig(): Promise<AppConfig> {
    if (this.config) {
      return this.config;
    }

    try {
      if (existsSync(this.configPath)) {
        const data = readFileSync(this.configPath, 'utf-8');
        this.config = { ...DEFAULT_CONFIG, ...JSON.parse(data) };
      } else {
        this.config = { ...DEFAULT_CONFIG };
        await this.saveConfig();
      }
    } catch (err) {
      console.error('[STORAGE] Failed to load config:', err);
      this.config = { ...DEFAULT_CONFIG };
    }

    // At this point config is guaranteed to be set
    return this.config!;
  }

  /**
   * Save app config to disk
   */
  private async saveConfig(): Promise<void> {
    if (!this.config) return;

    try {
      writeFileSync(this.configPath, JSON.stringify(this.config, null, 2), 'utf-8');
    } catch (err) {
      console.error('[STORAGE] Failed to save config:', err);
    }
  }

  /**
   * Get app config
   */
  async getConfig(): Promise<AppConfig> {
    return this.loadConfig();
  }

  /**
   * Update app config
   */
  async updateConfig(updates: Partial<AppConfig>): Promise<void> {
    await this.loadConfig();
    this.config = { ...this.config!, ...updates };
    await this.saveConfig();
  }

  /**
   * Add a project to recent projects and set as last opened
   */
  async addRecentProject(projectPath: string): Promise<void> {
    await this.loadConfig();

    const name = path.basename(projectPath);
    const now = Date.now();

    // Remove if already exists
    this.config!.recentProjects = this.config!.recentProjects.filter(
      p => p.path !== projectPath
    );

    // Add to front
    this.config!.recentProjects.unshift({
      path: projectPath,
      name,
      lastOpened: now,
    });

    // Keep only last 10
    this.config!.recentProjects = this.config!.recentProjects.slice(0, 10);

    // Update last project
    this.config!.lastProjectPath = projectPath;

    await this.saveConfig();
  }

  /**
   * Remove a project from recent projects
   */
  async removeRecentProject(projectPath: string): Promise<void> {
    await this.loadConfig();

    this.config!.recentProjects = this.config!.recentProjects.filter(
      p => p.path !== projectPath
    );

    if (this.config!.lastProjectPath === projectPath) {
      this.config!.lastProjectPath = this.config!.recentProjects[0]?.path || null;
    }

    await this.saveConfig();
  }

  /**
   * Get recent projects list
   */
  async getRecentProjects(): Promise<RecentProject[]> {
    const config = await this.loadConfig();
    return config.recentProjects;
  }

  /**
   * Get last opened project path
   */
  async getLastProject(): Promise<string | null> {
    const config = await this.loadConfig();

    // Verify path still exists
    if (config.lastProjectPath && existsSync(config.lastProjectPath)) {
      return config.lastProjectPath;
    }

    return null;
  }

  /**
   * Clear last opened project (used when user explicitly wants to change projects)
   */
  async clearLastProject(): Promise<void> {
    await this.loadConfig();
    this.config!.lastProjectPath = null;
    await this.saveConfig();
  }

  /**
   * Generate a project ID from path (used for storage directory)
   */
  private getProjectId(projectPath: string): string {
    // Use base64 encoding of path for uniqueness
    return Buffer.from(projectPath).toString('base64').replace(/[/+=]/g, '_');
  }

  /**
   * Get project storage directory
   */
  private getProjectDir(projectPath: string): string {
    const projectId = this.getProjectId(projectPath);
    return path.join(this.projectsDir, projectId);
  }

  /**
   * Save conversation history for a project
   */
  async saveProjectHistory(
    projectPath: string,
    messages: Array<{ role: 'user' | 'assistant'; content: string }>
  ): Promise<void> {
    const projectDir = this.getProjectDir(projectPath);

    if (!existsSync(projectDir)) {
      mkdirSync(projectDir, { recursive: true });
    }

    const historyPath = path.join(projectDir, 'history.json');
    const history: ProjectHistory = {
      messages: messages.map(m => ({
        ...m,
        timestamp: Date.now(),
      })),
      lastUpdated: Date.now(),
    };

    try {
      writeFileSync(historyPath, JSON.stringify(history, null, 2), 'utf-8');
    } catch (err) {
      console.error('[STORAGE] Failed to save history:', err);
    }
  }

  /**
   * Load conversation history for a project
   */
  async loadProjectHistory(
    projectPath: string
  ): Promise<Array<{ role: 'user' | 'assistant'; content: string }> | null> {
    const projectDir = this.getProjectDir(projectPath);
    const historyPath = path.join(projectDir, 'history.json');

    try {
      if (existsSync(historyPath)) {
        const data = readFileSync(historyPath, 'utf-8');
        const history: ProjectHistory = JSON.parse(data);
        return history.messages.map(m => ({
          role: m.role,
          content: m.content,
        }));
      }
    } catch (err) {
      console.error('[STORAGE] Failed to load history:', err);
    }

    return null;
  }

  /**
   * Clear conversation history for a project
   */
  async clearProjectHistory(projectPath: string): Promise<void> {
    const projectDir = this.getProjectDir(projectPath);
    const historyPath = path.join(projectDir, 'history.json');

    try {
      if (existsSync(historyPath)) {
        await fs.unlink(historyPath);
      }
    } catch (err) {
      console.error('[STORAGE] Failed to clear history:', err);
    }
  }

  /**
   * Save project context (file tree, readme, etc.)
   */
  async saveProjectContext(projectPath: string, context: ProjectContext): Promise<void> {
    const projectDir = this.getProjectDir(projectPath);

    if (!existsSync(projectDir)) {
      mkdirSync(projectDir, { recursive: true });
    }

    const contextPath = path.join(projectDir, 'context.json');

    try {
      writeFileSync(contextPath, JSON.stringify(context, null, 2), 'utf-8');
    } catch (err) {
      console.error('[STORAGE] Failed to save context:', err);
    }
  }

  /**
   * Load project context
   */
  async loadProjectContext(projectPath: string): Promise<ProjectContext | null> {
    const projectDir = this.getProjectDir(projectPath);
    const contextPath = path.join(projectDir, 'context.json');

    try {
      if (existsSync(contextPath)) {
        const data = readFileSync(contextPath, 'utf-8');
        return JSON.parse(data);
      }
    } catch (err) {
      console.error('[STORAGE] Failed to load context:', err);
    }

    return null;
  }

  /**
   * Session management
   */

  /**
   * Create a new session
   */
  async createSession(sessionId: string, projectPath: string): Promise<SessionInfo> {
    const session: SessionInfo = {
      id: sessionId,
      projectPath,
      name: `Session ${new Date().toLocaleString()}`,
      lastActive: Date.now(),
      created: Date.now(),
      messageCount: 0,
    };

    const sessionPath = path.join(this.sessionsDir, `${sessionId}.json`);
    writeFileSync(sessionPath, JSON.stringify(session, null, 2), 'utf-8');

    return session;
  }

  /**
   * Get a session by ID
   */
  async getSession(sessionId: string): Promise<SessionInfo | null> {
    const sessionPath = path.join(this.sessionsDir, `${sessionId}.json`);

    try {
      if (existsSync(sessionPath)) {
        const data = readFileSync(sessionPath, 'utf-8');
        return JSON.parse(data);
      }
    } catch (err) {
      console.error('[STORAGE] Failed to load session:', err);
    }

    return null;
  }

  /**
   * Get all sessions for a project
   */
  async getProjectSessions(projectPath: string): Promise<SessionInfo[]> {
    const sessions: SessionInfo[] = [];

    try {
      const files = await fs.readdir(this.sessionsDir);

      for (const file of files) {
        if (file.endsWith('.json')) {
          const sessionPath = path.join(this.sessionsDir, file);
          const data = readFileSync(sessionPath, 'utf-8');
          const session: SessionInfo = JSON.parse(data);

          if (session.projectPath === projectPath) {
            sessions.push(session);
          }
        }
      }

      // Sort by last active (most recent first)
      sessions.sort((a, b) => b.lastActive - a.lastActive);
    } catch (err) {
      console.error('[STORAGE] Failed to load project sessions:', err);
    }

    return sessions;
  }

  /**
   * Update session metadata
   */
  async updateSession(sessionId: string, updates: Partial<SessionInfo>): Promise<void> {
    const session = await this.getSession(sessionId);
    if (!session) return;

    const updated = { ...session, ...updates, lastActive: Date.now() };
    const sessionPath = path.join(this.sessionsDir, `${sessionId}.json`);
    writeFileSync(sessionPath, JSON.stringify(updated, null, 2), 'utf-8');
  }

  /**
   * Delete a session
   */
  async deleteSession(sessionId: string): Promise<void> {
    const sessionPath = path.join(this.sessionsDir, `${sessionId}.json`);
    const historyPath = path.join(this.sessionsDir, `${sessionId}-history.json`);

    try {
      if (existsSync(sessionPath)) {
        await fs.unlink(sessionPath);
      }
      if (existsSync(historyPath)) {
        await fs.unlink(historyPath);
      }
    } catch (err) {
      console.error('[STORAGE] Failed to delete session:', err);
    }
  }

  /**
   * Save conversation history for a session
   */
  async saveSessionHistory(
    sessionId: string,
    messages: Array<{ role: 'user' | 'assistant'; content: string }>
  ): Promise<void> {
    const historyPath = path.join(this.sessionsDir, `${sessionId}-history.json`);

    try {
      writeFileSync(historyPath, JSON.stringify(messages, null, 2), 'utf-8');

      // Update session message count
      await this.updateSession(sessionId, { messageCount: messages.length });
    } catch (err) {
      console.error('[STORAGE] Failed to save session history:', err);
    }
  }

  /**
   * Load conversation history for a session
   */
  async loadSessionHistory(
    sessionId: string
  ): Promise<Array<{ role: 'user' | 'assistant'; content: string }> | null> {
    const historyPath = path.join(this.sessionsDir, `${sessionId}-history.json`);

    try {
      if (existsSync(historyPath)) {
        const data = readFileSync(historyPath, 'utf-8');
        return JSON.parse(data);
      }
    } catch (err) {
      console.error('[STORAGE] Failed to load session history:', err);
    }

    return null;
  }
}

// Singleton instance
export const storage = new TeddyStorage();
