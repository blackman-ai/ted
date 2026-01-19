// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Bundled Dependencies Manager
 *
 * Manages bundled binaries (cloudflared, ollama) that ship with Teddy
 * or are auto-downloaded on first use.
 */

import { app } from 'electron';
import { existsSync, chmodSync, mkdirSync } from 'fs';
import { chmod, mkdir, writeFile, unlink } from 'fs/promises';
import path from 'path';
import os from 'os';
import { exec } from 'child_process';
import { promisify } from 'util';
import https from 'https';
import fs from 'fs';
import { createGunzip } from 'zlib';
import * as tar from 'tar';

const execAsync = promisify(exec);

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

  console.log(`[BUNDLED] Downloading cloudflared from ${downloadUrl}...`);

  if (isTarball) {
    // Download to temp file, extract, then delete
    const tempPath = path.join(getLocalBinDir(), 'cloudflared.tgz');
    await downloadFile(downloadUrl, tempPath);

    console.log(`[BUNDLED] Extracting cloudflared...`);

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

  console.log(`[BUNDLED] cloudflared installed to ${outputPath}`);

  return outputPath;
}

/**
 * Download Ollama installer/binary
 */
export async function downloadOllama(): Promise<string> {
  const platform = os.platform();

  if (platform === 'darwin') {
    // On macOS, download the .app bundle
    throw new Error('Ollama auto-download for macOS requires manual installation. Please install from https://ollama.com/download');
  } else if (platform === 'linux') {
    // On Linux, use the install script
    console.log('[BUNDLED] Installing Ollama via install script...');
    try {
      await execAsync('curl -fsSL https://ollama.com/install.sh | sh');
      return '/usr/local/bin/ollama';
    } catch (err) {
      throw new Error(`Failed to install Ollama: ${err}`);
    }
  } else if (platform === 'win32') {
    // On Windows, download the installer
    throw new Error('Ollama auto-download for Windows requires manual installation. Please install from https://ollama.com/download');
  }

  throw new Error(`Unsupported platform: ${platform}`);
}

/**
 * Check if cloudflared is installed
 */
export function isCloudflaredInstalled(): boolean {
  return getCloudflaredPath() !== null;
}

/**
 * Check if Ollama is installed
 */
export async function isOllamaInstalled(): Promise<boolean> {
  try {
    await execAsync('which ollama');
    return true;
  } catch {
    // Check common installation paths
    const paths = [
      '/usr/local/bin/ollama',
      '/usr/bin/ollama',
      path.join(os.homedir(), '.ollama', 'bin', 'ollama'),
    ];

    return paths.some(p => existsSync(p));
  }
}

/**
 * Get Ollama binary path
 */
export async function getOllamaPath(): Promise<string | null> {
  try {
    const { stdout } = await execAsync('which ollama');
    return stdout.trim();
  } catch {
    const paths = [
      '/usr/local/bin/ollama',
      '/usr/bin/ollama',
      path.join(os.homedir(), '.ollama', 'bin', 'ollama'),
    ];

    for (const p of paths) {
      if (existsSync(p)) {
        return p;
      }
    }

    return null;
  }
}

/**
 * Get installation instructions for manual installation
 */
export function getInstallInstructions(binary: 'cloudflared' | 'ollama'): string {
  const platform = os.platform();

  if (binary === 'cloudflared') {
    if (platform === 'darwin') {
      return 'Install cloudflared:\n\nbrew install cloudflared\n\nOr download from:\nhttps://github.com/cloudflare/cloudflared/releases';
    } else if (platform === 'linux') {
      return 'Install cloudflared:\n\nDebian/Ubuntu:\nwget -q https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64\nsudo mv cloudflared-linux-amd64 /usr/local/bin/cloudflared\nsudo chmod +x /usr/local/bin/cloudflared';
    } else if (platform === 'win32') {
      return 'Install cloudflared:\n\nDownload from:\nhttps://github.com/cloudflare/cloudflared/releases\n\nOr use winget:\nwinget install --id Cloudflare.cloudflared';
    }
  } else if (binary === 'ollama') {
    return `Install Ollama from:\n\nhttps://ollama.com/download\n\nOllama provides native installers for macOS, Linux, and Windows.`;
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
  const ollamaPath = await getOllamaPath();

  return [
    {
      name: 'cloudflared',
      version: 'latest',
      path: cloudflaredPath || 'not installed',
      installed: cloudflaredPath !== null,
    },
    {
      name: 'ollama',
      version: 'latest',
      path: ollamaPath || 'not installed',
      installed: ollamaPath !== null,
    },
  ];
}
