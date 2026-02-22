// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect, useCallback, useRef } from 'react';
import { FileTree } from './components/FileTree';
import { Editor } from './components/Editor';
import { ChatPanel } from './components/ChatPanel';
import { Console } from './components/Console';
import { Preview, stopPreviewServer } from './components/Preview';
import { ProjectPicker } from './components/ProjectPicker';
import { DiffViewer } from './components/DiffViewer';
import { Settings } from './components/Settings';
import { SessionManager, Session } from './components/SessionManager';
import { Memory } from './components/Memory';
import { useTed } from './hooks/useTed';
import { useProject } from './hooks/useProject';
import { usePendingChanges } from './hooks/usePendingChanges';
import { debugLog } from './utils/logger';
import './App.css';

type EditorTab = 'editor' | 'preview' | 'review';
type RightPanelTab = 'chat' | 'memory';
type SettingsTab = 'providers' | 'deployment' | 'database' | 'hardware';
type NotificationLevel = 'info' | 'success' | 'error';

interface AppNotification {
  id: number;
  level: NotificationLevel;
  message: string;
}

function App() {
  const { project, setProject } = useProject();
  const { sendPrompt, stop, events, isRunning, logs, clearEvents } = useTed();
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<EditorTab>('editor');
  const [fileTreeKey, setFileTreeKey] = useState(0);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab>('providers');
  const [currentSession, setCurrentSession] = useState<Session | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [rightPanelTab, setRightPanelTab] = useState<RightPanelTab>('chat');
  const [editorReloadToken, setEditorReloadToken] = useState(0);
  const [chatFocusToken, setChatFocusToken] = useState(0);
  const [notifications, setNotifications] = useState<AppNotification[]>([]);
  const [showShortcuts, setShowShortcuts] = useState(false);

  // Track which events we've already processed
  const processedEventsRef = useRef<Set<string>>(new Set());
  const notificationIdRef = useRef(0);

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
    debugLog('[APP] useEffect triggered - reviewMode:', reviewMode, 'project:', !!project, 'events count:', events.length);

    if (!reviewMode || !project) {
      debugLog('[APP] Skipping event processing - reviewMode:', reviewMode, 'hasProject:', !!project);
      return;
    }

    const processEvents = async () => {
      debugLog('[APP] processEvents called with', events.length, 'events');
      for (const event of events) {
        // Create a unique key for this event
        const eventKey = `${event.type}-${event.timestamp}-${JSON.stringify(event.data).substring(0, 100)}`;
        debugLog('[APP] Processing event:', event.type, 'key:', eventKey.substring(0, 50));

        // Skip if already processed
        if (processedEventsRef.current.has(eventKey)) {
          debugLog('[APP] Event already processed, skipping');
          continue;
        }

        if (event.type === 'file_create') {
          const data = event.data as { path: string; content: string };
          debugLog('[APP] Processing file_create event:', data.path, 'content length:', data.content?.length || 0);

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
          debugLog('[APP] Processing file_edit event:', data.path, 'full data:', JSON.stringify(data));

          // Mark as processed
          processedEventsRef.current.add(eventKey);

          // For edits, we need to read the current file content (if it exists)
          let originalContent = '';
          try {
            const result = await window.teddy.readFile(data.path);
            debugLog('[APP] Read file for diff, content length:', result.content?.length || 0);
            originalContent = result.content || '';
          } catch (err) {
            // File doesn't exist yet - this is fine for new file creation
            debugLog('[APP] File does not exist yet:', data.path);
          }

          let newContent = originalContent;
          let editError: string | null = null;

          if (data.operation === 'replace' && data.new_text !== undefined) {
            // If old_text is empty string or undefined, it means replace entire file content
            if (data.old_text === '' || data.old_text === undefined) {
              debugLog('[APP] Applying full file replacement, new_text length:', data.new_text.length);
              newContent = data.new_text;
            } else {
              debugLog('[APP] Applying replace: old_text length:', data.old_text.length, 'new_text length:', data.new_text.length);
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
            debugLog('[APP] Not applying replace - operation:', data.operation, 'has old_text:', data.old_text !== undefined, 'has new_text:', data.new_text !== undefined);
          }

          const exists = pendingChanges.some(
            (c) => c.path === data.path && c.type === 'edit'
          );
          debugLog('[APP] Change already exists?', exists);
          if (!exists) {
            // Only add if there's an actual change OR if there's an error to report
            if (newContent !== originalContent || editError) {
              debugLog('[APP] Adding pending change for:', data.path, editError ? '(with error)' : '');
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
              debugLog('[APP] Skipping pending change - no actual change for:', data.path);
            }
          }
        } else if (event.type === 'file_delete') {
          const data = event.data as { path: string };
          debugLog('[APP] Processing file_delete event:', data.path);

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
      debugLog('[APP] External file change detected:', event.type, event.relativePath);

      // Refresh file tree when files are added/changed/deleted
      if (event.type === 'add' || event.type === 'unlink' || event.type === 'addDir' || event.type === 'unlinkDir') {
        debugLog('[APP] Refreshing file tree due to external change');
        setFileTreeKey((prev) => prev + 1);
      }

      // If the currently open file was modified externally, we could show a notification
      if (selectedFile && event.relativePath === selectedFile && event.type === 'change') {
        debugLog('[APP] Currently open file was modified externally');
        const shouldReload = window.confirm(
          `${selectedFile} changed outside Teddy. Reload the editor? Unsaved changes will be lost.`
        );
        if (shouldReload) {
          setEditorReloadToken((prev) => prev + 1);
        }
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

  const openSettingsTab = useCallback((tab: SettingsTab) => {
    setSettingsInitialTab(tab);
    setShowSettings(true);
  }, []);

  const pushNotification = useCallback((level: NotificationLevel, message: string) => {
    const id = notificationIdRef.current + 1;
    notificationIdRef.current = id;

    setNotifications((prev) => [...prev, { id, level, message }]);

    window.setTimeout(() => {
      setNotifications((prev) => prev.filter((n) => n.id !== id));
    }, 4200);
  }, []);

  const dismissNotification = useCallback((id: number) => {
    setNotifications((prev) => prev.filter((n) => n.id !== id));
  }, []);

  const startFreshChat = useCallback(async () => {
    clearEvents();
    await window.teddy.clearHistory();
    pushNotification('info', 'Started a fresh chat');
  }, [clearEvents, pushNotification]);

  const loadSessions = useCallback(async () => {
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
      pushNotification('error', 'Failed to load sessions');
    }
  }, [pushNotification]);

  // Load sessions when project changes
  useEffect(() => {
    if (project) {
      void loadSessions();
    }
  }, [project, loadSessions]);

  const handleNewSession = async () => {
    try {
      const result = await window.teddy.createSession();
      debugLog('Created new session:', result.id);

      // Clear events for new session
      clearEvents();
      await window.teddy.clearHistory();

      // Reload sessions
      await loadSessions();
      pushNotification('success', 'Created a new session');
    } catch (err) {
      console.error('Failed to create session:', err);
      pushNotification('error', 'Failed to create new session');
    }
  };

  const handleSessionSwitch = async (sessionId: string) => {
    try {
      const result = await window.teddy.switchSession(sessionId);
      debugLog('Switched to session:', result.id);

      // Clear current events
      clearEvents();

      // Reload sessions to update UI
      await loadSessions();
      pushNotification('success', 'Switched session');
    } catch (err) {
      console.error('Failed to switch session:', err);
      pushNotification('error', 'Failed to switch session');
    }
  };

  const handleDeleteSession = async (sessionId: string) => {
    try {
      await window.teddy.deleteSession(sessionId);
      debugLog('Deleted session:', sessionId);

      // Reload sessions to update UI
      await loadSessions();
      pushNotification('success', 'Deleted session');
    } catch (err) {
      console.error('Failed to delete session:', err);
      pushNotification('error', 'Failed to delete session');
    }
  };

  useEffect(() => {
    const isEditableTarget = (target: EventTarget | null): boolean => {
      if (!(target instanceof HTMLElement)) {
        return false;
      }
      const tag = target.tagName;
      return (
        target.isContentEditable ||
        tag === 'INPUT' ||
        tag === 'TEXTAREA' ||
        tag === 'SELECT'
      );
    };

    const handleShortcut = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      const hasModifier = event.metaKey || event.ctrlKey;
      const editable = isEditableTarget(event.target);

      if (event.key === 'Escape') {
        if (showSettings) {
          event.preventDefault();
          setShowSettings(false);
          return;
        }
        if (showShortcuts) {
          event.preventDefault();
          setShowShortcuts(false);
        }
        return;
      }

      if (!hasModifier) {
        return;
      }

      if (key === ',') {
        event.preventDefault();
        openSettingsTab('providers');
        return;
      }

      if (key === 'k') {
        event.preventDefault();
        setShowShortcuts((prev) => !prev);
        return;
      }

      if (editable) {
        return;
      }

      if (key === 'n') {
        event.preventDefault();
        void startFreshChat();
        return;
      }

      if (key === 'l') {
        event.preventDefault();
        setRightPanelTab('chat');
        setChatFocusToken((prev) => prev + 1);
        return;
      }

      if (key === '1') {
        event.preventDefault();
        setActiveTab('editor');
        return;
      }

      if (key === '2') {
        event.preventDefault();
        setActiveTab('preview');
        return;
      }

      if (key === '3') {
        event.preventDefault();
        setActiveTab('review');
        return;
      }

      if (key === '4') {
        event.preventDefault();
        setRightPanelTab('chat');
        return;
      }

      if (key === '5') {
        event.preventDefault();
        setRightPanelTab('memory');
      }
    };

    window.addEventListener('keydown', handleShortcut);
    return () => {
      window.removeEventListener('keydown', handleShortcut);
    };
  }, [openSettingsTab, showSettings, showShortcuts, startFreshChat]);

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
            onClick={() => void startFreshChat()}
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
            onClick={() => openSettingsTab('providers')}
            title="Settings"
          >
            ⚙️
          </button>
          <button
            className="btn-secondary"
            onClick={() => setShowShortcuts(true)}
            title="Keyboard shortcuts"
          >
            ⌨️
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
              <button
                className="btn-icon"
                title="Open Docker controls"
                onClick={() => openSettingsTab('database')}
              >
                🐳
              </button>
              <button
                className="btn-icon"
                title="Open PostgreSQL controls"
                onClick={() => openSettingsTab('database')}
              >
                🐘
              </button>
              <button
                className="btn-icon"
                title="Open deployment settings"
                onClick={() => openSettingsTab('deployment')}
              >
                🚀
              </button>
            </div>
          </div>

          {activeTab === 'editor' && (
            <Editor
              key={`${selectedFile ?? 'none'}-${editorReloadToken}`}
              projectPath={project.path}
              selectedFile={selectedFile}
              reloadToken={editorReloadToken}
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
            <button
              className={`tab ${rightPanelTab === 'chat' ? 'active' : ''}`}
              onClick={() => setRightPanelTab('chat')}
            >
              Chat
            </button>
            <button
              className={`tab ${rightPanelTab === 'memory' ? 'active' : ''}`}
              onClick={() => setRightPanelTab('memory')}
            >
              Memory
            </button>
          </div>
          {rightPanelTab === 'chat' ? (
            <ChatPanel
              onSendMessage={sendPrompt}
              onStop={stop}
              events={events}
              isRunning={isRunning}
              focusRequestToken={chatFocusToken}
            />
          ) : (
            <Memory projectPath={project.path} />
          )}
        </div>
      </div>

      <div className="bottom-panel">
        <Console logs={logs} />
      </div>

      {showSettings && (
        <Settings
          initialTab={settingsInitialTab}
          onClose={() => setShowSettings(false)}
        />
      )}

      {showShortcuts && (
        <div className="shortcuts-overlay" onClick={() => setShowShortcuts(false)}>
          <div className="shortcuts-modal" onClick={(e) => e.stopPropagation()}>
            <div className="shortcuts-header">
              <h3>Keyboard Shortcuts</h3>
              <button
                className="btn-secondary btn-small"
                onClick={() => setShowShortcuts(false)}
              >
                Close
              </button>
            </div>
            <div className="shortcuts-grid">
              <div><code>Cmd/Ctrl + N</code><span>New Chat</span></div>
              <div><code>Cmd/Ctrl + ,</code><span>Open Settings</span></div>
              <div><code>Cmd/Ctrl + K</code><span>Toggle Shortcuts</span></div>
              <div><code>Cmd/Ctrl + L</code><span>Focus Chat Input</span></div>
              <div><code>Cmd/Ctrl + 1</code><span>Editor Tab</span></div>
              <div><code>Cmd/Ctrl + 2</code><span>Preview Tab</span></div>
              <div><code>Cmd/Ctrl + 3</code><span>Review Tab</span></div>
              <div><code>Cmd/Ctrl + 4</code><span>Chat Panel</span></div>
              <div><code>Cmd/Ctrl + 5</code><span>Memory Panel</span></div>
              <div><code>Esc</code><span>Close Dialogs</span></div>
            </div>
          </div>
        </div>
      )}

      <div className="notification-stack">
        {notifications.map((notification) => (
          <div
            key={notification.id}
            className={`notification ${notification.level}`}
            onClick={() => dismissNotification(notification.id)}
          >
            {notification.message}
          </div>
        ))}
      </div>
    </div>
  );
}

export default App;
