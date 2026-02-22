// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Netlify deployment integration
 *
 * Handles deployment to Netlify via REST API
 * Uses the deploy API with file digests for efficient uploads
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import { debugLog } from '../utils/logger';

const NETLIFY_API_BASE = 'https://api.netlify.com/api/v1';

export interface NetlifyDeploymentOptions {
  projectPath: string;
  netlifyToken: string;
  siteName?: string;
  siteId?: string; // If deploying to existing site
}

export interface NetlifyDeploymentResult {
  success: boolean;
  url?: string;
  deployId?: string;
  siteId?: string;
  adminUrl?: string;
  error?: string;
}

export interface NetlifyDeploymentStatus {
  id: string;
  siteId: string;
  state: 'uploading' | 'uploaded' | 'processing' | 'ready' | 'error';
  url?: string;
  sslUrl?: string;
  adminUrl?: string;
  errorMessage?: string;
}

interface NetlifyFile {
  path: string;
  sha1: string;
  size: number;
}

interface NetlifySiteSummary {
  id: string;
  name: string;
  url: string;
  ssl_url?: string;
}

/**
 * Verify a Netlify API token is valid
 */
export async function verifyNetlifyToken(token: string): Promise<{ valid: boolean; error?: string }> {
  try {
    const response = await fetch(`${NETLIFY_API_BASE}/user`, {
      headers: {
        'Authorization': `Bearer ${token}`,
      },
    });

    if (response.ok) {
      return { valid: true };
    } else if (response.status === 401) {
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
export async function getNetlifyDeploymentStatus(
  deployId: string,
  token: string
): Promise<NetlifyDeploymentStatus> {
  const response = await fetch(`${NETLIFY_API_BASE}/deploys/${deployId}`, {
    headers: {
      'Authorization': `Bearer ${token}`,
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to get deployment status: ${response.statusText}`);
  }

  const data = await response.json();
  return {
    id: data.id,
    siteId: data.site_id,
    state: data.state,
    url: data.url,
    sslUrl: data.ssl_url,
    adminUrl: data.admin_url,
    errorMessage: data.error_message,
  };
}

/**
 * Read and hash all files in a directory recursively
 * Netlify uses SHA1 hashes for file deduplication
 */
async function getProjectFiles(projectPath: string): Promise<NetlifyFile[]> {
  const files: NetlifyFile[] = [];

  async function scanDir(dir: string, baseDir: string) {
    const entries = await fs.readdir(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      const relativePath = '/' + path.relative(baseDir, fullPath).replace(/\\/g, '/');

      // Skip common directories that shouldn't be deployed
      if (entry.isDirectory()) {
        if (['.git', 'node_modules', '.next', '.netlify', 'out', '.teddy', '.vercel'].includes(entry.name)) {
          continue;
        }
        await scanDir(fullPath, baseDir);
      } else {
        // Skip hidden files and common non-deployable files
        if (entry.name.startsWith('.') && entry.name !== '.htaccess') {
          continue;
        }

        // Read file and compute SHA1
        const content = await fs.readFile(fullPath);
        const sha1 = crypto.createHash('sha1').update(content).digest('hex');
        const size = content.length;

        files.push({
          path: relativePath,
          sha1,
          size,
        });
      }
    }
  }

  await scanDir(projectPath, projectPath);
  return files;
}

/**
 * Create or get a Netlify site
 */
async function getOrCreateSite(
  token: string,
  siteName?: string
): Promise<{ id: string; name: string; url: string }> {
  // If siteName provided, try to find existing site
  if (siteName) {
    const listResponse = await fetch(`${NETLIFY_API_BASE}/sites?name=${encodeURIComponent(siteName)}`, {
      headers: {
        'Authorization': `Bearer ${token}`,
      },
    });

    if (listResponse.ok) {
      const sites = await listResponse.json() as NetlifySiteSummary[];
      const existingSite = sites.find((s) => s.name === siteName);
      if (existingSite) {
        return {
          id: existingSite.id,
          name: existingSite.name,
          url: existingSite.ssl_url || existingSite.url,
        };
      }
    }
  }

  // Create new site
  const createResponse = await fetch(`${NETLIFY_API_BASE}/sites`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      name: siteName,
    }),
  });

  if (!createResponse.ok) {
    const error = await createResponse.json().catch(() => ({}));
    throw new Error(error.message || `Failed to create site: ${createResponse.statusText}`);
  }

  const site = await createResponse.json();
  return {
    id: site.id,
    name: site.name,
    url: site.ssl_url || site.url,
  };
}

/**
 * Detect the publish directory based on project type
 */
async function detectPublishDirectory(projectPath: string): Promise<string> {
  const packageJsonPath = path.join(projectPath, 'package.json');

  try {
    const packageJson = JSON.parse(await fs.readFile(packageJsonPath, 'utf-8'));
    const deps = { ...packageJson.dependencies, ...packageJson.devDependencies };

    // Check for built output directories
    if (deps.next) {
      // Next.js - check for static export
      const outDir = path.join(projectPath, 'out');
      try {
        await fs.access(outDir);
        return outDir;
      } catch {
        // No static export, would need next export or use Netlify's Next.js runtime
        // For simplicity, we'll deploy the whole project
        return projectPath;
      }
    } else if (deps.vite) {
      const distDir = path.join(projectPath, 'dist');
      try {
        await fs.access(distDir);
        return distDir;
      } catch {
        return projectPath;
      }
    } else if (deps['react-scripts']) {
      const buildDir = path.join(projectPath, 'build');
      try {
        await fs.access(buildDir);
        return buildDir;
      } catch {
        return projectPath;
      }
    }
  } catch {
    // No package.json - static site
  }

  // Check for common static output directories
  for (const dir of ['dist', 'build', 'public', 'out', '_site']) {
    const fullPath = path.join(projectPath, dir);
    try {
      const stat = await fs.stat(fullPath);
      if (stat.isDirectory()) {
        // Check if it has an index.html
        try {
          await fs.access(path.join(fullPath, 'index.html'));
          return fullPath;
        } catch {
          // No index.html, continue checking
        }
      }
    } catch {
      // Directory doesn't exist
    }
  }

  // Default to project root (static site)
  return projectPath;
}

/**
 * Deploy a project to Netlify
 *
 * Netlify's deploy process:
 * 1. Create a new deploy with file digest list
 * 2. Netlify returns which files need to be uploaded (not already in their CDN)
 * 3. Upload only the required files
 * 4. Deploy goes live automatically
 */
export async function deployNetlifyProject(options: NetlifyDeploymentOptions): Promise<NetlifyDeploymentResult> {
  const { projectPath, netlifyToken, siteName, siteId: existingSiteId } = options;

  try {
    // Verify token first
    const tokenCheck = await verifyNetlifyToken(netlifyToken);
    if (!tokenCheck.valid) {
      return {
        success: false,
        error: tokenCheck.error || 'Invalid token',
      };
    }

    // Get or create site
    let siteId = existingSiteId;

    if (!siteId) {
      debugLog('[Netlify] Getting or creating site...');
      const site = await getOrCreateSite(netlifyToken, siteName);
      siteId = site.id;
      debugLog(`[Netlify] Using site: ${site.name} (${siteId}) - ${site.url}`);
    }

    // Detect publish directory
    const publishDir = await detectPublishDirectory(projectPath);
    debugLog(`[Netlify] Publish directory: ${publishDir}`);

    // Get all project files from publish directory
    debugLog('[Netlify] Scanning project files...');
    const files = await getProjectFiles(publishDir);
    debugLog(`[Netlify] Found ${files.length} files`);

    if (files.length === 0) {
      return {
        success: false,
        error: 'No files found to deploy. Make sure your project is built.',
      };
    }

    // Create file digest map (path -> sha1)
    const fileDigests: Record<string, string> = {};
    for (const file of files) {
      fileDigests[file.path] = file.sha1;
    }

    // Create deploy
    debugLog('[Netlify] Creating deployment...');
    const deployResponse = await fetch(`${NETLIFY_API_BASE}/sites/${siteId}/deploys`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${netlifyToken}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        files: fileDigests,
      }),
    });

    if (!deployResponse.ok) {
      const error = await deployResponse.json().catch(() => ({}));
      throw new Error(error.message || `Failed to create deploy: ${deployResponse.statusText}`);
    }

    const deploy = await deployResponse.json();
    const deployId = deploy.id;
    const requiredFiles = deploy.required || [];

    debugLog(`[Netlify] Deploy created: ${deployId}`);
    debugLog(`[Netlify] Files to upload: ${requiredFiles.length} of ${files.length}`);

    // Upload required files
    if (requiredFiles.length > 0) {
      debugLog('[Netlify] Uploading files...');

      for (const sha1 of requiredFiles) {
        // Find the file with this sha1
        const file = files.find(f => f.sha1 === sha1);
        if (!file) continue;

        const filePath = path.join(publishDir, file.path.slice(1)); // Remove leading /
        const content = await fs.readFile(filePath);

        const uploadResponse = await fetch(
          `${NETLIFY_API_BASE}/deploys/${deployId}/files${file.path}`,
          {
            method: 'PUT',
            headers: {
              'Authorization': `Bearer ${netlifyToken}`,
              'Content-Type': 'application/octet-stream',
            },
            body: content,
          }
        );

        if (!uploadResponse.ok) {
          console.error(`[Netlify] Failed to upload ${file.path}: ${uploadResponse.statusText}`);
        }
      }

      debugLog('[Netlify] Files uploaded');
    }

    // Wait for deploy to be ready (poll status)
    debugLog('[Netlify] Waiting for deployment to process...');
    let status: NetlifyDeploymentStatus;
    let attempts = 0;
    const maxAttempts = 60; // 2 minutes max

    do {
      await new Promise(resolve => setTimeout(resolve, 2000));
      status = await getNetlifyDeploymentStatus(deployId, netlifyToken);
      attempts++;

      if (status.state === 'error') {
        throw new Error(status.errorMessage || 'Deployment failed');
      }
    } while (status.state !== 'ready' && attempts < maxAttempts);

    if (status.state !== 'ready') {
      return {
        success: false,
        error: 'Deployment timed out. Check Netlify dashboard for status.',
        deployId,
        siteId,
      };
    }

    debugLog(`[Netlify] Deployment ready: ${status.sslUrl || status.url}`);

    return {
      success: true,
      url: status.sslUrl || status.url,
      deployId,
      siteId,
      adminUrl: status.adminUrl,
    };
  } catch (error) {
    console.error('[Netlify] Deployment failed:', error);
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
