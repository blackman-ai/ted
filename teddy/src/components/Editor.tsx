// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect, useCallback } from 'react';
import MonacoEditor from '@monaco-editor/react';
import './Editor.css';

interface EditorProps {
  projectPath: string;
  selectedFile: string | null;
  reloadToken?: number;
  onFileChange?: () => void;
}

export function Editor({
  projectPath: _projectPath,
  selectedFile,
  reloadToken = 0,
  onFileChange,
}: EditorProps) {
  const [content, setContent] = useState('');
  const [language, setLanguage] = useState('plaintext');
  const [isLoading, setIsLoading] = useState(false);

  const loadFile = useCallback(async (path: string) => {
    setIsLoading(true);
    try {
      const result = await window.teddy.readFile(path);
      setContent(result.content);
      setLanguage(getLanguageFromPath(path));
    } catch (err) {
      console.error('Failed to load file:', err);
      setContent(`// Error loading file: ${err}`);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (selectedFile) {
      loadFile(selectedFile);
    }
  }, [selectedFile, reloadToken, loadFile]);

  const handleSave = async () => {
    if (!selectedFile) return;

    try {
      await window.teddy.writeFile(selectedFile, content);
      onFileChange?.();
    } catch (err) {
      console.error('Failed to save file:', err);
    }
  };

  const getLanguageFromPath = (path: string): string => {
    const ext = path.split('.').pop()?.toLowerCase();
    const languageMap: Record<string, string> = {
      'js': 'javascript',
      'jsx': 'javascript',
      'ts': 'typescript',
      'tsx': 'typescript',
      'json': 'json',
      'html': 'html',
      'css': 'css',
      'scss': 'scss',
      'py': 'python',
      'rs': 'rust',
      'go': 'go',
      'java': 'java',
      'cpp': 'cpp',
      'c': 'c',
      'md': 'markdown',
      'yaml': 'yaml',
      'yml': 'yaml',
      'toml': 'toml',
      'sql': 'sql',
      'sh': 'shell',
      'bash': 'shell',
    };
    return languageMap[ext || ''] || 'plaintext';
  };

  if (!selectedFile) {
    return (
      <div className="editor-empty">
        <div className="empty-state">
          <p>No file selected</p>
          <p className="empty-hint">Select a file from the file tree to start editing</p>
        </div>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="editor-empty">
        <div className="empty-state">
          <p>Loading...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="editor-container">
      <div className="editor-header">
        <span className="editor-filename">{selectedFile}</span>
        <button className="btn-secondary btn-small" onClick={handleSave}>
          Save
        </button>
      </div>
      <div className="editor-content">
        <MonacoEditor
          height="100%"
          language={language}
          theme="vs-dark"
          value={content}
          onChange={(value) => setContent(value || '')}
          options={{
            minimap: { enabled: false },
            fontSize: 14,
            lineNumbers: 'on',
            roundedSelection: false,
            scrollBeyondLastLine: false,
            automaticLayout: true,
          }}
        />
      </div>
    </div>
  );
}
