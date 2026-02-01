// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { exec } from 'child_process';
import { promisify } from 'util';
import path from 'path';
import os from 'os';
import fs from 'fs/promises';
import { existsSync, readFileSync, writeFileSync } from 'fs';

const execAsync = promisify(exec);

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
  // Blackman AI - optimized routing with cost savings
  blackmanApiKey: string;
  blackmanBaseUrl: string;
  blackmanModel: string;
  vercelToken: string;
  netlifyToken: string;
  hardware: HardwareInfo | null;
  /** User's experience level - affects verbosity and explanations */
  experienceLevel: 'beginner' | 'intermediate' | 'advanced';
}

/** Default Blackman API URLs for different environments */
export const BLACKMAN_URLS = {
  production: 'https://app.useblackman.ai',
  staging: 'https://staging.useblackman.ai',
  development: 'http://localhost:8080',
} as const;

/**
 * Get Ted's settings directory
 */
function getTedSettingsPath(): string {
  return path.join(os.homedir(), '.ted', 'settings.json');
}

/**
 * Load settings from Ted's settings file
 */
export async function loadTedSettings(): Promise<TedSettings> {
  const settingsPath = getTedSettingsPath();

  const defaultSettings: TedSettings = {
    provider: 'anthropic',
    model: 'claude-sonnet-4-20250514',
    anthropicApiKey: process.env.ANTHROPIC_API_KEY || '',
    anthropicModel: 'claude-sonnet-4-20250514',
    ollamaBaseUrl: 'http://localhost:11434',
    ollamaModel: 'qwen2.5-coder:7b',
    openrouterApiKey: process.env.OPENROUTER_API_KEY || '',
    openrouterModel: 'anthropic/claude-3.5-sonnet',
    blackmanApiKey: process.env.BLACKMAN_API_KEY || '',
    blackmanBaseUrl: process.env.BLACKMAN_BASE_URL || BLACKMAN_URLS.production,
    blackmanModel: 'claude-sonnet-4-20250514',
    vercelToken: process.env.VERCEL_TOKEN || '',
    netlifyToken: process.env.NETLIFY_AUTH_TOKEN || '',
    hardware: null,
    experienceLevel: 'beginner',
  };

  if (!existsSync(settingsPath)) {
    return defaultSettings;
  }

  try {
    const content = readFileSync(settingsPath, 'utf-8');
    const rawSettings = JSON.parse(content);

    // Extract relevant settings from Ted's format
    return {
      provider: rawSettings.defaults?.provider || defaultSettings.provider,
      model: rawSettings.defaults?.model || defaultSettings.model,
      anthropicApiKey: rawSettings.providers?.anthropic?.api_key || defaultSettings.anthropicApiKey,
      anthropicModel: rawSettings.providers?.anthropic?.default_model || defaultSettings.anthropicModel,
      ollamaBaseUrl: rawSettings.providers?.ollama?.base_url || defaultSettings.ollamaBaseUrl,
      ollamaModel: rawSettings.providers?.ollama?.default_model || defaultSettings.ollamaModel,
      openrouterApiKey: rawSettings.providers?.openrouter?.api_key || defaultSettings.openrouterApiKey,
      openrouterModel: rawSettings.providers?.openrouter?.default_model || defaultSettings.openrouterModel,
      blackmanApiKey: rawSettings.providers?.blackman?.api_key || defaultSettings.blackmanApiKey,
      blackmanBaseUrl: rawSettings.providers?.blackman?.base_url || defaultSettings.blackmanBaseUrl,
      blackmanModel: rawSettings.providers?.blackman?.default_model || defaultSettings.blackmanModel,
      vercelToken: rawSettings.deploy?.vercel_token || defaultSettings.vercelToken,
      netlifyToken: rawSettings.deploy?.netlify_token || defaultSettings.netlifyToken,
      hardware: rawSettings.hardware || null,
      experienceLevel: rawSettings.teddy?.experience_level || defaultSettings.experienceLevel,
    };
  } catch (err) {
    console.error('[TED-SETTINGS] Failed to load settings:', err);
    return defaultSettings;
  }
}

/**
 * Save settings to Ted's settings file
 */
export async function saveTedSettings(settings: TedSettings): Promise<void> {
  const settingsPath = getTedSettingsPath();
  const settingsDir = path.dirname(settingsPath);

  // Ensure directory exists
  if (!existsSync(settingsDir)) {
    await fs.mkdir(settingsDir, { recursive: true });
  }

  // Load existing settings or create new
  let rawSettings: any = {};
  if (existsSync(settingsPath)) {
    try {
      const content = readFileSync(settingsPath, 'utf-8');
      rawSettings = JSON.parse(content);
    } catch (err) {
      console.error('[TED-SETTINGS] Failed to parse existing settings:', err);
    }
  }

  // Update settings in Ted's format
  rawSettings.defaults = rawSettings.defaults || {};
  rawSettings.defaults.provider = settings.provider;

  // Sync model with the correct provider-specific model
  const modelForProvider = (() => {
    switch (settings.provider) {
      case 'ollama':
        return settings.ollamaModel;
      case 'openrouter':
        return settings.openrouterModel;
      case 'blackman':
        return settings.blackmanModel;
      case 'anthropic':
      default:
        return settings.anthropicModel;
    }
  })();
  rawSettings.defaults.model = modelForProvider;

  rawSettings.providers = rawSettings.providers || {};

  rawSettings.providers.anthropic = rawSettings.providers.anthropic || {};
  if (settings.anthropicApiKey) {
    rawSettings.providers.anthropic.api_key = settings.anthropicApiKey;
  }
  rawSettings.providers.anthropic.default_model = settings.anthropicModel;

  rawSettings.providers.ollama = rawSettings.providers.ollama || {};
  rawSettings.providers.ollama.base_url = settings.ollamaBaseUrl;
  rawSettings.providers.ollama.default_model = settings.ollamaModel;

  rawSettings.providers.openrouter = rawSettings.providers.openrouter || {};
  if (settings.openrouterApiKey) {
    rawSettings.providers.openrouter.api_key = settings.openrouterApiKey;
  }
  rawSettings.providers.openrouter.default_model = settings.openrouterModel;

  rawSettings.providers.blackman = rawSettings.providers.blackman || {};
  if (settings.blackmanApiKey) {
    rawSettings.providers.blackman.api_key = settings.blackmanApiKey;
  }
  rawSettings.providers.blackman.base_url = settings.blackmanBaseUrl;
  rawSettings.providers.blackman.default_model = settings.blackmanModel;

  if (settings.hardware) {
    rawSettings.hardware = settings.hardware;
  }

  // Save deployment tokens
  rawSettings.deploy = rawSettings.deploy || {};
  if (settings.vercelToken) {
    rawSettings.deploy.vercel_token = settings.vercelToken;
  }
  if (settings.netlifyToken) {
    rawSettings.deploy.netlify_token = settings.netlifyToken;
  }

  // Save Teddy-specific settings
  rawSettings.teddy = rawSettings.teddy || {};
  rawSettings.teddy.experience_level = settings.experienceLevel;

  // Write settings
  writeFileSync(settingsPath, JSON.stringify(rawSettings, null, 2), 'utf-8');
}

/**
 * Detect hardware using Ted's system command
 */
export async function detectHardware(): Promise<HardwareInfo> {
  try {
    // Find Ted binary
    const tedPath = await findTedBinary();
    if (!tedPath) {
      throw new Error('Ted binary not found');
    }

    // Run ted system command with JSON output
    const { stdout } = await execAsync(`"${tedPath}" system --format json`);
    const data = JSON.parse(stdout);
    return data;
  } catch (err) {
    console.error('[TED-SETTINGS] Failed to detect hardware:', err);

    // Return fallback hardware info
    return {
      tier: 'Medium',
      tierDescription: 'Modern Laptop / Desktop',
      cpuBrand: 'Unknown CPU',
      cpuCores: 4,
      ramGb: 16,
      hasSsd: true,
      architecture: 'X86_64',
      isSbc: false,
      cpuYear: null,
      recommendedModels: ['qwen2.5-coder:7b', 'deepseek-coder:6.7b'],
      expectedResponseTime: [5, 10],
      capabilities: ['Full-stack apps', 'REST APIs', 'Database-backed apps'],
      limitations: ['Massive codebases'],
    };
  }
}

/**
 * Find Ted binary in PATH or common locations
 */
async function findTedBinary(): Promise<string | null> {
  try {
    const { stdout } = await execAsync('which ted');
    return stdout.trim();
  } catch {
    // Try common installation locations
    const locations = [
      '/usr/local/bin/ted',
      path.join(os.homedir(), '.local/bin/ted'),
      path.join(os.homedir(), '.cargo/bin/ted'),
    ];

    for (const loc of locations) {
      if (existsSync(loc)) {
        return loc;
      }
    }

    return null;
  }
}

/**
 * Parse text output from ted system command
 */
function parseSystemOutput(output: string): HardwareInfo {
  const lines = output.split('\n');

  let tier = 'Medium';
  let tierDescription = 'Modern Laptop / Desktop';
  let cpuBrand = 'Unknown CPU';
  let cpuCores = 4;
  let ramGb = 16;
  let hasSsd = true;

  for (const line of lines) {
    if (line.includes('Tier:')) {
      const match = line.match(/Tier:\s*(\w+)\s*\(([^)]+)\)/);
      if (match) {
        tier = match[1];
        tierDescription = match[2];
      }
    } else if (line.includes('CPU:')) {
      const match = line.match(/CPU:\s*([^(]+)\((\d+)\s*cores\)/);
      if (match) {
        cpuBrand = match[1].trim();
        cpuCores = parseInt(match[2]);
      }
    } else if (line.includes('RAM:')) {
      const match = line.match(/RAM:\s*(\d+)GB/);
      if (match) {
        ramGb = parseInt(match[1]);
      }
    } else if (line.includes('Storage:')) {
      hasSsd = line.includes('SSD');
    }
  }

  return {
    tier,
    tierDescription,
    cpuBrand,
    cpuCores,
    ramGb,
    hasSsd,
    architecture: 'X86_64',
    isSbc: false,
    cpuYear: null,
    recommendedModels: ['qwen2.5-coder:7b', 'deepseek-coder:6.7b'],
    expectedResponseTime: [5, 10],
    capabilities: ['Full-stack apps', 'REST APIs', 'Database-backed apps'],
    limitations: ['Massive codebases'],
  };
}
