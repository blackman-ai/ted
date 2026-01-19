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
  groupId?: string; // Optional group ID for atomic changes
  groupDescription?: string; // Description of the group
  relatedFiles?: string[]; // List of related file paths
}

interface DiffViewerProps {
  projectPath: string;
  pendingChanges: PendingChange[];
  onAccept: (changeId: string) => void;
  onReject: (changeId: string) => void;
  onAcceptAll: () => void;
  onRejectAll: () => void;
  onAcceptGroup?: (groupId: string) => void; // Optional: accept all changes in a group
  onRejectGroup?: (groupId: string) => void; // Optional: reject all changes in a group
}

export function DiffViewer({
  projectPath,
  pendingChanges,
  onAccept,
  onReject,
  onAcceptAll,
  onRejectAll,
  onAcceptGroup,
  onRejectGroup,
}: DiffViewerProps) {
  const [selectedChangeId, setSelectedChangeId] = useState<string | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [showRelatedFiles, setShowRelatedFiles] = useState(false);

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

  // Group changes by groupId
  const groupedChanges = useMemo(() => {
    const groups = new Map<string | null, PendingChange[]>();

    for (const change of pendingChanges) {
      const key = change.groupId || null;
      if (!groups.has(key)) {
        groups.set(key, []);
      }
      groups.get(key)!.push(change);
    }

    return groups;
  }, [pendingChanges]);

  const hasGroups = useMemo(() => {
    return Array.from(groupedChanges.keys()).some(key => key !== null);
  }, [groupedChanges]);

  const toggleGroup = (groupId: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev);
      if (next.has(groupId)) {
        next.delete(groupId);
      } else {
        next.add(groupId);
      }
      return next;
    });
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
          {Array.from(groupedChanges.entries()).map(([groupId, changes]) => {
            if (!groupId) {
              // Render ungrouped changes normally
              return changes.map((change) => (
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
              ));
            }

            // Render grouped changes
            const isExpanded = expandedGroups.has(groupId);
            const groupDescription = changes[0]?.groupDescription || `Group ${groupId.slice(0, 8)}`;

            return (
              <div key={groupId} className="diff-group">
                <div
                  className="diff-group-header"
                  onClick={() => toggleGroup(groupId)}
                >
                  <span className="diff-group-toggle">
                    {isExpanded ? '▼' : '▶'}
                  </span>
                  <div className="diff-group-info">
                    <span className="diff-group-title">{groupDescription}</span>
                    <span className="diff-group-count">
                      {changes.length} {changes.length === 1 ? 'file' : 'files'}
                    </span>
                  </div>
                  {onAcceptGroup && onRejectGroup && (
                    <div className="diff-group-actions">
                      <button
                        className="btn-icon btn-accept"
                        onClick={(e) => {
                          e.stopPropagation();
                          onAcceptGroup(groupId);
                        }}
                        title="Accept all changes in this group"
                      >
                        ✓
                      </button>
                      <button
                        className="btn-icon btn-reject"
                        onClick={(e) => {
                          e.stopPropagation();
                          onRejectGroup(groupId);
                        }}
                        title="Reject all changes in this group"
                      >
                        ✕
                      </button>
                    </div>
                  )}
                </div>
                {isExpanded && changes.map((change) => (
                  <div
                    key={change.id}
                    className={`diff-item diff-item-grouped ${selectedChangeId === change.id ? 'selected' : ''} ${getChangeTypeClass(change)}`}
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
            );
          })}
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
                  {selectedChange.relatedFiles && selectedChange.relatedFiles.length > 0 && (
                    <button
                      className="btn-related-files"
                      onClick={() => setShowRelatedFiles(!showRelatedFiles)}
                      title="View related files"
                    >
                      📎 {selectedChange.relatedFiles.length} related
                    </button>
                  )}
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
              {showRelatedFiles && selectedChange.relatedFiles && (
                <div className="related-files-panel">
                  <div className="related-files-header">Related Files:</div>
                  <ul className="related-files-list">
                    {selectedChange.relatedFiles.map((file, idx) => (
                      <li key={idx} className="related-file-item">
                        {file}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
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
