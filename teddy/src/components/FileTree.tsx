// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useCallback, useEffect, useState } from 'react';
import './FileTree.css';

interface FileEntry {
  name: string;
  path: string;
  isDirectory: boolean;
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
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<FileEntry[]>([]);
  const [isSearching, setIsSearching] = useState(false);

  const filterAndSortEntries = useCallback((entries: FileEntry[]): FileEntry[] => {
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

    return filtered;
  }, []);

  const setChildrenAtPath = useCallback(
    (entries: FileEntry[], dirPath: string, children: FileEntry[]): FileEntry[] =>
      entries.map(entry => {
        if (entry.path === dirPath) {
          return { ...entry, children };
        }
        if (!entry.children || entry.children.length === 0) {
          return entry;
        }
        return {
          ...entry,
          children: setChildrenAtPath(entry.children, dirPath, children),
        };
      }),
    []
  );

  const loadDirectory = useCallback(async (dirPath: string) => {
    try {
      const entries = await window.teddy.listFiles(dirPath);
      const normalized = filterAndSortEntries(entries);

      if (dirPath === '.') {
        setRoot(normalized);
      } else {
        setRoot(prev => setChildrenAtPath(prev, dirPath, normalized));
      }
    } catch (err) {
      console.error('Failed to load directory:', err);
    }
  }, [filterAndSortEntries, setChildrenAtPath]);

  useEffect(() => {
    setExpandedDirs(new Set(['.']));
    setSearchQuery('');
    setSearchResults([]);
    void loadDirectory('.');
  }, [projectPath, loadDirectory]);

  // Refresh file tree when Ted creates/modifies files
  useEffect(() => {
    const cleanup = window.teddy?.onFileChanged?.((_info) => {
      // Refresh the file tree when files change
      void loadDirectory('.');
    });

    return () => {
      cleanup?.();
    };
  }, [projectPath, loadDirectory]);

  const normalizedSearch = searchQuery.trim().toLowerCase();
  const isSearchMode = normalizedSearch.length > 0;

  // Global search is backed by Electron IPC so results are not limited to loaded/expanded nodes.
  useEffect(() => {
    let cancelled = false;
    if (!isSearchMode) {
      setSearchResults([]);
      setIsSearching(false);
      return;
    }

    setIsSearching(true);
    const timeoutId = window.setTimeout(() => {
      window.teddy.searchFiles(normalizedSearch, 200)
        .then((results) => {
          if (cancelled) {
            return;
          }
          setSearchResults(results);
        })
        .catch((err) => {
          if (cancelled) {
            return;
          }
          console.error('Failed to search files:', err);
          setSearchResults([]);
        })
        .finally(() => {
          if (!cancelled) {
            setIsSearching(false);
          }
        });
    }, 120);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [isSearchMode, normalizedSearch, projectPath]);

  const expandToPath = useCallback(async (targetPath: string, isDirectory: boolean) => {
    const segments = targetPath.split('/').filter(Boolean);
    const depth = isDirectory ? segments.length : Math.max(segments.length - 1, 0);

    const nextExpanded = new Set<string>(['.']);
    const dirsToLoad: string[] = [];
    let current = '.';

    for (let i = 0; i < depth; i += 1) {
      current = current === '.' ? segments[i] : `${current}/${segments[i]}`;
      nextExpanded.add(current);
      dirsToLoad.push(current);
    }

    setExpandedDirs(nextExpanded);
    await loadDirectory('.');
    for (const dir of dirsToLoad) {
      await loadDirectory(dir);
    }
  }, [loadDirectory]);

  const handleSearchResultSelect = useCallback((entry: FileEntry) => {
    const applySelection = async () => {
      await expandToPath(entry.path, entry.isDirectory);
      setSearchQuery('');
      if (!entry.isDirectory) {
        onFileSelect(entry.path);
      }
    };
    void applySelection();
  }, [expandToPath, onFileSelect]);

  const handleToggle = (entry: FileEntry) => {
    if (entry.isDirectory) {
      const shouldExpand = !expandedDirs.has(entry.path);
      setExpandedDirs(prev => {
        const next = new Set(prev);
        if (next.has(entry.path)) {
          next.delete(entry.path);
        } else {
          next.add(entry.path);
        }
        return next;
      });

      if (shouldExpand) {
        void loadDirectory(entry.path);
      }
    } else {
      onFileSelect(entry.path);
    }
  };

  const renderEntry = (entry: FileEntry, depth: number = 0) => {
    const isExpanded = expandedDirs.has(entry.path);
    const isSelected = selectedFile === entry.path;
    const hasChildren = !!entry.children && entry.children.length > 0;
    const showChildren = entry.isDirectory && hasChildren && isExpanded;
    const folderIcon = showChildren ? '▼' : '▶';

    return (
      <div key={entry.path} className="tree-entry">
        <div
          className={`tree-item ${isSelected ? 'selected' : ''}`}
          style={{ paddingLeft: `${depth * 16 + 8}px` }}
          onClick={() => handleToggle(entry)}
        >
          {entry.isDirectory && (
            <span className="tree-icon">{folderIcon}</span>
          )}
          {!entry.isDirectory && <span className="tree-icon">📄</span>}
          <span className="tree-name">{entry.name}</span>
        </div>
        {showChildren && entry.children?.map(child => renderEntry(child, depth + 1))}
      </div>
    );
  };

  const renderSearchResult = (entry: FileEntry) => {
    const isSelected = selectedFile === entry.path;
    return (
      <div key={entry.path} className="tree-entry">
        <div
          className={`tree-item ${isSelected ? 'selected' : ''}`}
          onClick={() => handleSearchResultSelect(entry)}
          title={entry.path}
        >
          <span className="tree-icon">{entry.isDirectory ? '📁' : '📄'}</span>
          <div className="tree-search-result">
            <span className="tree-name">{entry.name}</span>
            <span className="tree-path">{entry.path}</span>
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="file-tree">
      <div className="tree-header">
        <span className="tree-title">Files</span>
        <input
          className="tree-search"
          type="text"
          placeholder="Search files..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
      </div>
      <div className="tree-content">
        {isSearchMode && isSearching && (
          <div className="tree-empty">Searching...</div>
        )}

        {isSearchMode && !isSearching && searchResults.length === 0 && (
          <div className="tree-empty">No matching files</div>
        )}

        {!isSearchMode && root.length === 0 && (
          <div className="tree-empty">
            No files found
          </div>
        )}

        {isSearchMode && !isSearching && searchResults.map(entry => renderSearchResult(entry))}
        {!isSearchMode && root.map(entry => renderEntry(entry))}
      </div>
    </div>
  );
}
