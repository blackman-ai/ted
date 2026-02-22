// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Vercel deployment integration
 *
 * Handles deployment to Vercel via REST API
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import { debugLog } from '../utils/logger';

const VERCEL_API_BASE = 'https://api.vercel.com';

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

interface VercelFile {
  file: string;
  sha: string;
  size: number;
}

/**
 * Verify a Vercel API token is valid
 */
export async function verifyToken(token: string): Promise<{ valid: boolean; error?: string }> {
  try {
    const response = await fetch(`${VERCEL_API_BASE}/v2/user`, {
      headers: {
        'Authorization': `Bearer ${token}`,
      },
    });

    if (response.ok) {
      return { valid: true };
    } else if (response.status === 401 || response.status === 403) {
      return { valid: false, error: 'Invalid or expired token' };
    } else {
      return { valid: false, error: `Verification failed: ${response.statusText}` };
    }
  } catch (error) {
    return { valid: false, error: `Network error: ${error}` };
  }
}

/**
 * Get the status of a deployment
 */
export async function getDeploymentStatus(
  deploymentId: string,
  token: string
): Promise<DeploymentStatus> {
  const response = await fetch(`${VERCEL_API_BASE}/v13/deployments/${deploymentId}`, {
    headers: {
      'Authorization': `Bearer ${token}`,
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to get deployment status: ${response.statusText}`);
  }

  const data = await response.json();
  return {
    id: data.id || data.uid,
    url: data.url,
    state: data.state || data.readyState,
    readyState: data.readyState,
    createdAt: data.createdAt || data.created,
  };
}

/**
 * Read and hash all files in a directory recursively
 */
async function getProjectFiles(projectPath: string): Promise<VercelFile[]> {
  const files: VercelFile[] = [];

  async function scanDir(dir: string, baseDir: string) {
    const entries = await fs.readdir(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      const relativePath = path.relative(baseDir, fullPath);

      // Skip common directories that shouldn't be deployed
      if (entry.isDirectory()) {
        if (['.git', 'node_modules', '.next', '.vercel', 'out', 'dist', '.teddy'].includes(entry.name)) {
          continue;
        }
        await scanDir(fullPath, baseDir);
      } else {
        // Read file and compute SHA
        const content = await fs.readFile(fullPath);
        const sha = crypto.createHash('sha1').update(content).digest('hex');
        const size = content.length;

        files.push({
          file: relativePath,
          sha,
          size,
        });
      }
    }
  }

  await scanDir(projectPath, projectPath);
  return files;
}

/**
 * Upload files to Vercel
 */
async function uploadFiles(
  projectPath: string,
  files: VercelFile[],
  token: string
): Promise<string[]> {
  const uploaded: string[] = [];

  for (const file of files) {
    const filePath = path.join(projectPath, file.file);
    const content = await fs.readFile(filePath);

    const response = await fetch(`${VERCEL_API_BASE}/v2/now/files`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/octet-stream',
        'x-vercel-digest': file.sha,
      },
      body: content,
    });

    if (!response.ok) {
      throw new Error(`Failed to upload ${file.file}: ${response.statusText}`);
    }

    uploaded.push(file.file);
  }

  return uploaded;
}

/**
 * Detect project framework and settings
 */
async function detectProjectSettings(projectPath: string): Promise<{
  name: string;
  framework?: string;
  buildCommand?: string;
  installCommand?: string;
  outputDirectory?: string;
}> {
  const packageJsonPath = path.join(projectPath, 'package.json');
  let name = path.basename(projectPath);
  let framework: string | undefined;
  let buildCommand: string | undefined;
  let outputDirectory: string | undefined;

  try {
    const packageJson = JSON.parse(await fs.readFile(packageJsonPath, 'utf-8'));
    name = packageJson.name || name;

    // Detect framework from dependencies
    const deps = { ...packageJson.dependencies, ...packageJson.devDependencies };

    if (deps.next) {
      framework = 'nextjs';
      buildCommand = 'next build';
      outputDirectory = '.next';
    } else if (deps.vite) {
      framework = 'vite';
      buildCommand = 'vite build';
      outputDirectory = 'dist';
    } else if (deps['create-react-app'] || deps['react-scripts']) {
      framework = 'create-react-app';
      buildCommand = 'react-scripts build';
      outputDirectory = 'build';
    }
  } catch (error) {
    // No package.json or can't read it - assume static site
  }

  return {
    name,
    framework,
    buildCommand,
    outputDirectory,
  };
}

/**
 * Deploy a project to Vercel
 */
export async function deployProject(options: DeploymentOptions): Promise<DeploymentResult> {
  const { projectPath, vercelToken, projectName, envVars } = options;

  try {
    // Verify token first
    const tokenCheck = await verifyToken(vercelToken);
    if (!tokenCheck.valid) {
      return {
        success: false,
        error: tokenCheck.error || 'Invalid token',
      };
    }

    // Detect project settings
    const settings = await detectProjectSettings(projectPath);
    const finalProjectName = projectName || settings.name;

    // Get all project files
    debugLog('[Vercel] Scanning project files...');
    const files = await getProjectFiles(projectPath);
    debugLog(`[Vercel] Found ${files.length} files`);

    // Upload files
    debugLog('[Vercel] Uploading files...');
    await uploadFiles(projectPath, files, vercelToken);
    debugLog('[Vercel] Files uploaded');

    // Create deployment
    debugLog('[Vercel] Creating deployment...');
    const deploymentPayload: Record<string, unknown> = {
      name: finalProjectName,
      files: files.map(f => ({
        file: f.file,
        sha: f.sha,
        size: f.size,
      })),
      projectSettings: {
        framework: settings.framework,
        buildCommand: settings.buildCommand,
        outputDirectory: settings.outputDirectory,
      },
      target: 'production',
    };

    // Add environment variables if provided
    if (envVars && Object.keys(envVars).length > 0) {
      deploymentPayload.env = envVars;
    }

    const response = await fetch(`${VERCEL_API_BASE}/v13/deployments`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${vercelToken}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(deploymentPayload),
    });

    if (!response.ok) {
      const errorData = await response.json().catch(() => ({}));
      throw new Error(errorData.error?.message || response.statusText);
    }

    const deployment = await response.json();
    debugLog('[Vercel] Deployment created:', deployment.id);

    return {
      success: true,
      url: `https://${deployment.url}`,
      deploymentId: deployment.id || deployment.uid,
    };
  } catch (error) {
    console.error('[Vercel] Deployment failed:', error);
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
