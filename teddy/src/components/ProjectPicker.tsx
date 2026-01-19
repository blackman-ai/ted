// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect } from 'react';
import './ProjectPicker.css';

interface RecentProject {
  path: string;
  name: string;
  lastOpened: number;
}

interface ProjectPickerProps {
  onProjectSelected: (path: string) => void;
}

export function ProjectPicker({ onProjectSelected }: ProjectPickerProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [isAutoLoading, setIsAutoLoading] = useState(true);
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);

  // On mount, check for last project and auto-load
  useEffect(() => {
    const init = async () => {
      try {
        // First, try to auto-load the last opened project
        const lastProject = await window.teddy.getLastProject();
        if (lastProject) {
          // Auto-load last project
          onProjectSelected(lastProject.path);
          return;
        }

        // No last project, load recent projects list
        setIsAutoLoading(false);
        const recent = await window.teddy.getRecentProjects();
        setRecentProjects(recent);
      } catch (err) {
        console.error('Failed to initialize project picker:', err);
        setIsAutoLoading(false);
      }
    };

    init();
  }, [onProjectSelected]);

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

  const handleSelectRecent = (projectPath: string) => {
    onProjectSelected(projectPath);
  };

  const handleRemoveRecent = async (e: React.MouseEvent, projectPath: string) => {
    e.stopPropagation();
    try {
      await window.teddy.removeRecentProject(projectPath);
      setRecentProjects(prev => prev.filter(p => p.path !== projectPath));
    } catch (err) {
      console.error('Failed to remove recent project:', err);
    }
  };

  const formatDate = (timestamp: number): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const diffDays = Math.floor((now.getTime() - date.getTime()) / (1000 * 60 * 60 * 24));

    if (diffDays === 0) return 'Today';
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return `${diffDays} days ago`;
    return date.toLocaleDateString();
  };

  // Show loading spinner during auto-load check
  if (isAutoLoading) {
    return (
      <div className="project-picker">
        <div className="picker-content">
          <div className="picker-logo">
            <div className="logo-icon">🧸</div>
            <h1>Teddy</h1>
            <p>Loading...</p>
          </div>
        </div>
      </div>
    );
  }

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
        </div>

        {recentProjects.length > 0 && (
          <div className="recent-projects">
            <h3>Recent Projects</h3>
            <ul className="recent-list">
              {recentProjects.map((project) => (
                <li
                  key={project.path}
                  className="recent-item"
                  onClick={() => handleSelectRecent(project.path)}
                >
                  <div className="recent-info">
                    <span className="recent-name">{project.name}</span>
                    <span className="recent-path" title={project.path}>
                      {project.path}
                    </span>
                    <span className="recent-date">{formatDate(project.lastOpened)}</span>
                  </div>
                  <button
                    className="recent-remove"
                    onClick={(e) => handleRemoveRecent(e, project.path)}
                    title="Remove from recent"
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}

        <div className="picker-footer">
          <p>Powered by Ted • Offline by default • Open source</p>
        </div>
      </div>
    </div>
  );
}
