// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState } from 'react';
import './SessionManager.css';

export interface Session {
  id: string;
  name: string;
  lastActive: string;
  messageCount: number;
  summary?: string;
  isActive: boolean;
}

interface SessionManagerProps {
  currentSession: Session | null;
  sessions: Session[];
  isLoading?: boolean;
  onSessionSwitch: (sessionId: string) => void;
  onNewSession: () => void;
  onDeleteSession?: (sessionId: string) => void;
}

export function SessionManager({
  currentSession,
  sessions,
  isLoading = false,
  onSessionSwitch,
  onNewSession,
  onDeleteSession,
}: SessionManagerProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  const handleSessionClick = (sessionId: string) => {
    if (currentSession?.id !== sessionId) {
      onSessionSwitch(sessionId);
      setIsExpanded(false);
    }
  };

  const handleNewSession = () => {
    onNewSession();
    setIsExpanded(false);
  };

  const handleDeleteSession = (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    if (onDeleteSession) {
      onDeleteSession(sessionId);
    }
  };

  const formatTimestamp = (timestamp: string): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / (1000 * 60));
    const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

    if (diffMins < 1) {
      return 'Just now';
    } else if (diffMins < 60) {
      return `${diffMins}m ago`;
    } else if (diffHours < 24) {
      return `${diffHours}h ago`;
    } else if (diffDays === 1) {
      return 'Yesterday';
    } else if (diffDays < 7) {
      return `${diffDays}d ago`;
    } else {
      return date.toLocaleDateString();
    }
  };

  return (
    <div className="session-manager">
      <button
        className="session-toggle"
        onClick={() => setIsExpanded(!isExpanded)}
        title="Manage sessions"
      >
        <span className="session-icon">💬</span>
        <span className="session-name">
          {currentSession?.name || 'No active session'}
        </span>
        <span className="session-count">
          {currentSession?.messageCount || 0} messages
        </span>
        <span className={`expand-icon ${isExpanded ? 'expanded' : ''}`}>▼</span>
      </button>

      {isExpanded && (
        <div className="session-dropdown">
          <div className="session-dropdown-header">
            <span className="dropdown-title">Sessions</span>
            <button className="btn-new-session" onClick={handleNewSession}>
              + New
            </button>
          </div>

          {isLoading && (
            <div className="session-loading">
              <div className="loading-spinner-small"></div>
              <span>Loading sessions...</span>
            </div>
          )}

          {!isLoading && sessions.length === 0 && (
            <div className="session-empty">
              <p>No sessions yet</p>
              <button className="btn-create-first" onClick={handleNewSession}>
                Create your first session
              </button>
            </div>
          )}

          {!isLoading && sessions.length > 0 && (
            <div className="session-list">
              {sessions.map((session) => (
                <div
                  key={session.id}
                  className={`session-item ${session.isActive ? 'active' : ''}`}
                  onClick={() => handleSessionClick(session.id)}
                >
                  <div className="session-item-header">
                    <span className="session-item-name">{session.name}</span>
                    <div className="session-item-actions">
                      <span className="session-item-time">
                        {formatTimestamp(session.lastActive)}
                      </span>
                      {onDeleteSession && !session.isActive && (
                        <button
                          className="btn-delete-session"
                          onClick={(e) => handleDeleteSession(e, session.id)}
                          title="Delete session"
                        >
                          ×
                        </button>
                      )}
                    </div>
                  </div>
                  {session.summary && (
                    <div className="session-item-summary">{session.summary}</div>
                  )}
                  <div className="session-item-meta">
                    <span>{session.messageCount} messages</span>
                    {session.isActive && (
                      <span className="session-active-badge">Active</span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
