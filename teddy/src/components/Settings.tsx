// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect } from 'react';
import './Settings.css';

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
  // Provider settings
  provider: string;
  model: string;

  // Anthropic
  anthropicApiKey: string;
  anthropicModel: string;

  // Ollama
  ollamaBaseUrl: string;
  ollamaModel: string;

  // OpenRouter
  openrouterApiKey: string;
  openrouterModel: string;

  // Deployment
  vercelToken: string;
  netlifyToken: string;

  // Hardware info
  hardware: HardwareInfo | null;
}

interface SettingsProps {
  onClose: () => void;
}

interface DockerStatus {
  installed: boolean;
  daemonRunning: boolean;
  version: string | null;
  composeVersion: string | null;
  error: string | null;
}

interface PostgresStatus {
  installed: boolean;
  running: boolean;
  containerId: string | null;
  databaseUrl: string | null;
  port: number;
  dataDir: string;
}

export function Settings({ onClose }: SettingsProps) {
  const [settings, setSettings] = useState<TedSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [activeTab, setActiveTab] = useState<'providers' | 'deployment' | 'database' | 'hardware'>('providers');
  const [verifyingVercelToken, setVerifyingVercelToken] = useState(false);
  const [vercelTokenValid, setVercelTokenValid] = useState<boolean | null>(null);
  const [verifyingNetlifyToken, setVerifyingNetlifyToken] = useState(false);
  const [netlifyTokenValid, setNetlifyTokenValid] = useState<boolean | null>(null);

  // Docker & PostgreSQL state
  const [dockerStatus, setDockerStatus] = useState<DockerStatus | null>(null);
  const [postgresStatus, setPostgresStatus] = useState<PostgresStatus | null>(null);
  const [dockerLoading, setDockerLoading] = useState(false);
  const [postgresLoading, setPostgresLoading] = useState(false);
  const [postgresLogs, setPostgresLogs] = useState<string | null>(null);
  const [showLogs, setShowLogs] = useState(false);
  const [dockerInstructions, setDockerInstructions] = useState<string | null>(null);

  useEffect(() => {
    loadSettings();
  }, []);

  useEffect(() => {
    if (activeTab === 'database') {
      loadDockerStatus();
      loadPostgresStatus();
    }
  }, [activeTab]);

  const loadSettings = async () => {
    try {
      const result = await window.teddy.getSettings();
      setSettings(result);
    } catch (err) {
      console.error('Failed to load settings:', err);
    } finally {
      setLoading(false);
    }
  };

  const loadDockerStatus = async () => {
    setDockerLoading(true);
    try {
      const status = await window.teddy.dockerGetStatus();
      setDockerStatus(status);

      if (!status.installed) {
        const { instructions } = await window.teddy.dockerGetInstallInstructions();
        setDockerInstructions(instructions);
      } else if (!status.daemonRunning) {
        const { instructions } = await window.teddy.dockerGetStartInstructions();
        setDockerInstructions(instructions);
      } else {
        setDockerInstructions(null);
      }
    } catch (err) {
      console.error('Failed to load Docker status:', err);
    } finally {
      setDockerLoading(false);
    }
  };

  const loadPostgresStatus = async () => {
    try {
      const status = await window.teddy.postgresGetStatus();
      setPostgresStatus(status);
    } catch (err) {
      console.error('Failed to load PostgreSQL status:', err);
    }
  };

  const startPostgres = async () => {
    setPostgresLoading(true);
    try {
      const result = await window.teddy.postgresStart();
      if (result.success) {
        await loadPostgresStatus();
      } else {
        alert(`Failed to start PostgreSQL: ${result.error}`);
      }
    } catch (err) {
      console.error('Failed to start PostgreSQL:', err);
      alert('Failed to start PostgreSQL');
    } finally {
      setPostgresLoading(false);
    }
  };

  const stopPostgres = async () => {
    setPostgresLoading(true);
    try {
      const result = await window.teddy.postgresStop();
      if (result.success) {
        await loadPostgresStatus();
      } else {
        alert(`Failed to stop PostgreSQL: ${result.error}`);
      }
    } catch (err) {
      console.error('Failed to stop PostgreSQL:', err);
      alert('Failed to stop PostgreSQL');
    } finally {
      setPostgresLoading(false);
    }
  };

  const loadPostgresLogs = async () => {
    try {
      const { logs } = await window.teddy.postgresGetLogs(100);
      setPostgresLogs(logs);
      setShowLogs(true);
    } catch (err) {
      console.error('Failed to load PostgreSQL logs:', err);
    }
  };

  const testPostgresConnection = async () => {
    try {
      const result = await window.teddy.postgresTestConnection();
      if (result.success) {
        alert('Connection successful! PostgreSQL is ready.');
      } else {
        alert(`Connection failed: ${result.error}`);
      }
    } catch (err) {
      console.error('Failed to test connection:', err);
      alert('Failed to test connection');
    }
  };

  const copyDatabaseUrl = async () => {
    if (postgresStatus?.databaseUrl) {
      await navigator.clipboard.writeText(postgresStatus.databaseUrl);
      alert('DATABASE_URL copied to clipboard!');
    }
  };

  const saveSettings = async () => {
    if (!settings) return;

    setSaving(true);
    try {
      await window.teddy.saveSettings(settings);
      onClose();
    } catch (err) {
      console.error('Failed to save settings:', err);
      alert('Failed to save settings');
    } finally {
      setSaving(false);
    }
  };

  const detectHardware = async () => {
    try {
      const hardware = await window.teddy.detectHardware();
      setSettings(prev => prev ? { ...prev, hardware } : null);
    } catch (err) {
      console.error('Failed to detect hardware:', err);
      alert('Failed to detect hardware');
    }
  };

  const verifyVercelToken = async () => {
    if (!settings?.vercelToken) {
      alert('Please enter a Vercel token first');
      return;
    }

    setVerifyingVercelToken(true);
    setVercelTokenValid(null);
    try {
      const result = await window.teddy.verifyVercelToken(settings.vercelToken);
      setVercelTokenValid(result.valid);
      if (!result.valid) {
        alert(`Token verification failed: ${result.error}`);
      }
    } catch (err) {
      console.error('Failed to verify token:', err);
      setVercelTokenValid(false);
      alert('Failed to verify token');
    } finally {
      setVerifyingVercelToken(false);
    }
  };

  const verifyNetlifyToken = async () => {
    if (!settings?.netlifyToken) {
      alert('Please enter a Netlify token first');
      return;
    }

    setVerifyingNetlifyToken(true);
    setNetlifyTokenValid(null);
    try {
      const result = await window.teddy.verifyNetlifyToken(settings.netlifyToken);
      setNetlifyTokenValid(result.valid);
      if (!result.valid) {
        alert(`Token verification failed: ${result.error}`);
      }
    } catch (err) {
      console.error('Failed to verify token:', err);
      setNetlifyTokenValid(false);
      alert('Failed to verify token');
    } finally {
      setVerifyingNetlifyToken(false);
    }
  };

  if (loading) {
    return (
      <div className="settings-overlay">
        <div className="settings-modal">
          <div className="settings-loading">Loading settings...</div>
        </div>
      </div>
    );
  }

  if (!settings) {
    return (
      <div className="settings-overlay">
        <div className="settings-modal">
          <div className="settings-error">Failed to load settings</div>
          <button onClick={onClose}>Close</button>
        </div>
      </div>
    );
  }

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <h2>Settings</h2>
          <button className="close-button" onClick={onClose}>×</button>
        </div>

        <div className="settings-tabs">
          <button
            className={`settings-tab ${activeTab === 'providers' ? 'active' : ''}`}
            onClick={() => setActiveTab('providers')}
          >
            AI Providers
          </button>
          <button
            className={`settings-tab ${activeTab === 'deployment' ? 'active' : ''}`}
            onClick={() => setActiveTab('deployment')}
          >
            Deployment
          </button>
          <button
            className={`settings-tab ${activeTab === 'database' ? 'active' : ''}`}
            onClick={() => setActiveTab('database')}
          >
            Database
          </button>
          <button
            className={`settings-tab ${activeTab === 'hardware' ? 'active' : ''}`}
            onClick={() => setActiveTab('hardware')}
          >
            Hardware
          </button>
        </div>

        <div className="settings-content">
          {activeTab === 'providers' && (
            <div className="settings-section">
              <h3>Provider Configuration</h3>

              <div className="form-group">
                <label htmlFor="provider">Default Provider</label>
                <select
                  id="provider"
                  value={settings.provider}
                  onChange={(e) => {
                    const newProvider = e.target.value;
                    // Sync the model field when provider changes
                    let newModel = settings.model;
                    if (newProvider === 'ollama') {
                      newModel = settings.ollamaModel;
                    } else if (newProvider === 'openrouter') {
                      newModel = settings.openrouterModel;
                    } else if (newProvider === 'anthropic') {
                      newModel = settings.anthropicModel;
                    }
                    setSettings({ ...settings, provider: newProvider, model: newModel });
                  }}
                >
                  <option value="anthropic">Anthropic Claude</option>
                  <option value="ollama">Ollama (Local)</option>
                  <option value="openrouter">OpenRouter</option>
                </select>
              </div>

              {settings.provider === 'anthropic' && (
                <>
                  <h4>Anthropic Settings</h4>
                  <div className="form-group">
                    <label htmlFor="anthropic-key">API Key</label>
                    <input
                      type="password"
                      id="anthropic-key"
                      value={settings.anthropicApiKey}
                      onChange={(e) => setSettings({ ...settings, anthropicApiKey: e.target.value })}
                      placeholder="sk-ant-..."
                    />
                    <small>You can also set ANTHROPIC_API_KEY environment variable</small>
                  </div>
                  <div className="form-group">
                    <label htmlFor="anthropic-model">Model</label>
                    <select
                      id="anthropic-model"
                      value={settings.anthropicModel}
                      onChange={(e) => setSettings({ ...settings, anthropicModel: e.target.value, model: e.target.value })}
                    >
                      <option value="claude-sonnet-4-20250514">Claude Sonnet 4</option>
                      <option value="claude-3.5-sonnet-20241022">Claude 3.5 Sonnet</option>
                      <option value="claude-3.5-haiku-20241022">Claude 3.5 Haiku</option>
                    </select>
                  </div>
                </>
              )}

              {settings.provider === 'ollama' && (
                <>
                  <h4>Ollama Settings</h4>
                  <div className="form-group">
                    <label htmlFor="ollama-url">Base URL</label>
                    <input
                      type="text"
                      id="ollama-url"
                      value={settings.ollamaBaseUrl}
                      onChange={(e) => setSettings({ ...settings, ollamaBaseUrl: e.target.value })}
                      placeholder="http://localhost:11434"
                    />
                  </div>
                  <div className="form-group">
                    <label htmlFor="ollama-model">Model</label>
                    <input
                      type="text"
                      id="ollama-model"
                      value={settings.ollamaModel}
                      onChange={(e) => setSettings({ ...settings, ollamaModel: e.target.value, model: e.target.value })}
                      placeholder="qwen2.5-coder:14b"
                    />
                    <small>
                      {settings.hardware?.recommendedModels?.[0] && (
                        <>Recommended for your hardware: {settings.hardware.recommendedModels[0]}</>
                      )}
                    </small>
                  </div>
                </>
              )}

              {settings.provider === 'openrouter' && (
                <>
                  <h4>OpenRouter Settings</h4>
                  <div className="form-group">
                    <label htmlFor="openrouter-key">API Key</label>
                    <input
                      type="password"
                      id="openrouter-key"
                      value={settings.openrouterApiKey}
                      onChange={(e) => setSettings({ ...settings, openrouterApiKey: e.target.value })}
                      placeholder="sk-or-..."
                    />
                    <small>You can also set OPENROUTER_API_KEY environment variable</small>
                  </div>
                  <div className="form-group">
                    <label htmlFor="openrouter-model">Model</label>
                    <input
                      type="text"
                      id="openrouter-model"
                      value={settings.openrouterModel}
                      onChange={(e) => setSettings({ ...settings, openrouterModel: e.target.value, model: e.target.value })}
                      placeholder="anthropic/claude-3.5-sonnet"
                    />
                    <small>100+ models available via OpenRouter</small>
                  </div>
                </>
              )}
            </div>
          )}

          {activeTab === 'deployment' && (
            <div className="settings-section">
              <h3>Deployment Configuration</h3>

              <h4>Vercel</h4>
              <div className="form-group">
                <label htmlFor="vercel-token">Vercel API Token</label>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <input
                    type="password"
                    id="vercel-token"
                    style={{ flex: 1 }}
                    value={settings.vercelToken}
                    onChange={(e) => {
                      setSettings({ ...settings, vercelToken: e.target.value });
                      setVercelTokenValid(null); // Reset validation
                    }}
                    placeholder="Enter your Vercel token"
                  />
                  <button
                    className="btn-secondary"
                    onClick={verifyVercelToken}
                    disabled={verifyingVercelToken || !settings.vercelToken}
                  >
                    {verifyingVercelToken ? 'Verifying...' : vercelTokenValid === true ? '✓ Valid' : vercelTokenValid === false ? '✗ Invalid' : 'Verify'}
                  </button>
                </div>
                <small>
                  Get your token from <a href="https://vercel.com/account/tokens" target="_blank" rel="noopener noreferrer">Vercel Dashboard → Settings → Tokens</a>
                </small>
                {vercelTokenValid === true && (
                  <div style={{ marginTop: '8px', color: 'var(--accent-success, #28a745)' }}>
                    ✓ Token verified successfully
                  </div>
                )}
              </div>

              <h4>Netlify</h4>
              <div className="form-group">
                <label htmlFor="netlify-token">Netlify Personal Access Token</label>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <input
                    type="password"
                    id="netlify-token"
                    style={{ flex: 1 }}
                    value={settings.netlifyToken || ''}
                    onChange={(e) => {
                      setSettings({ ...settings, netlifyToken: e.target.value });
                      setNetlifyTokenValid(null); // Reset validation
                    }}
                    placeholder="Enter your Netlify token"
                  />
                  <button
                    className="btn-secondary"
                    onClick={verifyNetlifyToken}
                    disabled={verifyingNetlifyToken || !settings.netlifyToken}
                  >
                    {verifyingNetlifyToken ? 'Verifying...' : netlifyTokenValid === true ? '✓ Valid' : netlifyTokenValid === false ? '✗ Invalid' : 'Verify'}
                  </button>
                </div>
                <small>
                  Get your token from <a href="https://app.netlify.com/user/applications#personal-access-tokens" target="_blank" rel="noopener noreferrer">Netlify → User Settings → Applications → Personal access tokens</a>
                </small>
                {netlifyTokenValid === true && (
                  <div style={{ marginTop: '8px', color: 'var(--accent-success, #28a745)' }}>
                    ✓ Token verified successfully
                  </div>
                )}
              </div>

              <div className="form-group">
                <h4>How to Deploy</h4>
                <ol style={{ marginLeft: '20px', lineHeight: '1.8' }}>
                  <li>Enter your API token above and click "Verify"</li>
                  <li>Save settings</li>
                  <li>Go to the Preview tab in Teddy</li>
                  <li>Click the "Deploy" button and choose your platform</li>
                  <li>Your project will be deployed automatically!</li>
                </ol>
              </div>

              <div className="form-group">
                <h4>Supported Project Types</h4>
                <ul style={{ marginLeft: '20px', lineHeight: '1.8' }}>
                  <li>Next.js applications</li>
                  <li>Vite + React projects</li>
                  <li>Static HTML/CSS/JS sites</li>
                  <li>Create React App projects</li>
                </ul>
              </div>
            </div>
          )}

          {activeTab === 'database' && (
            <div className="settings-section">
              <h3>Database & Services</h3>
              <p style={{ color: 'var(--text-secondary)', marginBottom: '16px' }}>
                Manage local database services for your projects. SQLite is the default and requires no setup.
                PostgreSQL is available for production-ready apps via Docker.
              </p>

              {/* Docker Status */}
              <div className="database-section">
                <div className="hardware-header">
                  <h4>Docker Status</h4>
                  <button className="btn-secondary" onClick={loadDockerStatus} disabled={dockerLoading}>
                    {dockerLoading ? 'Checking...' : '🔄 Refresh'}
                  </button>
                </div>

                {dockerStatus ? (
                  <div className="hardware-info">
                    <div className="info-row">
                      <span className="label">Installed:</span>
                      <span className="value">
                        {dockerStatus.installed ? '✅ Yes' : '❌ No'}
                      </span>
                    </div>
                    {dockerStatus.installed && (
                      <>
                        <div className="info-row">
                          <span className="label">Daemon Running:</span>
                          <span className="value">
                            {dockerStatus.daemonRunning ? '✅ Yes' : '❌ No'}
                          </span>
                        </div>
                        {dockerStatus.version && (
                          <div className="info-row">
                            <span className="label">Version:</span>
                            <span className="value">{dockerStatus.version}</span>
                          </div>
                        )}
                      </>
                    )}
                  </div>
                ) : dockerLoading ? (
                  <p>Checking Docker status...</p>
                ) : (
                  <p>Click refresh to check Docker status</p>
                )}

                {dockerInstructions && (
                  <div className="docker-instructions" style={{
                    marginTop: '12px',
                    padding: '12px',
                    backgroundColor: 'var(--bg-tertiary, #2d2d2d)',
                    borderRadius: '8px',
                    whiteSpace: 'pre-wrap',
                    fontFamily: 'monospace',
                    fontSize: '12px',
                    maxHeight: '200px',
                    overflow: 'auto',
                  }}>
                    {dockerInstructions}
                  </div>
                )}
              </div>

              {/* PostgreSQL Service */}
              <div className="database-section" style={{ marginTop: '24px' }}>
                <div className="hardware-header">
                  <h4>PostgreSQL Service</h4>
                  <button
                    className="btn-secondary"
                    onClick={loadPostgresStatus}
                    disabled={postgresLoading}
                  >
                    🔄 Refresh
                  </button>
                </div>

                {!dockerStatus?.installed || !dockerStatus?.daemonRunning ? (
                  <div style={{ padding: '16px', backgroundColor: 'var(--bg-warning, #3d3500)', borderRadius: '8px' }}>
                    <p style={{ margin: 0 }}>
                      ⚠️ Docker must be installed and running to use PostgreSQL.
                      {!dockerStatus?.installed && ' Please install Docker first.'}
                      {dockerStatus?.installed && !dockerStatus?.daemonRunning && ' Please start Docker.'}
                    </p>
                  </div>
                ) : (
                  <>
                    {postgresStatus ? (
                      <div className="hardware-info">
                        <div className="info-row">
                          <span className="label">Status:</span>
                          <span className="value">
                            {postgresStatus.running ? (
                              <span style={{ color: 'var(--accent-success, #28a745)' }}>● Running</span>
                            ) : postgresStatus.installed ? (
                              <span style={{ color: 'var(--text-warning, #ffc107)' }}>○ Stopped</span>
                            ) : (
                              <span style={{ color: 'var(--text-secondary)' }}>○ Not installed</span>
                            )}
                          </span>
                        </div>

                        {postgresStatus.running && postgresStatus.databaseUrl && (
                          <>
                            <div className="info-row">
                              <span className="label">Port:</span>
                              <span className="value">{postgresStatus.port}</span>
                            </div>
                            <div className="info-row">
                              <span className="label">DATABASE_URL:</span>
                              <span className="value" style={{ fontFamily: 'monospace', fontSize: '11px' }}>
                                {postgresStatus.databaseUrl.replace(/:[^:@]+@/, ':****@')}
                              </span>
                            </div>
                          </>
                        )}

                        <div style={{ display: 'flex', gap: '8px', marginTop: '16px', flexWrap: 'wrap' }}>
                          {postgresStatus.running ? (
                            <>
                              <button
                                className="btn-secondary"
                                onClick={stopPostgres}
                                disabled={postgresLoading}
                              >
                                {postgresLoading ? 'Stopping...' : '⏹ Stop'}
                              </button>
                              <button
                                className="btn-secondary"
                                onClick={testPostgresConnection}
                              >
                                🔌 Test Connection
                              </button>
                              <button
                                className="btn-secondary"
                                onClick={copyDatabaseUrl}
                              >
                                📋 Copy URL
                              </button>
                              <button
                                className="btn-secondary"
                                onClick={loadPostgresLogs}
                              >
                                📜 View Logs
                              </button>
                            </>
                          ) : (
                            <button
                              className="btn-primary"
                              onClick={startPostgres}
                              disabled={postgresLoading}
                            >
                              {postgresLoading ? 'Starting...' : '▶ Start PostgreSQL'}
                            </button>
                          )}
                        </div>
                      </div>
                    ) : (
                      <p>Loading PostgreSQL status...</p>
                    )}
                  </>
                )}

                {/* Logs Modal */}
                {showLogs && postgresLogs && (
                  <div style={{ marginTop: '16px' }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <h5>Container Logs</h5>
                      <button className="btn-secondary" onClick={() => setShowLogs(false)}>
                        Close
                      </button>
                    </div>
                    <pre style={{
                      padding: '12px',
                      backgroundColor: 'var(--bg-tertiary, #1a1a1a)',
                      borderRadius: '8px',
                      maxHeight: '200px',
                      overflow: 'auto',
                      fontSize: '11px',
                      whiteSpace: 'pre-wrap',
                    }}>
                      {postgresLogs}
                    </pre>
                  </div>
                )}
              </div>

              {/* Usage Guide */}
              <div className="database-section" style={{ marginTop: '24px' }}>
                <h4>Using PostgreSQL in Your Project</h4>
                <ol style={{ marginLeft: '20px', lineHeight: '1.8' }}>
                  <li>Start the PostgreSQL container above</li>
                  <li>Copy the DATABASE_URL</li>
                  <li>Add it to your project's <code>.env</code> file</li>
                  <li>Run <code>npx prisma migrate dev</code> to initialize your database</li>
                  <li>Use Ted's database tools: <code>database_init</code>, <code>database_migrate</code>, <code>database_query</code></li>
                </ol>

                <div style={{ marginTop: '16px', padding: '12px', backgroundColor: 'var(--bg-tertiary, #2d2d2d)', borderRadius: '8px' }}>
                  <strong>Note:</strong> Data is persisted in <code>~/.teddy/docker/postgres-data/</code>.
                  Stopping or removing the container will not delete your data.
                </div>
              </div>
            </div>
          )}

          {activeTab === 'hardware' && (
            <div className="settings-section">
              <div className="hardware-header">
                <h3>Hardware Profile</h3>
                <button className="btn-secondary" onClick={detectHardware}>
                  🔄 Re-detect
                </button>
              </div>

              {settings.hardware ? (
                <div className="hardware-info">
                  <div className="info-row">
                    <span className="label">Tier:</span>
                    <span className="value">
                      <strong>{settings.hardware.tier}</strong> ({settings.hardware.tierDescription})
                    </span>
                  </div>

                  <div className="info-row">
                    <span className="label">CPU:</span>
                    <span className="value">
                      {settings.hardware.cpuBrand} ({settings.hardware.cpuCores} cores)
                    </span>
                  </div>

                  <div className="info-row">
                    <span className="label">RAM:</span>
                    <span className="value">
                      {settings.hardware.ramGb}GB
                      {settings.hardware.ramGb < 16 && ' ⚠️'}
                    </span>
                  </div>

                  <div className="info-row">
                    <span className="label">Storage:</span>
                    <span className="value">
                      {settings.hardware.hasSsd ? 'SSD ✓' : 'HDD (consider upgrading) ⚠️'}
                    </span>
                  </div>

                  <div className="info-row">
                    <span className="label">Architecture:</span>
                    <span className="value">{settings.hardware.architecture}</span>
                  </div>

                  {settings.hardware.cpuYear && (
                    <div className="info-row">
                      <span className="label">CPU Generation:</span>
                      <span className="value">~{settings.hardware.cpuYear}</span>
                    </div>
                  )}

                  <h4>What You Can Build</h4>
                  <ul className="capabilities-list">
                    {settings.hardware.capabilities.map((cap, i) => (
                      <li key={i} className="capability">✓ {cap}</li>
                    ))}
                  </ul>

                  {settings.hardware.limitations.length > 0 && (
                    <>
                      <h4>Limitations</h4>
                      <ul className="capabilities-list">
                        {settings.hardware.limitations.map((lim, i) => (
                          <li key={i} className="limitation">✗ {lim}</li>
                        ))}
                      </ul>
                    </>
                  )}

                  <h4>Recommended Models</h4>
                  <ul className="models-list">
                    {settings.hardware.recommendedModels.slice(0, 3).map((model, i) => (
                      <li key={i}>• {model}</li>
                    ))}
                  </ul>

                  <h4>Expected Performance</h4>
                  <div className="info-row">
                    <span className="label">AI Response Time:</span>
                    <span className="value">
                      {settings.hardware.expectedResponseTime[0]}-{settings.hardware.expectedResponseTime[1]} seconds
                    </span>
                  </div>
                </div>
              ) : (
                <div className="hardware-empty">
                  <p>Hardware information not available.</p>
                  <button onClick={detectHardware}>Detect Hardware</button>
                </div>
              )}
            </div>
          )}
        </div>

        <div className="settings-footer">
          <button className="btn-secondary" onClick={onClose}>
            Cancel
          </button>
          <button className="btn-primary" onClick={saveSettings} disabled={saving}>
            {saving ? 'Saving...' : 'Save Settings'}
          </button>
        </div>
      </div>
    </div>
  );
}
