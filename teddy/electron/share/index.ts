// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { spawn, ChildProcess } from 'child_process';
import path from 'path';
import os from 'os';
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'fs';
import { debugLog } from '../utils/logger';

/**
 * Share module for teddy.rocks subdomain service
 *
 * Zero-config sharing - just click "Share" and get a teddy.rocks URL!
 *
 * Flow:
 * 1. Start a cloudflared tunnel to a local port
 * 2. Register a subdomain with teddy.rocks (no API key needed!)
 * 3. Get back a client token for managing the subdomain
 * 4. Store the token locally for future use
 */

export interface ShareOptions {
  port: number;
  projectName?: string;
  customSlug?: string;
}

export interface ShareResult {
  success: boolean;
  slug?: string;
  previewUrl?: string;
  tunnelUrl?: string;
  error?: string;
}

// teddy.rocks API - no secrets required!
const TEDDY_ROCKS_API = 'https://api.teddy.rocks';

// Local storage for client tokens
const TOKEN_STORAGE_DIR = path.join(os.homedir(), '.teddy');
const TOKEN_STORAGE_FILE = path.join(TOKEN_STORAGE_DIR, 'share-tokens.json');

// Track active shares
const activeShares = new Map<number, {
  process: ChildProcess;
  slug: string;
  tunnelUrl: string;
  clientToken: string;
}>();

function getErrorMessage(err: unknown): string {
  if (err instanceof Error) {
    return err.message;
  }
  return String(err);
}

/**
 * Load stored client tokens
 */
function loadTokens(): Record<string, string> {
  try {
    if (existsSync(TOKEN_STORAGE_FILE)) {
      return JSON.parse(readFileSync(TOKEN_STORAGE_FILE, 'utf-8'));
    }
  } catch (err) {
    console.error('[SHARE] Failed to load tokens:', err);
  }
  return {};
}

/**
 * Save client tokens
 */
function saveTokens(tokens: Record<string, string>): void {
  try {
    if (!existsSync(TOKEN_STORAGE_DIR)) {
      mkdirSync(TOKEN_STORAGE_DIR, { recursive: true });
    }
    writeFileSync(TOKEN_STORAGE_FILE, JSON.stringify(tokens, null, 2), 'utf-8');
  } catch (err) {
    console.error('[SHARE] Failed to save tokens:', err);
  }
}

/**
 * Get stored token for a slug
 */
function getStoredToken(slug: string): string | null {
  const tokens = loadTokens();
  return tokens[slug] || null;
}

/**
 * Store token for a slug
 */
function storeToken(slug: string, token: string): void {
  const tokens = loadTokens();
  tokens[slug] = token;
  saveTokens(tokens);
}

/**
 * Remove stored token for a slug
 */
function removeStoredToken(slug: string): void {
  const tokens = loadTokens();
  delete tokens[slug];
  saveTokens(tokens);
}

/**
 * Generate a random slug for the subdomain
 */
export function generateSlug(projectName?: string): string {
  const base = projectName
    ? projectName.toLowerCase().replace(/[^a-z0-9]/g, '-').replace(/-+/g, '-').slice(0, 20)
    : 'app';

  // Add random suffix
  const suffix = Math.random().toString(36).substring(2, 6);

  // Ensure it meets the regex: starts/ends with alphanumeric, 4-32 chars
  const slug = `${base}-${suffix}`.replace(/^-+|-+$/g, '');

  // Ensure minimum length of 4
  if (slug.length < 4) {
    return `app-${suffix}`;
  }

  return slug;
}

/**
 * Check if a slug is available
 */
export async function checkSlugAvailability(slug: string): Promise<boolean> {
  try {
    const response = await fetch(`${TEDDY_ROCKS_API}/api/status/${slug}`);
    const data = await response.json();
    return data.available === true;
  } catch (err) {
    console.error('[SHARE] Failed to check slug availability:', err);
    // If we can't reach the API, assume it's available and let registration fail if not
    return true;
  }
}

/**
 * Get the path to cloudflared binary
 */
function getCloudflaredPath(): string | null {
  const bundledPaths = [
    path.join(__dirname, '../../bin/cloudflared'),
    path.join(__dirname, '../../../bin/cloudflared'),
    path.join(process.resourcesPath || '', 'bin/cloudflared'),
  ];

  for (const p of bundledPaths) {
    if (existsSync(p)) {
      return p;
    }
  }

  // Check auto-downloaded location (~/.teddy/bin/cloudflared)
  const localBinPath = path.join(os.homedir(), '.teddy', 'bin', 'cloudflared');
  if (existsSync(localBinPath)) {
    return localBinPath;
  }

  const systemPaths = [
    '/usr/local/bin/cloudflared',
    '/opt/homebrew/bin/cloudflared',
    path.join(os.homedir(), '.cloudflared/cloudflared'),
  ];

  for (const p of systemPaths) {
    if (existsSync(p)) {
      return p;
    }
  }

  return null;
}

/**
 * Start a cloudflared tunnel and register with teddy.rocks
 */
export async function startShare(options: ShareOptions): Promise<ShareResult> {
  const { port, projectName, customSlug } = options;

  // Check if already sharing this port
  if (activeShares.has(port)) {
    const existing = activeShares.get(port)!;
    return {
      success: true,
      slug: existing.slug,
      previewUrl: `https://${existing.slug}.teddy.rocks`,
      tunnelUrl: existing.tunnelUrl,
    };
  }

  const cloudflaredPath = getCloudflaredPath();
  if (!cloudflaredPath) {
    return {
      success: false,
      error: 'cloudflared not found. Please install it first.',
    };
  }

  // Generate or use custom slug
  let slug = customSlug || generateSlug(projectName);

  // Check if we have a stored token for this slug (allows reusing the same subdomain)
  let existingToken = getStoredToken(slug);

  // If no custom slug and slug is taken, generate a new one
  if (!customSlug) {
    let attempts = 0;
    while (!(await checkSlugAvailability(slug)) && !existingToken && attempts < 5) {
      slug = generateSlug(projectName);
      existingToken = getStoredToken(slug);
      attempts++;
    }

    if (attempts >= 5 && !existingToken) {
      return {
        success: false,
        error: 'Could not find an available subdomain. Please try again.',
      };
    }
  }

  debugLog('[SHARE] Starting tunnel for port', port, 'with slug', slug);

  // Start cloudflared tunnel
  return new Promise((resolve) => {
    const tunnelProcess = spawn(cloudflaredPath, [
      'tunnel',
      '--url', `http://localhost:${port}`,
      '--no-autoupdate',
    ], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let tunnelUrl: string | null = null;
    let resolved = false;

    const resolveOnce = (result: ShareResult) => {
      if (!resolved) {
        resolved = true;
        resolve(result);
      }
    };

    let tunnelConnected = false;

    // Function to register once tunnel is connected
    const doRegister = () => {
      if (!tunnelUrl || !tunnelConnected || resolved) return;

      debugLog('[SHARE] Tunnel connected, registering with teddy.rocks...');

      // Register with teddy.rocks
      registerSubdomain(slug, tunnelUrl, projectName, existingToken)
        .then((result) => {
          if (result.success && result.clientToken) {
            // Store the client token for future use
            storeToken(slug, result.clientToken);

            activeShares.set(port, {
              process: tunnelProcess,
              slug,
              tunnelUrl: tunnelUrl!,
              clientToken: result.clientToken,
            });

            resolveOnce({
              success: true,
              slug,
              previewUrl: `https://${slug}.teddy.rocks`,
              tunnelUrl: tunnelUrl ?? undefined,
            });
          } else {
            tunnelProcess.kill();
            resolveOnce({
              success: false,
              error: result.error || 'Failed to register subdomain',
            });
          }
        })
        .catch((err: unknown) => {
          console.error('[SHARE] Registration error:', err);
          tunnelProcess.kill();
          resolveOnce({
            success: false,
            error: `Registration failed: ${getErrorMessage(err)}`,
          });
        });
    };

    // Parse tunnel URL and connection status from output
    const handleOutput = (data: Buffer) => {
      const text = data.toString();
      debugLog('[SHARE] cloudflared:', text);

      // Capture the tunnel URL when it appears
      const urlMatch = text.match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/);
      if (urlMatch && !tunnelUrl) {
        tunnelUrl = urlMatch[0];
        debugLog('[SHARE] Tunnel URL:', tunnelUrl);
      }

      // Wait for tunnel to be fully registered before calling the API
      if (text.includes('Registered tunnel connection')) {
        tunnelConnected = true;
        debugLog('[SHARE] Tunnel connection registered');
        doRegister();
      }
    };

    tunnelProcess.stdout?.on('data', handleOutput);
    tunnelProcess.stderr?.on('data', handleOutput);

    tunnelProcess.on('error', (err) => {
      console.error('[SHARE] Tunnel process error:', err);
      resolveOnce({
        success: false,
        error: `Tunnel failed to start: ${err.message}`,
      });
    });

    tunnelProcess.on('exit', (code) => {
      debugLog('[SHARE] Tunnel process exited with code', code);
      activeShares.delete(port);

      if (!resolved) {
        resolveOnce({
          success: false,
          error: `Tunnel exited unexpectedly with code ${code}`,
        });
      }
    });

    // Timeout after 30 seconds
    setTimeout(() => {
      if (!resolved) {
        tunnelProcess.kill();
        resolveOnce({
          success: false,
          error: 'Tunnel startup timed out after 30 seconds',
        });
      }
    }, 30000);
  });
}

/**
 * Register a subdomain with teddy.rocks (no API key needed!)
 */
async function registerSubdomain(
  slug: string,
  tunnelUrl: string,
  projectName?: string,
  existingToken?: string | null
): Promise<{ success: boolean; clientToken?: string; error?: string }> {
  try {
    const response = await fetch(`${TEDDY_ROCKS_API}/api/register`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        slug,
        tunnelUrl,
        projectName,
        clientToken: existingToken || undefined,
      }),
    });

    const data = await response.json();

    if (!response.ok) {
      console.error('[SHARE] Registration failed:', data);
      return { success: false, error: data.error || 'Registration failed' };
    }

    debugLog('[SHARE] Registered:', data);
    return {
      success: data.success === true,
      clientToken: data.clientToken,
      error: data.error,
    };
  } catch (err: unknown) {
    console.error('[SHARE] Registration error:', err);
    return { success: false, error: getErrorMessage(err) };
  }
}

/**
 * Unregister a subdomain from teddy.rocks
 */
async function unregisterSubdomain(slug: string, clientToken: string): Promise<boolean> {
  try {
    const response = await fetch(
      `${TEDDY_ROCKS_API}/api/unregister?slug=${encodeURIComponent(slug)}`,
      {
        method: 'DELETE',
        headers: {
          'X-Client-Token': clientToken,
        },
      }
    );

    return response.ok;
  } catch (err) {
    console.error('[SHARE] Unregistration error:', err);
    return false;
  }
}

/**
 * Stop sharing a port
 */
export async function stopShare(port: number): Promise<boolean> {
  const share = activeShares.get(port);
  if (!share) {
    return false;
  }

  debugLog('[SHARE] Stopping share for port', port);

  // Unregister from teddy.rocks
  await unregisterSubdomain(share.slug, share.clientToken);

  // Remove stored token (they can get a new one next time)
  removeStoredToken(share.slug);

  // Kill the tunnel process
  share.process.kill('SIGTERM');

  setTimeout(() => {
    if (!share.process.killed) {
      share.process.kill('SIGKILL');
    }
  }, 5000);

  activeShares.delete(port);
  return true;
}

/**
 * Get active share for a port
 */
export function getActiveShare(port: number): { slug: string; previewUrl: string } | null {
  const share = activeShares.get(port);
  if (!share) {
    return null;
  }

  return {
    slug: share.slug,
    previewUrl: `https://${share.slug}.teddy.rocks`,
  };
}

/**
 * Get all active shares
 */
export function getAllActiveShares(): Array<{ port: number; slug: string; previewUrl: string }> {
  const shares: Array<{ port: number; slug: string; previewUrl: string }> = [];

  for (const [port, share] of activeShares) {
    shares.push({
      port,
      slug: share.slug,
      previewUrl: `https://${share.slug}.teddy.rocks`,
    });
  }

  return shares;
}

/**
 * Stop all active shares (for cleanup on app exit)
 */
export async function stopAllShares(): Promise<void> {
  const ports = Array.from(activeShares.keys());

  for (const port of ports) {
    await stopShare(port);
  }
}
