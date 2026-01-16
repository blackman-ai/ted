// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState } from 'react';
import './Preview.css';

interface PreviewProps {
  projectPath: string;
}

export function Preview({ projectPath }: PreviewProps) {
  const [url, setUrl] = useState('http://localhost:5173');
  const [isRunning, setIsRunning] = useState(false);

  // In MVP, this is a simple iframe
  // Future: detect dev server, auto-start, etc.

  return (
    <div className="preview-container">
      <div className="preview-toolbar">
        <input
          type="text"
          className="preview-url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="Enter preview URL (e.g., http://localhost:3000)"
        />
        <button className="btn-secondary btn-small">
          {isRunning ? 'Stop Server' : 'Start Server'}
        </button>
        <button className="btn-secondary btn-small">↻ Refresh</button>
      </div>

      <div className="preview-content">
        {url ? (
          <iframe
            src={url}
            title="Preview"
            className="preview-frame"
            sandbox="allow-same-origin allow-scripts allow-forms"
          />
        ) : (
          <div className="preview-empty">
            <p>No preview available</p>
            <p className="empty-hint">
              Start a dev server and enter the URL above
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
