// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect } from 'react';
import './FileTree.css';

interface FileEntry {
  name: string;
  path: string;
  isDirectory: boolean;
  isExpanded?: boolean;
  children?: FileEntry[];
}

interface FileTreeProps {
  projectPath: string;
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
}

export function FileTree({ projectPath, selectedFile, onFileSelect }: FileTreeProps) {
  const [root, setRoot] = useState<FileEntry[]>([]);
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set(['.']));

  useEffect(() => {
    loadDirectory('.');
  }, [projectPath]);

  // Refresh file tree when Ted creates/modifies files
  useEffect(() => {
    const cleanup = window.teddy?.onFileChanged?.((info) => {
      // Refresh the file tree when files change
      loadDirectory('.');
    });

    return () => {
      cleanup?.();
    };
  }, [projectPath]);

  const loadDirectory = async (dirPath: string) => {
    try {
      const entries = await window.teddy.listFiles(dirPath);

      // Filter out common hidden/build directories
      const filtered = entries.filter(e => {
        const name = e.name;
        return !name.startsWith('.') &&
               name !== 'node_modules' &&
               name !== 'target' &&
               name !== 'dist' &&
               name !== 'build';
      });

      // Sort: directories first, then files alphabetically
      filtered.sort((a, b) => {
        if (a.isDirectory && !b.isDirectory) return -1;
        if (!a.isDirectory && b.isDirectory) return 1;
        return a.name.localeCompare(b.name);
      });

      if (dirPath === '.') {
        setRoot(filtered);
      }
    } catch (err) {
      console.error('Failed to load directory:', err);
    }
  };

  const handleToggle = (entry: FileEntry) => {
    if (entry.isDirectory) {
      const newExpanded = new Set(expandedDirs);
      if (newExpanded.has(entry.path)) {
        newExpanded.delete(entry.path);
      } else {
        newExpanded.add(entry.path);
        loadDirectory(entry.path);
      }
      setExpandedDirs(newExpanded);
    } else {
      onFileSelect(entry.path);
    }
  };

  const renderEntry = (entry: FileEntry, depth: number = 0) => {
    const isExpanded = expandedDirs.has(entry.path);
    const isSelected = selectedFile === entry.path;

    return (
      <div key={entry.path} className="tree-entry">
        <div
          className={`tree-item ${isSelected ? 'selected' : ''}`}
          style={{ paddingLeft: `${depth * 16 + 8}px` }}
          onClick={() => handleToggle(entry)}
        >
          {entry.isDirectory && (
            <span className="tree-icon">{isExpanded ? '▼' : '▶'}</span>
          )}
          {!entry.isDirectory && <span className="tree-icon">📄</span>}
          <span className="tree-name">{entry.name}</span>
        </div>
      </div>
    );
  };

  return (
    <div className="file-tree">
      <div className="tree-header">
        <span className="tree-title">Files</span>
      </div>
      <div className="tree-content">
        {root.map(entry => renderEntry(entry))}
      </div>
    </div>
  );
}
