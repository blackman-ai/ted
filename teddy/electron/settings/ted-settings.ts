// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { exec } from 'child_process';
import { promisify } from 'util';
import path from 'path';
import os from 'os';
import fs from 'fs/promises';
import { createWriteStream, existsSync, readFileSync, unlinkSync, writeFileSync } from 'fs';
import https from 'https';

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
  localPort: number;
  localModel: string;
  localBaseUrl?: string;
  localModelPath?: string;
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

interface TedSettingsFile {
  defaults?: {
    provider?: string;
    model?: string;
  };
  providers?: {
    anthropic?: {
      api_key?: string;
      default_model?: string;
    };
    local?: {
      port?: number;
      base_url?: string;
      default_model?: string;
      model_path?: string;
    };
    // Legacy provider settings for backward compatibility with older Teddy versions.
    ollama?: {
      base_url?: string;
      default_model?: string;
    };
    openrouter?: {
      api_key?: string;
      default_model?: string;
    };
    blackman?: {
      api_key?: string;
      base_url?: string;
      default_model?: string;
    };
  };
  deploy?: {
    vercel_token?: string;
    netlify_token?: string;
  };
  hardware?: HardwareInfo | null;
  teddy?: {
    experience_level?: TedSettings['experienceLevel'];
  };
}

/** Default Blackman API URLs for different environments */
export const BLACKMAN_URLS = {
  production: 'https://app.useblackman.ai',
  staging: 'https://staging.useblackman.ai',
  development: 'http://localhost:8080',
} as const;

const VALID_PROVIDERS = new Set(['anthropic', 'local', 'openrouter', 'blackman']);

function normalizeProviderName(provider: string): string {
  // Legacy alias kept for older Teddy configs.
  if (provider === 'ollama') {
    return 'local';
  }
  return VALID_PROVIDERS.has(provider) ? provider : 'anthropic';
}

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
    localPort: 8847,
    localModel: 'qwen2.5-coder:3b',
    localBaseUrl: '',
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
    const rawSettings = JSON.parse(content) as TedSettingsFile;
    const normalizedProvider = normalizeProviderName(
      rawSettings.defaults?.provider || defaultSettings.provider
    );

    // Extract relevant settings from Ted's format
    const hardware = rawSettings.hardware || null;
    const normalizedLocalModel = normalizeToKnownInstructLocalModel(
      rawSettings.providers?.local?.default_model ||
      rawSettings.providers?.ollama?.default_model ||
      defaultSettings.localModel,
      hardware
    );

    return {
      provider: normalizedProvider,
      model: rawSettings.defaults?.model || defaultSettings.model,
      anthropicApiKey: rawSettings.providers?.anthropic?.api_key || defaultSettings.anthropicApiKey,
      anthropicModel: rawSettings.providers?.anthropic?.default_model || defaultSettings.anthropicModel,
      localPort: rawSettings.providers?.local?.port || defaultSettings.localPort,
      localModel: normalizedLocalModel,
      localBaseUrl: normalizeBaseUrl(rawSettings.providers?.local?.base_url || ''),
      localModelPath: rawSettings.providers?.local?.model_path,
      openrouterApiKey: rawSettings.providers?.openrouter?.api_key || defaultSettings.openrouterApiKey,
      openrouterModel: rawSettings.providers?.openrouter?.default_model || defaultSettings.openrouterModel,
      blackmanApiKey: rawSettings.providers?.blackman?.api_key || defaultSettings.blackmanApiKey,
      blackmanBaseUrl: rawSettings.providers?.blackman?.base_url || defaultSettings.blackmanBaseUrl,
      blackmanModel: rawSettings.providers?.blackman?.default_model || defaultSettings.blackmanModel,
      vercelToken: rawSettings.deploy?.vercel_token || defaultSettings.vercelToken,
      netlifyToken: rawSettings.deploy?.netlify_token || defaultSettings.netlifyToken,
      hardware,
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
  let rawSettings: TedSettingsFile = {};
  if (existsSync(settingsPath)) {
    try {
      const content = readFileSync(settingsPath, 'utf-8');
      rawSettings = JSON.parse(content) as TedSettingsFile;
    } catch (err) {
      console.error('[TED-SETTINGS] Failed to parse existing settings:', err);
    }
  }

  const normalizedLocalModel = normalizeToKnownInstructLocalModel(
    settings.localModel,
    settings.hardware
  );
  const normalizedLocalBaseUrl = normalizeBaseUrl(settings.localBaseUrl || '');
  const normalizedLocalModelPath = (() => {
    if (normalizedLocalBaseUrl) {
      return settings.localModelPath || '';
    }

    const preset = selectPresetForModel(normalizedLocalModel, settings.hardware);
    if (!preset) {
      return settings.localModelPath || '';
    }

    return path.join(os.homedir(), '.ted', 'models', 'local', preset.fileName);
  })();

  // Update settings in Ted's format
  rawSettings.defaults = rawSettings.defaults || {};
  const normalizedProvider = normalizeProviderName(settings.provider);
  rawSettings.defaults.provider = normalizedProvider;

  // Sync model with the correct provider-specific model
  const modelForProvider = (() => {
    switch (normalizedProvider) {
      case 'local':
        return normalizedLocalModel;
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

  rawSettings.providers.local = rawSettings.providers.local || {};
  rawSettings.providers.local.port = settings.localPort;
  rawSettings.providers.local.default_model = normalizedLocalModel;
  if (normalizedLocalBaseUrl) {
    rawSettings.providers.local.base_url = normalizedLocalBaseUrl;
  } else {
    delete rawSettings.providers.local.base_url;
  }
  if (normalizedLocalModelPath) {
    rawSettings.providers.local.model_path = normalizedLocalModelPath;
  } else {
    delete rawSettings.providers.local.model_path;
  }

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
      recommendedModels: ['qwen2.5-coder:3b', 'qwen2.5-coder:1.5b'],
      expectedResponseTime: [10, 30],
      capabilities: ['Full-stack apps', 'REST APIs', 'Database-backed apps'],
      limitations: ['Massive codebases'],
    };
  }
}

interface LocalModelPreset {
  aliases: string[];
  modelName: string;
  fileName: string;
  url: string;
}

const LOCAL_MODEL_PRESETS: LocalModelPreset[] = [
  {
    aliases: ['qwen2.5-coder:1.5b', 'qwen2.5-coder-1.5b', 'qwen2.5-coder-1.5b-instruct'],
    modelName: 'qwen2.5-coder:1.5b',
    fileName: 'qwen2.5-coder-1.5b-instruct-q4_k_m.gguf',
    url: 'https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF/resolve/main/qwen2.5-coder-1.5b-instruct-q4_k_m.gguf',
  },
  {
    aliases: ['qwen2.5-coder:3b', 'qwen2.5-coder-3b', 'qwen2.5-coder-3b-instruct'],
    modelName: 'qwen2.5-coder:3b',
    fileName: 'qwen2.5-coder-3b-instruct-q4_k_m.gguf',
    url: 'https://huggingface.co/Qwen/Qwen2.5-Coder-3B-Instruct-GGUF/resolve/main/qwen2.5-coder-3b-instruct-q4_k_m.gguf',
  },
  {
    aliases: ['qwen2.5-coder:7b', 'qwen2.5-coder-7b', 'qwen2.5-coder-7b-instruct'],
    modelName: 'qwen2.5-coder:7b',
    fileName: 'qwen2.5-coder-7b-instruct-q4_k_m.gguf',
    url: 'https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/qwen2.5-coder-7b-instruct-q4_k_m.gguf',
  },
];

export const KNOWN_LOCAL_INSTRUCT_MODELS: string[] = LOCAL_MODEL_PRESETS.map(
  (preset) => preset.modelName
);

function normalizeModelName(value: string): string {
  return value.trim().toLowerCase().replace(':', '-');
}

function normalizeBaseUrl(value: string): string {
  return value.trim().replace(/\/+$/, '');
}

function normalizeToKnownInstructLocalModel(
  model: string,
  hardware: HardwareInfo | null
): string {
  const normalized = normalizeModelName(model || '');
  const matched = LOCAL_MODEL_PRESETS.find((preset) =>
    preset.aliases.map(normalizeModelName).includes(normalized)
  );

  if (matched) {
    return matched.modelName;
  }

  if (hardware) {
    return selectPresetForHardware(hardware).modelName;
  }

  return LOCAL_MODEL_PRESETS[1].modelName; // qwen2.5-coder:3b fallback
}

function selectPresetForHardware(hardware: HardwareInfo): LocalModelPreset {
  const normalized = (hardware.recommendedModels || []).map(normalizeModelName);

  for (const model of normalized) {
    const match = LOCAL_MODEL_PRESETS.find((preset) =>
      preset.aliases.map(normalizeModelName).includes(model)
    );
    if (match) {
      return match;
    }
  }

  // Conservative default for unknown systems.
  return hardware.isSbc
    ? LOCAL_MODEL_PRESETS[1] // qwen2.5-coder:3b
    : LOCAL_MODEL_PRESETS[2]; // qwen2.5-coder:7b
}

function selectPresetForModel(model: string, hardware: HardwareInfo | null): LocalModelPreset {
  const normalizedModel = normalizeToKnownInstructLocalModel(model, hardware);
  return (
    LOCAL_MODEL_PRESETS.find((preset) => preset.modelName === normalizedModel) ||
    LOCAL_MODEL_PRESETS[1]
  );
}

function downloadFile(url: string, outputPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const stream = createWriteStream(outputPath);
    https
      .get(url, (response) => {
        if (response.statusCode === 301 || response.statusCode === 302) {
          const redirectUrl = response.headers.location;
          stream.close();
          if (redirectUrl) {
            if (existsSync(outputPath)) {
              unlinkSync(outputPath);
            }
            downloadFile(redirectUrl, outputPath).then(resolve).catch(reject);
            return;
          }
        }

        if (response.statusCode !== 200) {
          stream.close();
          if (existsSync(outputPath)) {
            unlinkSync(outputPath);
          }
          reject(new Error(`Model download failed: HTTP ${response.statusCode}`));
          return;
        }

        response.pipe(stream);
        stream.on('finish', () => {
          stream.close();
          resolve();
        });
      })
      .on('error', (err) => {
        stream.close();
        if (existsSync(outputPath)) {
          unlinkSync(outputPath);
        }
        reject(err);
      });
  });
}

export interface LocalSetupResult {
  success: boolean;
  model?: string;
  modelPath?: string;
  downloaded?: boolean;
  message?: string;
  error?: string;
}

export async function ensureLocalModelInstalled(
  model: string,
  hardware: HardwareInfo | null
): Promise<LocalSetupResult> {
  try {
    const preset = selectPresetForModel(model, hardware);
    const modelDir = path.join(os.homedir(), '.ted', 'models', 'local');
    await fs.mkdir(modelDir, { recursive: true });

    const modelPath = path.join(modelDir, preset.fileName);
    let downloaded = false;

    if (!existsSync(modelPath)) {
      await downloadFile(preset.url, modelPath);
      downloaded = true;
    }

    return {
      success: true,
      model: preset.modelName,
      modelPath,
      downloaded,
      message: downloaded
        ? `${preset.modelName} downloaded and configured.`
        : `${preset.modelName} already installed.`,
    };
  } catch (err) {
    return {
      success: false,
      error: err instanceof Error ? err.message : String(err),
    };
  }
}

/**
 * Configure local AI automatically for end users.
 * Detects hardware, picks a good model, downloads if needed, and updates settings.
 */
export async function setupRecommendedLocalModel(): Promise<LocalSetupResult> {
  try {
    const hardware = await detectHardware();
    const preset = selectPresetForHardware(hardware);
    const installResult = await ensureLocalModelInstalled(preset.modelName, hardware);
    if (!installResult.success || !installResult.model || !installResult.modelPath) {
      return {
        success: false,
        error: installResult.error || 'Failed to install local model',
      };
    }

    const settings = await loadTedSettings();
    const updated: TedSettings = {
      ...settings,
      provider: 'local',
      model: installResult.model,
      localModel: installResult.model,
      localBaseUrl: '',
      localModelPath: installResult.modelPath,
      hardware,
    };
    await saveTedSettings(updated);

    return {
      success: true,
      model: installResult.model,
      modelPath: installResult.modelPath,
      downloaded: installResult.downloaded,
      message: installResult.downloaded
        ? 'Local AI is ready. Model downloaded and configured.'
        : 'Local AI is ready. Existing model configured.',
    };
  } catch (err) {
    return {
      success: false,
      error: err instanceof Error ? err.message : String(err),
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
export function parseSystemOutput(output: string): HardwareInfo {
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
    recommendedModels: ['qwen2.5-coder:3b', 'qwen2.5-coder:1.5b'],
    expectedResponseTime: [10, 30],
    capabilities: ['Full-stack apps', 'REST APIs', 'Database-backed apps'],
    limitations: ['Massive codebases'],
  };
}
