// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useRef, useCallback, useEffect } from 'react';
import './Preview.css';

interface PreviewProps {
  projectPath: string;
}

type ProjectType = 'vite' | 'nextjs' | 'static' | 'unknown';

// Track server state globally so it persists across tab switches
let globalServerRunning = false;
let globalServerPort = 8080;
let globalServerPid: number | null = null;
let globalProjectType: ProjectType = 'unknown';

export function Preview({ projectPath }: PreviewProps) {
  const [url, setUrl] = useState(`http://localhost:${globalServerPort}`);
  const [isRunning, setIsRunning] = useState(globalServerRunning);
  const [serverOutput, setServerOutput] = useState('');
  const [port, setPort] = useState(globalServerPort);
  const [projectType, setProjectType] = useState<ProjectType>(globalProjectType);
  const [detecting, setDetecting] = useState(false);
  const [deploying, setDeploying] = useState(false);
  const [deploymentUrl, setDeploymentUrl] = useState<string | null>(null);
  const [sharingTunnel, setSharingTunnel] = useState(false);
  const [tunnelUrl, setTunnelUrl] = useState<string | null>(null);
  const iframeRef = useRef<HTMLIFrameElement>(null);

  // Detect project type on mount or when project changes
  useEffect(() => {
    detectProjectType();
  }, [projectPath]);

  // Sync with global state on mount
  useEffect(() => {
    setIsRunning(globalServerRunning);
    setUrl(`http://localhost:${globalServerPort}`);
    if (globalServerRunning) {
      setServerOutput(`Server running on port ${globalServerPort}`);
    }
  }, []);

  const detectProjectType = useCallback(async () => {
    setDetecting(true);
    try {
      // Check for package.json
      const packageJsonPath = `${projectPath}/package.json`;
      let packageJson: any = null;
      try {
        const result = await window.teddy.readFile(packageJsonPath);
        packageJson = JSON.parse(result.content);
      } catch (err) {
        // No package.json, probably static site
        setProjectType('static');
        globalProjectType = 'static';
        setDetecting(false);
        return;
      }

      // Check scripts for Vite
      if (packageJson?.scripts) {
        const scripts = packageJson.scripts;
        const devScript = scripts.dev || scripts.start || '';

        if (devScript.includes('vite')) {
          setProjectType('vite');
          globalProjectType = 'vite';
          setPort(5173);
          globalServerPort = 5173;
          setUrl('http://localhost:5173');
        } else if (devScript.includes('next')) {
          setProjectType('nextjs');
          globalProjectType = 'nextjs';
          setPort(3000);
          globalServerPort = 3000;
          setUrl('http://localhost:3000');
        } else {
          // Check dependencies
          const deps = { ...packageJson.dependencies, ...packageJson.devDependencies };
          if (deps.vite) {
            setProjectType('vite');
            globalProjectType = 'vite';
            setPort(5173);
            globalServerPort = 5173;
            setUrl('http://localhost:5173');
          } else if (deps.next) {
            setProjectType('nextjs');
            globalProjectType = 'nextjs';
            setPort(3000);
            globalServerPort = 3000;
            setUrl('http://localhost:3000');
          } else {
            setProjectType('static');
            globalProjectType = 'static';
          }
        }
      } else {
        setProjectType('static');
        globalProjectType = 'static';
      }
    } catch (err) {
      console.error('Failed to detect project type:', err);
      setProjectType('unknown');
      globalProjectType = 'unknown';
    } finally {
      setDetecting(false);
    }
  }, [projectPath]);

  const startServer = useCallback(async () => {
    if (isRunning) {
      // Stop server
      if (globalServerPid) {
        try {
          await window.teddy.killShell(globalServerPid);
        } catch (err) {
          console.error('Failed to kill server:', err);
        }
        globalServerPid = null;
      }
      setIsRunning(false);
      globalServerRunning = false;
      setServerOutput('Server stopped');
      return;
    }

    // Determine command based on project type
    let command: string;
    let startupMessage: string;

    switch (projectType) {
      case 'vite':
        command = 'npm run dev';
        startupMessage = `Starting Vite dev server on port ${port}...`;
        break;
      case 'nextjs':
        command = 'npm run dev';
        startupMessage = `Starting Next.js dev server on port ${port}...`;
        break;
      case 'static':
      default:
        command = `python3 -m http.server ${port}`;
        startupMessage = `Starting static server on port ${port}...`;
        break;
    }

    setServerOutput(startupMessage);
    globalServerPort = port;
    setUrl(`http://localhost:${port}`);

    try {
      const result = await window.teddy.runShell(command);
      if (result.success && result.pid) {
        globalServerPid = result.pid;
        setIsRunning(true);
        globalServerRunning = true;

        const typeLabel = projectType === 'vite' ? 'Vite' :
                         projectType === 'nextjs' ? 'Next.js' :
                         'Static';
        setServerOutput(`${typeLabel} server running on port ${port}`);
      } else {
        setServerOutput('Failed to start server');
      }
    } catch (err) {
      console.error('Failed to start server:', err);
      setServerOutput(`Failed to start server: ${err}`);
    }
  }, [isRunning, port, projectType]);

  const refreshPreview = useCallback(() => {
    if (iframeRef.current) {
      // Force reload by setting src to empty then back
      const currentSrc = iframeRef.current.src;
      iframeRef.current.src = '';
      setTimeout(() => {
        if (iframeRef.current) {
          iframeRef.current.src = currentSrc;
        }
      }, 100);
    }
  }, []);

  const handleUrlKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      if (iframeRef.current) {
        iframeRef.current.src = url;
      }
    }
  }, [url]);

  const handlePortChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const newPort = parseInt(e.target.value) || 8080;
    setPort(newPort);
    globalServerPort = newPort;
    setUrl(`http://localhost:${newPort}`);
  }, []);

  const getProjectTypeLabel = () => {
    if (detecting) return '🔍 Detecting...';
    switch (projectType) {
      case 'vite': return '⚡ Vite';
      case 'nextjs': return '▲ Next.js';
      case 'static': return '📄 Static';
      default: return '❓ Unknown';
    }
  };

  const deployToVercel = useCallback(async () => {
    setDeploying(true);
    setServerOutput('Preparing deployment to Vercel...');
    setDeploymentUrl(null);

    try {
      // Get settings to retrieve Vercel token
      const settings = await window.teddy.getSettings();

      if (!settings.vercelToken) {
        alert('Please configure your Vercel token in Settings → Deployment tab first');
        setServerOutput('✗ Deployment failed: No Vercel token configured');
        return;
      }

      setServerOutput('Uploading files to Vercel...');

      const result = await window.teddy.deployVercel({
        projectPath,
        vercelToken: settings.vercelToken,
      });

      if (result.success && result.url) {
        setDeploymentUrl(result.url);
        setServerOutput(`✓ Deployed successfully! Click to open: ${result.url}`);
      } else {
        setServerOutput(`✗ Deployment failed: ${result.error || 'Unknown error'}`);
        alert(`Deployment failed: ${result.error}`);
      }
    } catch (err) {
      console.error('Deployment error:', err);
      setServerOutput(`✗ Deployment failed: ${err}`);
      alert(`Deployment failed: ${err}`);
    } finally {
      setDeploying(false);
    }
  }, [projectPath]);

  const shareViaTunnel = useCallback(async () => {
    if (tunnelUrl) {
      // Already sharing - stop tunnel
      try {
        await window.teddy.tunnelStop(port);
        setTunnelUrl(null);
        setSharingTunnel(false);
        setServerOutput('Tunnel stopped');
      } catch (err) {
        console.error('Failed to stop tunnel:', err);
      }
      return;
    }

    if (!isRunning) {
      alert('Please start the dev server first before sharing');
      return;
    }

    setSharingTunnel(true);
    setServerOutput('Creating Cloudflare Tunnel...');

    try {
      // Check if cloudflared is installed
      const { installed } = await window.teddy.tunnelIsInstalled();

      if (!installed) {
        // Offer to auto-download cloudflared
        const shouldInstall = confirm(
          'Cloudflared is not installed.\n\n' +
          'Would you like Teddy to download and install it automatically?\n\n' +
          '(This will download the official cloudflared binary from GitHub)'
        );

        if (!shouldInstall) {
          const { instructions } = await window.teddy.tunnelGetInstallInstructions();
          alert(`Installation cancelled.\n\nTo install manually:\n\n${instructions}`);
          setServerOutput('✗ cloudflared not installed');
          return;
        }

        // Auto-download cloudflared
        setServerOutput('Downloading cloudflared...');
        const installResult = await window.teddy.tunnelAutoInstall();

        if (!installResult.success) {
          alert(`Failed to install cloudflared: ${installResult.error}`);
          setServerOutput('✗ Failed to install cloudflared');
          return;
        }

        setServerOutput(`✓ cloudflared installed to ${installResult.path}`);
        // Give user a moment to see the success message
        await new Promise(resolve => setTimeout(resolve, 1000));
        setServerOutput('Creating Cloudflare Tunnel...');
      }

      const result = await window.teddy.tunnelStart({ port });

      if (result.success && result.url) {
        setTunnelUrl(result.url);
        setServerOutput(`✓ Sharing at: ${result.url}`);
        // Copy to clipboard
        navigator.clipboard.writeText(result.url).catch(() => {});
      } else {
        setServerOutput(`✗ Failed to create tunnel: ${result.error}`);
        alert(`Failed to create tunnel: ${result.error}`);
      }
    } catch (err) {
      console.error('Tunnel error:', err);
      setServerOutput(`✗ Tunnel failed: ${err}`);
      alert(`Tunnel failed: ${err}`);
    } finally {
      setSharingTunnel(false);
    }
  }, [port, isRunning, tunnelUrl]);

  return (
    <div className="preview-container">
      <div className="preview-toolbar">
        <div className="project-type-badge" title={`Detected project type: ${projectType}`}>
          {getProjectTypeLabel()}
        </div>
        <input
          type="text"
          className="preview-url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={handleUrlKeyDown}
          placeholder="Enter preview URL"
        />
        <div className="preview-port">
          <label>Port:</label>
          <input
            type="number"
            value={port}
            onChange={handlePortChange}
            min={1024}
            max={65535}
            disabled={isRunning}
          />
        </div>
        <button
          className={`btn-secondary btn-small ${isRunning ? 'server-running' : ''}`}
          onClick={startServer}
          disabled={detecting}
        >
          {isRunning ? '⬛ Stop' : '▶ Start Server'}
        </button>
        <button
          className="btn-secondary btn-small"
          onClick={refreshPreview}
          title="Refresh preview"
        >
          ↻
        </button>
        <button
          className="btn-secondary btn-small"
          onClick={detectProjectType}
          title="Re-detect project type"
          disabled={detecting}
        >
          🔍
        </button>
        <button
          className={`btn-secondary btn-small ${tunnelUrl ? 'server-running' : ''}`}
          onClick={shareViaTunnel}
          title={tunnelUrl ? 'Stop sharing' : 'Share via Cloudflare Tunnel'}
          disabled={sharingTunnel || detecting || deploying}
        >
          {sharingTunnel ? '⏳ Creating...' : tunnelUrl ? '⬛ Stop Share' : '🔗 Share'}
        </button>
        <button
          className="btn-primary btn-small"
          onClick={deployToVercel}
          title="Deploy to Vercel"
          disabled={deploying || detecting}
        >
          {deploying ? '⏳ Deploying...' : '🚀 Deploy'}
        </button>
      </div>

      <div className="preview-content">
        <iframe
          ref={iframeRef}
          src={url}
          title="Preview"
          className="preview-frame"
          sandbox="allow-same-origin allow-scripts allow-forms allow-popups"
        />
      </div>

      {serverOutput && (
        <div className={`preview-status ${isRunning || tunnelUrl ? 'status-running' : ''}`}>
          {(isRunning || tunnelUrl) && <span className="status-dot"></span>}
          {deploymentUrl || tunnelUrl ? (
            <a href={deploymentUrl || tunnelUrl || '#'} target="_blank" rel="noopener noreferrer" style={{ color: 'inherit', textDecoration: 'underline' }}>
              {serverOutput}
            </a>
          ) : (
            serverOutput
          )}
          {tunnelUrl && (
            <button
              onClick={() => navigator.clipboard.writeText(tunnelUrl)}
              style={{
                marginLeft: '8px',
                padding: '2px 8px',
                fontSize: '11px',
                cursor: 'pointer',
                background: 'var(--bg-tertiary)',
                border: '1px solid var(--border-color)',
                borderRadius: '3px',
                color: 'var(--text-primary)',
              }}
            >
              📋 Copy
            </button>
          )}
        </div>
      )}
    </div>
  );
}
