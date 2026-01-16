// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState } from 'react';
import './ProjectPicker.css';

interface ProjectPickerProps {
  onProjectSelected: (path: string) => void;
}

export function ProjectPicker({ onProjectSelected }: ProjectPickerProps) {
  const [isLoading, setIsLoading] = useState(false);

  const handleOpenFolder = async () => {
    setIsLoading(true);
    try {
      const path = await window.teddy.openFolderDialog();
      if (path) {
        onProjectSelected(path);
      }
    } catch (err) {
      console.error('Failed to open folder:', err);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="project-picker">
      <div className="picker-content">
        <div className="picker-logo">
          <div className="logo-icon">🧸</div>
          <h1>Teddy</h1>
          <p>Offline-first AI coding environment</p>
        </div>

        <div className="picker-actions">
          <button
            className="btn-primary btn-large"
            onClick={handleOpenFolder}
            disabled={isLoading}
          >
            {isLoading ? 'Opening...' : 'Open Project Folder'}
          </button>
          <p className="picker-hint">
            Select a folder to start coding with AI
          </p>
        </div>

        <div className="picker-footer">
          <p>Powered by Ted • Offline by default • Open source</p>
        </div>
      </div>
    </div>
  );
}
