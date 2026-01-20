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

export function Settings({ onClose }: SettingsProps) {
  const [settings, setSettings] = useState<TedSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [activeTab, setActiveTab] = useState<'providers' | 'deployment' | 'hardware'>('providers');
  const [verifyingVercelToken, setVerifyingVercelToken] = useState(false);
  const [vercelTokenValid, setVercelTokenValid] = useState<boolean | null>(null);
  const [verifyingNetlifyToken, setVerifyingNetlifyToken] = useState(false);
  const [netlifyTokenValid, setNetlifyTokenValid] = useState<boolean | null>(null);

  useEffect(() => {
    loadSettings();
  }, []);

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
                  onChange={(e) => setSettings({ ...settings, provider: e.target.value })}
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
                      onChange={(e) => setSettings({ ...settings, anthropicModel: e.target.value })}
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
                      onChange={(e) => setSettings({ ...settings, ollamaModel: e.target.value })}
                      placeholder="qwen2.5-coder:14b"
                    />
                    <small>
                      {settings.hardware && (
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
                      onChange={(e) => setSettings({ ...settings, openrouterModel: e.target.value })}
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
