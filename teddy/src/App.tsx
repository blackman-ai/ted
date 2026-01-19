// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect, useCallback, useRef } from 'react';
import { FileTree } from './components/FileTree';
import { Editor } from './components/Editor';
import { ChatPanel } from './components/ChatPanel';
import { Console } from './components/Console';
import { Preview } from './components/Preview';
import { ProjectPicker } from './components/ProjectPicker';
import { DiffViewer, PendingChange } from './components/DiffViewer';
import { Settings } from './components/Settings';
import { useTed } from './hooks/useTed';
import { useProject } from './hooks/useProject';
import { usePendingChanges } from './hooks/usePendingChanges';
import './App.css';

type EditorTab = 'editor' | 'preview' | 'review';

function App() {
  const { project, setProject } = useProject();
  const { sendPrompt, stop, events, isRunning, logs, clearEvents } = useTed();
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<EditorTab>('editor');
  const [fileTreeKey, setFileTreeKey] = useState(0);
  const [showSettings, setShowSettings] = useState(false);

  // Track which events we've already processed
  const processedEventsRef = useRef<Set<string>>(new Set());

  const {
    pendingChanges,
    reviewMode,
    toggleReviewMode,
    addPendingChange,
    acceptChange,
    rejectChange,
    acceptAllChanges,
    rejectAllChanges,
    hasPendingChanges,
  } = usePendingChanges();

  // Process Ted events to collect pending changes when review mode is on
  useEffect(() => {
    console.log('[APP] useEffect triggered - reviewMode:', reviewMode, 'project:', !!project, 'events count:', events.length);

    if (!reviewMode || !project) {
      console.log('[APP] Skipping event processing - reviewMode:', reviewMode, 'hasProject:', !!project);
      return;
    }

    const processEvents = async () => {
      console.log('[APP] processEvents called with', events.length, 'events');
      for (const event of events) {
        // Create a unique key for this event
        const eventKey = `${event.type}-${event.timestamp}-${JSON.stringify(event.data).substring(0, 100)}`;
        console.log('[APP] Processing event:', event.type, 'key:', eventKey.substring(0, 50));

        // Skip if already processed
        if (processedEventsRef.current.has(eventKey)) {
          console.log('[APP] Event already processed, skipping');
          continue;
        }

        if (event.type === 'file_create') {
          const data = event.data as { path: string; content: string };
          console.log('[APP] Processing file_create event:', data.path, 'content length:', data.content?.length || 0);

          // Mark as processed
          processedEventsRef.current.add(eventKey);

          // Check if we already have this change pending
          const exists = pendingChanges.some(
            (c) => c.path === data.path && c.type === 'create'
          );
          if (!exists && data.content) {
            addPendingChange({
              type: 'create',
              path: data.path,
              originalContent: '',
              newContent: data.content,
              timestamp: event.timestamp,
            });
          }
        } else if (event.type === 'file_edit') {
          const data = event.data as {
            path: string;
            operation: 'replace' | 'insert' | 'delete';
            old_text?: string;
            new_text?: string;
          };
          console.log('[APP] Processing file_edit event:', data.path, 'full data:', JSON.stringify(data));

          // Mark as processed
          processedEventsRef.current.add(eventKey);

          // For edits, we need to read the current file content
          try {
            const result = await window.teddy.readFile(data.path);
            console.log('[APP] Read file for diff, content length:', result.content?.length || 0);
            let newContent = result.content;

            if (data.operation === 'replace' && data.old_text && data.new_text !== undefined) {
              console.log('[APP] Applying replace: old_text length:', data.old_text.length, 'new_text length:', data.new_text.length);
              newContent = result.content.replace(data.old_text, data.new_text);
            } else {
              console.log('[APP] Not applying replace - operation:', data.operation, 'has old_text:', !!data.old_text, 'has new_text:', data.new_text !== undefined);
            }

            const exists = pendingChanges.some(
              (c) => c.path === data.path && c.type === 'edit'
            );
            console.log('[APP] Change already exists?', exists);
            if (!exists) {
              console.log('[APP] Adding pending change for:', data.path);
              addPendingChange({
                type: 'edit',
                path: data.path,
                operation: data.operation,
                originalContent: result.content,
                newContent: newContent,
                timestamp: event.timestamp,
              });
            }
          } catch (err) {
            console.error('[APP] Failed to read file for diff:', err);
          }
        } else if (event.type === 'file_delete') {
          const data = event.data as { path: string };
          console.log('[APP] Processing file_delete event:', data.path);

          // Mark as processed
          processedEventsRef.current.add(eventKey);

          try {
            const result = await window.teddy.readFile(data.path);
            const exists = pendingChanges.some(
              (c) => c.path === data.path && c.type === 'delete'
            );
            if (!exists) {
              addPendingChange({
                type: 'delete',
                path: data.path,
                originalContent: result.content,
                newContent: '',
                timestamp: event.timestamp,
              });
            }
          } catch (err) {
            console.error('Failed to read file for diff:', err);
          }
        }
      }
    };

    processEvents();
  }, [events, reviewMode, project, addPendingChange, pendingChanges]);

  // Auto-switch to review tab when there are pending changes
  useEffect(() => {
    if (hasPendingChanges && activeTab !== 'review') {
      setActiveTab('review');
    }
  }, [hasPendingChanges, activeTab]);

  // Refresh file tree after accepting changes
  const handleAcceptChange = useCallback(async (changeId: string) => {
    const success = await acceptChange(changeId);
    if (success) {
      setFileTreeKey((prev) => prev + 1); // Force refresh
    }
  }, [acceptChange]);

  const handleAcceptAll = useCallback(async () => {
    const success = await acceptAllChanges();
    if (success) {
      setFileTreeKey((prev) => prev + 1); // Force refresh
    }
  }, [acceptAllChanges]);

  if (!project) {
    return <ProjectPicker onProjectSelected={setProject} />;
  }

  return (
    <div className="app">
      <div className="titlebar">
        <div className="titlebar-left">
          <span className="app-title">Teddy</span>
          <span className="project-name">{project.name}</span>
        </div>
        <div className="titlebar-right">
          <label className="review-toggle" title="Review changes before applying">
            <input
              type="checkbox"
              checked={reviewMode}
              onChange={() => toggleReviewMode()}
            />
            <span>Review Mode</span>
          </label>
          <button
            className="btn-secondary"
            onClick={async () => {
              clearEvents();
              await window.teddy.clearHistory();
            }}
            title="Start a fresh conversation"
          >
            New Chat
          </button>
          <button
            className="btn-secondary"
            onClick={() => setProject(null)}
          >
            Change Project
          </button>
          <button
            className="btn-secondary"
            onClick={() => setShowSettings(true)}
            title="Settings"
          >
            ⚙️
          </button>
        </div>
      </div>

      <div className="main-content">
        <div className="sidebar">
          <FileTree
            key={fileTreeKey}
            projectPath={project.path}
            selectedFile={selectedFile}
            onFileSelect={setSelectedFile}
          />
        </div>

        <div className="editor-area">
          <div className="editor-tabs">
            <button
              className={`tab ${activeTab === 'editor' ? 'active' : ''}`}
              onClick={() => setActiveTab('editor')}
            >
              Editor
            </button>
            <button
              className={`tab ${activeTab === 'preview' ? 'active' : ''}`}
              onClick={() => setActiveTab('preview')}
            >
              Preview
            </button>
            <button
              className={`tab ${activeTab === 'review' ? 'active' : ''} ${hasPendingChanges ? 'has-badge' : ''}`}
              onClick={() => setActiveTab('review')}
            >
              Review
              {hasPendingChanges && (
                <span className="tab-badge">{pendingChanges.length}</span>
              )}
            </button>
            <div className="tab-actions">
              <button className="btn-icon" title="Docker (Coming Soon)" disabled>
                🐳
              </button>
              <button className="btn-icon" title="PostgreSQL (Coming Soon)" disabled>
                🐘
              </button>
              <button className="btn-icon" title="Deploy (Coming Soon)" disabled>
                🚀
              </button>
            </div>
          </div>

          {activeTab === 'editor' && (
            <Editor
              projectPath={project.path}
              selectedFile={selectedFile}
              onFileChange={() => {
                setFileTreeKey((prev) => prev + 1);
              }}
            />
          )}

          {activeTab === 'preview' && (
            <Preview projectPath={project.path} />
          )}

          {activeTab === 'review' && (
            <DiffViewer
              projectPath={project.path}
              pendingChanges={pendingChanges}
              onAccept={handleAcceptChange}
              onReject={rejectChange}
              onAcceptAll={handleAcceptAll}
              onRejectAll={rejectAllChanges}
            />
          )}
        </div>

        <div className="right-panel">
          <div className="panel-tabs">
            <button className="tab active">Chat</button>
          </div>
          <ChatPanel
            onSendMessage={sendPrompt}
            onStop={stop}
            events={events}
            isRunning={isRunning}
          />
        </div>
      </div>

      <div className="bottom-panel">
        <Console logs={logs} />
      </div>

      {showSettings && (
        <Settings onClose={() => setShowSettings(false)} />
      )}
    </div>
  );
}

export default App;
