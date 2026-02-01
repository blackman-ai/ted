// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect, useCallback, useRef } from 'react';
import { FileTree } from './components/FileTree';
import { Editor } from './components/Editor';
import { ChatPanel } from './components/ChatPanel';
import { Console } from './components/Console';
import { Preview, stopPreviewServer } from './components/Preview';
import { ProjectPicker } from './components/ProjectPicker';
import { DiffViewer, PendingChange } from './components/DiffViewer';
import { Settings } from './components/Settings';
import { SessionManager, Session } from './components/SessionManager';
import { useTed } from './hooks/useTed';
import { useProject } from './hooks/useProject';
import { usePendingChanges } from './hooks/usePendingChanges';
import { useSession } from './hooks/useSession';
import './App.css';

type EditorTab = 'editor' | 'preview' | 'review';

function App() {
  const { project, setProject } = useProject();
  const { sendPrompt, stop, events, isRunning, logs, clearEvents } = useTed();
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<EditorTab>('editor');
  const [fileTreeKey, setFileTreeKey] = useState(0);
  const [showSettings, setShowSettings] = useState(false);
  const [currentSession, setCurrentSession] = useState<Session | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);

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

          // For edits, we need to read the current file content (if it exists)
          let originalContent = '';
          try {
            const result = await window.teddy.readFile(data.path);
            console.log('[APP] Read file for diff, content length:', result.content?.length || 0);
            originalContent = result.content || '';
          } catch (err) {
            // File doesn't exist yet - this is fine for new file creation
            console.log('[APP] File does not exist yet:', data.path);
          }

          let newContent = originalContent;
          let editError: string | null = null;

          if (data.operation === 'replace' && data.new_text !== undefined) {
            // If old_text is empty string or undefined, it means replace entire file content
            if (data.old_text === '' || data.old_text === undefined) {
              console.log('[APP] Applying full file replacement, new_text length:', data.new_text.length);
              newContent = data.new_text;
            } else {
              console.log('[APP] Applying replace: old_text length:', data.old_text.length, 'new_text length:', data.new_text.length);
              // Check if old_text exists in the file before replacing
              if (!originalContent.includes(data.old_text)) {
                // Generate helpful error message similar to file-applier.ts
                const firstLine = data.old_text.split('\n')[0].trim();
                const hasPartialMatch = firstLine && originalContent.includes(firstLine);
                editError = hasPartialMatch
                  ? `Found partial match for "${firstLine.substring(0, 50)}..." but full text differs. The file may have changed.`
                  : `The text to replace was not found in the file. The model may be hallucinating file content.`;
                console.error(`[APP] Edit failed for ${data.path}: ${editError}`);
                console.error('[APP] old_text was:', data.old_text.substring(0, 200));
              } else {
                newContent = originalContent.replace(data.old_text, data.new_text);
              }
            }
          } else {
            console.log('[APP] Not applying replace - operation:', data.operation, 'has old_text:', data.old_text !== undefined, 'has new_text:', data.new_text !== undefined);
          }

          const exists = pendingChanges.some(
            (c) => c.path === data.path && c.type === 'edit'
          );
          console.log('[APP] Change already exists?', exists);
          if (!exists) {
            // Only add if there's an actual change OR if there's an error to report
            if (newContent !== originalContent || editError) {
              console.log('[APP] Adding pending change for:', data.path, editError ? '(with error)' : '');
              addPendingChange({
                type: 'edit',
                path: data.path,
                operation: data.operation,
                originalContent: originalContent,
                newContent: newContent,
                timestamp: event.timestamp,
                error: editError || undefined,
              });
            } else {
              console.log('[APP] Skipping pending change - no actual change for:', data.path);
            }
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

  // Listen for external file changes (from file watcher)
  useEffect(() => {
    const unsubscribe = window.teddy.onFileExternalChange((event) => {
      console.log('[APP] External file change detected:', event.type, event.relativePath);

      // Refresh file tree when files are added/changed/deleted
      if (event.type === 'add' || event.type === 'unlink' || event.type === 'addDir' || event.type === 'unlinkDir') {
        console.log('[APP] Refreshing file tree due to external change');
        setFileTreeKey((prev) => prev + 1);
      }

      // If the currently open file was modified externally, we could show a notification
      if (selectedFile && event.relativePath === selectedFile && event.type === 'change') {
        console.log('[APP] Currently open file was modified externally');
        // TODO: Show notification or auto-reload option
      }
    });

    return () => unsubscribe();
  }, [selectedFile]);

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
      setActiveTab('preview'); // Switch to preview after accepting changes
    }
  }, [acceptAllChanges]);

  // Load sessions when project changes
  useEffect(() => {
    if (project) {
      loadSessions();
    }
  }, [project]);

  const loadSessions = async () => {
    try {
      const sessionList = await window.teddy.listSessions();
      const sessionInfos: Session[] = sessionList.map(s => ({
        id: s.id,
        name: s.name,
        lastActive: new Date(s.lastActive).toISOString(),
        messageCount: s.messageCount,
        summary: s.summary,
        isActive: false,
      }));

      // Get current session
      const current = await window.teddy.getCurrentSession();
      if (current) {
        const currentSessionInfo: Session = {
          id: current.id,
          name: current.name,
          lastActive: new Date(current.lastActive).toISOString(),
          messageCount: current.messageCount,
          summary: current.summary,
          isActive: true,
        };
        setCurrentSession(currentSessionInfo);

        // Mark as active in list
        const updated = sessionInfos.map(s => ({
          ...s,
          isActive: s.id === current.id,
        }));
        setSessions(updated);
      } else {
        setSessions(sessionInfos);
      }
    } catch (err) {
      console.error('Failed to load sessions:', err);
    }
  };

  const handleNewSession = async () => {
    try {
      const result = await window.teddy.createSession();
      console.log('Created new session:', result.id);

      // Clear events for new session
      clearEvents();
      await window.teddy.clearHistory();

      // Reload sessions
      await loadSessions();
    } catch (err) {
      console.error('Failed to create session:', err);
    }
  };

  const handleSessionSwitch = async (sessionId: string) => {
    try {
      const result = await window.teddy.switchSession(sessionId);
      console.log('Switched to session:', result.id);

      // Clear current events
      clearEvents();

      // Reload sessions to update UI
      await loadSessions();
    } catch (err) {
      console.error('Failed to switch session:', err);
    }
  };

  const handleDeleteSession = async (sessionId: string) => {
    try {
      await window.teddy.deleteSession(sessionId);
      console.log('Deleted session:', sessionId);

      // Reload sessions to update UI
      await loadSessions();
    } catch (err) {
      console.error('Failed to delete session:', err);
    }
  };

  if (!project) {
    return <ProjectPicker onProjectSelected={setProject} />;
  }

  return (
    <div className="app">
      <div className="titlebar">
        <div className="titlebar-left">
          <span className="app-title">Teddy</span>
          <span className="project-name">{project.name}</span>
          <SessionManager
            currentSession={currentSession}
            sessions={sessions}
            onSessionSwitch={handleSessionSwitch}
            onNewSession={handleNewSession}
            onDeleteSession={handleDeleteSession}
          />
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
            onClick={async () => {
              // Stop any running preview server before changing projects
              await stopPreviewServer();
              await window.teddy.clearLastProject();
              setProject(null);
            }}
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
            <Preview projectPath={project.path} key={fileTreeKey} />
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
