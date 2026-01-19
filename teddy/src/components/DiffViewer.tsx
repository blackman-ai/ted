// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect, useMemo } from 'react';
import { DiffEditor } from '@monaco-editor/react';
import './DiffViewer.css';

export interface PendingChange {
  id: string;
  type: 'create' | 'edit' | 'delete';
  path: string;
  originalContent: string;
  newContent: string;
  operation?: 'replace' | 'insert' | 'delete';
  timestamp: number;
}

interface DiffViewerProps {
  projectPath: string;
  pendingChanges: PendingChange[];
  onAccept: (changeId: string) => void;
  onReject: (changeId: string) => void;
  onAcceptAll: () => void;
  onRejectAll: () => void;
}

export function DiffViewer({
  projectPath,
  pendingChanges,
  onAccept,
  onReject,
  onAcceptAll,
  onRejectAll,
}: DiffViewerProps) {
  const [selectedChangeId, setSelectedChangeId] = useState<string | null>(null);

  // Auto-select first change when list changes or when current selection becomes invalid
  useEffect(() => {
    if (pendingChanges.length === 0) {
      setSelectedChangeId(null);
      return;
    }

    // Check if current selection is still valid
    const currentSelectionValid = selectedChangeId &&
      pendingChanges.some((c) => c.id === selectedChangeId);

    if (!currentSelectionValid) {
      // Auto-select the first change
      setSelectedChangeId(pendingChanges[0].id);
    }
  }, [pendingChanges, selectedChangeId]);

  const selectedChange = useMemo(() => {
    if (!selectedChangeId) return null;
    return pendingChanges.find((c) => c.id === selectedChangeId) || null;
  }, [pendingChanges, selectedChangeId]);

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
      'prisma': 'plaintext',
    };
    return languageMap[ext || ''] || 'plaintext';
  };

  const getChangeTypeLabel = (change: PendingChange): string => {
    switch (change.type) {
      case 'create':
        return 'New File';
      case 'edit':
        return change.operation === 'replace' ? 'Modified' :
               change.operation === 'insert' ? 'Inserted' :
               change.operation === 'delete' ? 'Lines Deleted' : 'Modified';
      case 'delete':
        return 'Deleted';
      default:
        return 'Changed';
    }
  };

  const getChangeTypeClass = (change: PendingChange): string => {
    switch (change.type) {
      case 'create':
        return 'change-create';
      case 'edit':
        return 'change-edit';
      case 'delete':
        return 'change-delete';
      default:
        return '';
    }
  };

  if (pendingChanges.length === 0) {
    return (
      <div className="diff-viewer-empty">
        <div className="empty-state">
          <div className="empty-icon">✓</div>
          <p>No pending changes</p>
          <p className="empty-hint">
            When Ted modifies files, changes will appear here for review before being applied.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="diff-viewer">
      <div className="diff-header">
        <div className="diff-title">
          <span className="diff-count">{pendingChanges.length}</span>
          <span>pending {pendingChanges.length === 1 ? 'change' : 'changes'}</span>
        </div>
        <div className="diff-actions">
          <button
            className="btn-primary btn-small"
            onClick={onAcceptAll}
            title="Accept all changes"
          >
            Accept All
          </button>
          <button
            className="btn-danger btn-small"
            onClick={onRejectAll}
            title="Reject all changes"
          >
            Reject All
          </button>
        </div>
      </div>

      <div className="diff-content">
        <div className="diff-list">
          {pendingChanges.map((change) => (
            <div
              key={change.id}
              className={`diff-item ${selectedChangeId === change.id ? 'selected' : ''} ${getChangeTypeClass(change)}`}
              onClick={() => setSelectedChangeId(change.id)}
            >
              <div className="diff-item-info">
                <span className={`change-badge ${getChangeTypeClass(change)}`}>
                  {getChangeTypeLabel(change)}
                </span>
                <span className="diff-item-path" title={change.path}>
                  {change.path}
                </span>
              </div>
              <div className="diff-item-actions">
                <button
                  className="btn-icon btn-accept"
                  onClick={(e) => {
                    e.stopPropagation();
                    onAccept(change.id);
                  }}
                  title="Accept this change"
                >
                  ✓
                </button>
                <button
                  className="btn-icon btn-reject"
                  onClick={(e) => {
                    e.stopPropagation();
                    onReject(change.id);
                  }}
                  title="Reject this change"
                >
                  ✕
                </button>
              </div>
            </div>
          ))}
        </div>

        <div className="diff-editor-container">
          {selectedChange ? (
            <>
              <div className="diff-editor-header">
                <div className="diff-editor-title">
                  <span className={`change-badge ${getChangeTypeClass(selectedChange)}`}>
                    {getChangeTypeLabel(selectedChange)}
                  </span>
                  <span className="diff-editor-path">{selectedChange.path}</span>
                </div>
                <div className="diff-editor-actions">
                  <button
                    className="btn-primary btn-small"
                    onClick={() => onAccept(selectedChange.id)}
                  >
                    Accept Change
                  </button>
                  <button
                    className="btn-danger btn-small"
                    onClick={() => onReject(selectedChange.id)}
                  >
                    Reject Change
                  </button>
                </div>
              </div>
              <div className="diff-editor-content">
                {selectedChange.type === 'delete' ? (
                  <div className="diff-delete-preview">
                    <p>This file will be deleted:</p>
                    <pre>{selectedChange.originalContent}</pre>
                  </div>
                ) : (
                  <DiffEditor
                    height="100%"
                    language={getLanguageFromPath(selectedChange.path)}
                    theme="vs-dark"
                    original={selectedChange.originalContent}
                    modified={selectedChange.newContent}
                    options={{
                      readOnly: true,
                      minimap: { enabled: false },
                      fontSize: 13,
                      lineNumbers: 'on',
                      roundedSelection: false,
                      scrollBeyondLastLine: false,
                      automaticLayout: true,
                      renderSideBySide: true,
                      originalEditable: false,
                      renderOverviewRuler: false,
                    }}
                  />
                )}
              </div>
            </>
          ) : (
            <div className="diff-editor-empty">
              <p>Select a change from the list to view the diff</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
