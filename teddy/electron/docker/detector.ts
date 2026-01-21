// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Docker detection module for Teddy
 * Detects Docker installation status and daemon availability
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export interface DockerStatus {
  installed: boolean;
  daemonRunning: boolean;
  version: string | null;
  composeVersion: string | null;
  error: string | null;
}

/**
 * Check if Docker is installed on the system
 */
export async function isDockerInstalled(): Promise<boolean> {
  try {
    await execAsync('docker --version');
    return true;
  } catch {
    return false;
  }
}

/**
 * Check if Docker daemon is running
 */
export async function isDockerDaemonRunning(): Promise<boolean> {
  try {
    await execAsync('docker info');
    return true;
  } catch {
    return false;
  }
}

/**
 * Get Docker version
 */
export async function getDockerVersion(): Promise<string | null> {
  try {
    const { stdout } = await execAsync('docker --version');
    // Parse "Docker version 24.0.7, build afdd53b"
    const match = stdout.match(/Docker version ([^,]+)/);
    return match ? match[1].trim() : stdout.trim();
  } catch {
    return null;
  }
}

/**
 * Get Docker Compose version
 */
export async function getDockerComposeVersion(): Promise<string | null> {
  try {
    const { stdout } = await execAsync('docker compose version');
    // Parse "Docker Compose version v2.23.0"
    const match = stdout.match(/version v?([^\s]+)/);
    return match ? match[1].trim() : stdout.trim();
  } catch {
    return null;
  }
}

/**
 * Get full Docker status
 */
export async function getDockerStatus(): Promise<DockerStatus> {
  const installed = await isDockerInstalled();

  if (!installed) {
    return {
      installed: false,
      daemonRunning: false,
      version: null,
      composeVersion: null,
      error: 'Docker is not installed',
    };
  }

  const daemonRunning = await isDockerDaemonRunning();
  const version = await getDockerVersion();
  const composeVersion = await getDockerComposeVersion();

  return {
    installed: true,
    daemonRunning,
    version,
    composeVersion,
    error: daemonRunning ? null : 'Docker daemon is not running. Please start Docker Desktop or the Docker service.',
  };
}

/**
 * Get Docker installation instructions for the current platform
 */
export function getDockerInstallInstructions(): string {
  const platform = process.platform;

  switch (platform) {
    case 'darwin':
      return `Docker is not installed on your Mac.

To install Docker Desktop for Mac:
1. Visit https://www.docker.com/products/docker-desktop
2. Download Docker Desktop for Mac
3. Open the .dmg file and drag Docker to Applications
4. Launch Docker from Applications
5. Wait for Docker to start (whale icon in menu bar)

Alternatively, using Homebrew:
  brew install --cask docker

After installation, launch Docker Desktop and ensure it's running before using PostgreSQL features.`;

    case 'win32':
      return `Docker is not installed on your Windows system.

To install Docker Desktop for Windows:
1. Visit https://www.docker.com/products/docker-desktop
2. Download Docker Desktop for Windows
3. Run the installer
4. Follow the installation wizard
5. Restart your computer if prompted
6. Launch Docker Desktop

Requirements:
- Windows 10/11 64-bit (Pro, Enterprise, or Education for Hyper-V)
- WSL 2 backend (recommended) or Hyper-V
- Enable virtualization in BIOS if needed

After installation, launch Docker Desktop and ensure it's running.`;

    case 'linux':
      return `Docker is not installed on your Linux system.

To install Docker on Ubuntu/Debian:
  curl -fsSL https://get.docker.com -o get-docker.sh
  sudo sh get-docker.sh
  sudo usermod -aG docker $USER
  newgrp docker

To install Docker on Fedora:
  sudo dnf install docker-ce docker-ce-cli containerd.io
  sudo systemctl start docker
  sudo systemctl enable docker
  sudo usermod -aG docker $USER

To install Docker on Arch:
  sudo pacman -S docker
  sudo systemctl start docker
  sudo systemctl enable docker
  sudo usermod -aG docker $USER

After installation, log out and back in for group changes to take effect.`;

    default:
      return `Docker is not installed.

Please visit https://www.docker.com/products/docker-desktop to download and install Docker for your platform.`;
  }
}

/**
 * Get instructions for starting the Docker daemon
 */
export function getDockerStartInstructions(): string {
  const platform = process.platform;

  switch (platform) {
    case 'darwin':
      return `Docker daemon is not running.

To start Docker on Mac:
1. Open Docker Desktop from Applications
2. Wait for the whale icon to appear in the menu bar
3. The icon should stop animating when Docker is ready

You can also start Docker from the terminal:
  open -a Docker`;

    case 'win32':
      return `Docker daemon is not running.

To start Docker on Windows:
1. Search for "Docker Desktop" in the Start menu
2. Launch Docker Desktop
3. Wait for the icon in the system tray to show "Docker Desktop is running"`;

    case 'linux':
      return `Docker daemon is not running.

To start Docker on Linux:
  sudo systemctl start docker

To enable Docker to start on boot:
  sudo systemctl enable docker

To check Docker status:
  sudo systemctl status docker`;

    default:
      return `Docker daemon is not running. Please start Docker Desktop or the Docker service.`;
  }
}
