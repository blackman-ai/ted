// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Bundled Dependencies Manager
 *
 * Manages bundled binaries (currently cloudflared) that ship with Teddy
 * or are auto-downloaded on first use.
 */

import { existsSync } from 'fs';
import { chmod, mkdir, unlink } from 'fs/promises';
import path from 'path';
import os from 'os';
import https from 'https';
import fs from 'fs';
import * as tar from 'tar';
import type { App } from 'electron';
import { app as electronApp } from 'electron';
import { debugLog } from '../utils/logger';

function getApp(): App {
  return electronApp;
}

export interface BundledBinary {
  name: string;
  version: string;
  path: string;
  url?: string;
  installed: boolean;
}

/**
 * Get the bundled resources directory
 */
export function getBundledResourcesDir(): string {
  const app = getApp();
  if (app.isPackaged) {
    // In production, bundled resources are in the app.asar.unpacked/resources
    return path.join(process.resourcesPath, 'bin');
  } else {
    // In development, use a local directory
    return path.join(app.getAppPath(), '..', 'bundled-bin');
  }
}

/**
 * Get the user's local bin directory for auto-downloaded binaries
 */
export function getLocalBinDir(): string {
  const homeDir = os.homedir();
  return path.join(homeDir, '.teddy', 'bin');
}

/**
 * Ensure local bin directory exists
 */
async function ensureLocalBinDir(): Promise<void> {
  const binDir = getLocalBinDir();
  if (!existsSync(binDir)) {
    await mkdir(binDir, { recursive: true });
  }
}

/**
 * Get cloudflared binary path
 */
export function getCloudflaredPath(): string | null {
  const platform = os.platform();
  const binaryName = platform === 'win32' ? 'cloudflared.exe' : 'cloudflared';

  // Check bundled location first
  const bundledPath = path.join(getBundledResourcesDir(), binaryName);
  if (existsSync(bundledPath)) {
    return bundledPath;
  }

  // Check local bin
  const localPath = path.join(getLocalBinDir(), binaryName);
  if (existsSync(localPath)) {
    return localPath;
  }

  // Check system installation
  const systemPaths = getSystemCloudflaredPaths();
  for (const p of systemPaths) {
    if (existsSync(p)) {
      return p;
    }
  }

  return null;
}

/**
 * Get system cloudflared installation paths to check
 */
function getSystemCloudflaredPaths(): string[] {
  const platform = os.platform();

  if (platform === 'darwin' || platform === 'linux') {
    return [
      '/opt/homebrew/bin/cloudflared',
      '/usr/local/bin/cloudflared',
      '/usr/bin/cloudflared',
    ];
  } else if (platform === 'win32') {
    const programFiles = process.env.ProgramFiles || 'C:\\Program Files';
    return [
      `${programFiles}\\cloudflared\\cloudflared.exe`,
      path.join(os.homedir(), 'cloudflared', 'cloudflared.exe'),
    ];
  }

  return [];
}

/**
 * Download cloudflared binary
 */
export async function downloadCloudflared(): Promise<string> {
  await ensureLocalBinDir();

  const platform = os.platform();
  const arch = os.arch();

  let downloadUrl: string;
  let binaryName: string;

  // Determine download URL based on platform/arch
  if (platform === 'darwin') {
    if (arch === 'arm64') {
      downloadUrl = 'https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-darwin-arm64.tgz';
    } else {
      downloadUrl = 'https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-darwin-amd64.tgz';
    }
    binaryName = 'cloudflared';
  } else if (platform === 'linux') {
    if (arch === 'arm64' || arch === 'aarch64') {
      downloadUrl = 'https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-arm64';
    } else {
      downloadUrl = 'https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64';
    }
    binaryName = 'cloudflared';
  } else if (platform === 'win32') {
    if (arch === 'arm64') {
      throw new Error('Windows ARM64 not supported for cloudflared auto-download');
    }
    downloadUrl = 'https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe';
    binaryName = 'cloudflared.exe';
  } else {
    throw new Error(`Unsupported platform: ${platform}`);
  }

  const outputPath = path.join(getLocalBinDir(), binaryName);
  const isTarball = downloadUrl.endsWith('.tgz') || downloadUrl.endsWith('.tar.gz');

  debugLog(`[BUNDLED] Downloading cloudflared from ${downloadUrl}...`);

  if (isTarball) {
    // Download to temp file, extract, then delete
    const tempPath = path.join(getLocalBinDir(), 'cloudflared.tgz');
    await downloadFile(downloadUrl, tempPath);

    debugLog(`[BUNDLED] Extracting cloudflared...`);

    // Extract the tarball using tar.extract
    await tar.extract({
      file: tempPath,
      cwd: getLocalBinDir(),
    });

    // Clean up temp file
    await unlink(tempPath);
  } else {
    // Direct binary download (Linux, Windows)
    await downloadFile(downloadUrl, outputPath);
  }

  // Make executable on Unix
  if (platform !== 'win32') {
    await chmod(outputPath, 0o755);
  }

  debugLog(`[BUNDLED] cloudflared installed to ${outputPath}`);

  return outputPath;
}

/**
 * Check if cloudflared is installed
 */
export function isCloudflaredInstalled(): boolean {
  return getCloudflaredPath() !== null;
}

/**
 * Get installation instructions for manual installation
 */
export function getInstallInstructions(binary: 'cloudflared'): string {
  const platform = os.platform();

  if (binary === 'cloudflared') {
    if (platform === 'darwin') {
      return 'Install cloudflared:\n\nbrew install cloudflared\n\nOr download from:\nhttps://github.com/cloudflare/cloudflared/releases';
    } else if (platform === 'linux') {
      return 'Install cloudflared:\n\nDebian/Ubuntu:\nwget -q https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64\nsudo mv cloudflared-linux-amd64 /usr/local/bin/cloudflared\nsudo chmod +x /usr/local/bin/cloudflared';
    } else if (platform === 'win32') {
      return 'Install cloudflared:\n\nDownload from:\nhttps://github.com/cloudflare/cloudflared/releases\n\nOr use winget:\nwinget install --id Cloudflare.cloudflared';
    }
  }

  return 'Unknown binary';
}

/**
 * Download a file from a URL
 */
function downloadFile(url: string, outputPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(outputPath);

    https.get(url, (response) => {
      // Handle redirects
      if (response.statusCode === 301 || response.statusCode === 302) {
        const redirectUrl = response.headers.location;
        if (redirectUrl) {
          file.close();
          fs.unlinkSync(outputPath);
          return downloadFile(redirectUrl, outputPath).then(resolve).catch(reject);
        }
      }

      if (response.statusCode !== 200) {
        file.close();
        fs.unlinkSync(outputPath);
        return reject(new Error(`Failed to download: HTTP ${response.statusCode}`));
      }

      response.pipe(file);

      file.on('finish', () => {
        file.close();
        resolve();
      });
    }).on('error', (err) => {
      file.close();
      fs.unlinkSync(outputPath);
      reject(err);
    });
  });
}

/**
 * Get all bundled binaries status
 */
export async function getBundledBinariesStatus(): Promise<BundledBinary[]> {
  const cloudflaredPath = getCloudflaredPath();

  return [
    {
      name: 'cloudflared',
      version: 'latest',
      path: cloudflaredPath || 'not installed',
      installed: cloudflaredPath !== null,
    },
  ];
}
