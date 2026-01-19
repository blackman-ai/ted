// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Cloudflare Tunnel integration for instant preview sharing
 *
 * Uses cloudflared to create temporary public URLs for local dev servers
 */

import { spawn, ChildProcess } from 'child_process';
import { platform } from 'os';
import * as bundled from '../bundled/manager';

export interface TunnelOptions {
  port: number;
  subdomain?: string;
}

export interface TunnelResult {
  success: boolean;
  url?: string;
  error?: string;
  tunnelId?: string;
}

interface TunnelState {
  process: ChildProcess | null;
  url: string | null;
  port: number;
}

// Global tunnel state to track active tunnels
const activeTunnels = new Map<number, TunnelState>();

/**
 * Check if cloudflared is installed
 */
export function isCloudflaredInstalled(): boolean {
  return bundled.isCloudflaredInstalled();
}

/**
 * Get the path to cloudflared binary
 */
function getCloudflaredPath(): string {
  const path = bundled.getCloudflaredPath();
  return path || 'cloudflared'; // Fallback to PATH
}

/**
 * Start a Cloudflare Tunnel for a local port
 */
export async function startTunnel(options: TunnelOptions): Promise<TunnelResult> {
  const { port, subdomain } = options;

  // Check if tunnel already exists for this port
  if (activeTunnels.has(port)) {
    const existing = activeTunnels.get(port)!;
    if (existing.url) {
      return {
        success: true,
        url: existing.url,
        tunnelId: `tunnel-${port}`,
      };
    }
  }

  // Check if cloudflared is installed
  if (!isCloudflaredInstalled()) {
    return {
      success: false,
      error: 'cloudflared is not installed. Install it from https://developers.cloudflare.com/cloudflare-one/connections/connect-apps/install-and-setup/installation/',
    };
  }

  return new Promise((resolve) => {
    const cloudflaredPath = getCloudflaredPath();
    const args = ['tunnel', '--url', `http://localhost:${port}`];

    // Add subdomain if specified (requires Cloudflare Tunnel setup)
    if (subdomain) {
      args.push('--hostname', `${subdomain}.trycloudflare.com`);
    }

    console.log('[CF Tunnel] Starting cloudflared:', cloudflaredPath, args.join(' '));

    const child = spawn(cloudflaredPath, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let tunnelUrl: string | null = null;
    let resolved = false;

    // Parse stdout for tunnel URL
    child.stdout?.on('data', (data: Buffer) => {
      const output = data.toString();
      console.log('[CF Tunnel] stdout:', output);

      // Look for the tunnel URL in output
      // Format: "Your quick Tunnel has been created! Visit it at: https://..."
      const urlMatch = output.match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/);
      if (urlMatch && !tunnelUrl) {
        tunnelUrl = urlMatch[0];
        console.log('[CF Tunnel] Found URL:', tunnelUrl);

        // Store tunnel state
        activeTunnels.set(port, {
          process: child,
          url: tunnelUrl,
          port,
        });

        if (!resolved) {
          resolved = true;
          resolve({
            success: true,
            url: tunnelUrl,
            tunnelId: `tunnel-${port}`,
          });
        }
      }
    });

    child.stderr?.on('data', (data: Buffer) => {
      const output = data.toString();
      console.log('[CF Tunnel] stderr:', output);

      // Also check stderr for URL (cloudflared outputs there too)
      const urlMatch = output.match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/);
      if (urlMatch && !tunnelUrl) {
        tunnelUrl = urlMatch[0];
        console.log('[CF Tunnel] Found URL in stderr:', tunnelUrl);

        activeTunnels.set(port, {
          process: child,
          url: tunnelUrl,
          port,
        });

        if (!resolved) {
          resolved = true;
          resolve({
            success: true,
            url: tunnelUrl,
            tunnelId: `tunnel-${port}`,
          });
        }
      }
    });

    child.on('error', (err) => {
      console.error('[CF Tunnel] Process error:', err);
      if (!resolved) {
        resolved = true;
        resolve({
          success: false,
          error: `Failed to start cloudflared: ${err.message}`,
        });
      }
    });

    child.on('exit', (code, signal) => {
      console.log('[CF Tunnel] Process exited:', code, signal);
      activeTunnels.delete(port);

      if (!resolved) {
        resolved = true;
        resolve({
          success: false,
          error: `cloudflared exited with code ${code}`,
        });
      }
    });

    // Timeout after 10 seconds if no URL found
    setTimeout(() => {
      if (!resolved) {
        resolved = true;
        child.kill();
        resolve({
          success: false,
          error: 'Timeout: Failed to get tunnel URL after 10 seconds',
        });
      }
    }, 10000);
  });
}

/**
 * Stop a running tunnel
 */
export async function stopTunnel(port: number): Promise<{ success: boolean }> {
  const tunnel = activeTunnels.get(port);

  if (!tunnel || !tunnel.process) {
    return { success: false };
  }

  return new Promise((resolve) => {
    const child = tunnel.process!;

    child.on('exit', () => {
      activeTunnels.delete(port);
      resolve({ success: true });
    });

    // Try graceful shutdown first
    child.kill('SIGTERM');

    // Force kill after 2 seconds
    setTimeout(() => {
      if (activeTunnels.has(port)) {
        child.kill('SIGKILL');
        activeTunnels.delete(port);
        resolve({ success: true });
      }
    }, 2000);
  });
}

/**
 * Get active tunnel URL for a port
 */
export function getTunnelUrl(port: number): string | null {
  const tunnel = activeTunnels.get(port);
  return tunnel?.url || null;
}

/**
 * Get all active tunnels
 */
export function getActiveTunnels(): Array<{ port: number; url: string }> {
  const tunnels: Array<{ port: number; url: string }> = [];

  activeTunnels.forEach((state, port) => {
    if (state.url) {
      tunnels.push({ port, url: state.url });
    }
  });

  return tunnels;
}

/**
 * Stop all active tunnels
 */
export async function stopAllTunnels(): Promise<void> {
  const ports = Array.from(activeTunnels.keys());

  await Promise.all(
    ports.map(port => stopTunnel(port))
  );
}

/**
 * Get installation instructions for cloudflared
 */
export function getInstallInstructions(): string {
  return bundled.getInstallInstructions('cloudflared');
}

/**
 * Auto-download and install cloudflared
 */
export async function autoInstallCloudflared(): Promise<{ success: boolean; path?: string; error?: string }> {
  try {
    const path = await bundled.downloadCloudflared();
    return { success: true, path };
  } catch (err: any) {
    return { success: false, error: err.message };
  }
}
