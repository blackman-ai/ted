// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect, useCallback } from 'react';
import { Session } from '../components/SessionManager';

export function useSession(projectPath: string) {
  const [currentSession, setCurrentSession] = useState<Session | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const loadProjectSessions = useCallback(async () => {
    if (!projectPath) {
      return;
    }

    setIsLoading(true);
    try {
      // Get all sessions for this project from backend
      const sessionList = await window.teddy.listSessions();
      const sessionInfos: Session[] = sessionList.map(s => ({
        id: s.id,
        name: s.name,
        lastActive: new Date(s.lastActive).toISOString(),
        messageCount: s.messageCount,
        summary: s.summary,
        isActive: false,
      }));

      // Get current session from backend
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
      } else if (sessionInfos.length === 0) {
        // No sessions exist, create one
        const result = await window.teddy.createSession();
        const newSession: Session = {
          id: result.id,
          name: `Session ${sessionInfos.length + 1}`,
          lastActive: new Date().toISOString(),
          messageCount: 0,
          isActive: true,
        };
        setCurrentSession(newSession);
        setSessions([newSession]);
      } else {
        // Sessions exist but none active, switch to most recent
        const newestSessionId = sessionInfos[0].id;
        await window.teddy.switchSession(newestSessionId);
        const updatedSessions = sessionInfos.map(s => ({
          ...s,
          isActive: s.id === newestSessionId,
        }));
        setCurrentSession(updatedSessions[0]);
        setSessions(updatedSessions);
      }
    } catch (err) {
      console.error('Failed to load sessions:', err);
    } finally {
      setIsLoading(false);
    }
  }, [projectPath]);

  // Load sessions for the current project
  useEffect(() => {
    void loadProjectSessions();
  }, [loadProjectSessions]);

  const createNewSession = useCallback(async () => {
    try {
      const result = await window.teddy.createSession();

      // Create local session object
      const newSession: Session = {
        id: result.id,
        name: `Session ${sessions.length + 1}`,
        lastActive: new Date().toISOString(),
        messageCount: 0,
        isActive: true,
      };

      // Mark old sessions as inactive
      const updatedSessions = sessions.map(s => ({ ...s, isActive: false }));
      updatedSessions.unshift(newSession);

      setCurrentSession(newSession);
      setSessions(updatedSessions);

      return result.id;
    } catch (err) {
      console.error('Failed to create new session:', err);
      throw err;
    }
  }, [sessions]);

  const switchSession = useCallback(async (sessionId: string) => {
    try {
      const result = await window.teddy.switchSession(sessionId);

      const targetSession = sessions.find(s => s.id === sessionId);
      if (targetSession) {
        // Update active states
        const updatedSessions = sessions.map(s => ({
          ...s,
          isActive: s.id === sessionId,
        }));

        setCurrentSession({ ...targetSession, isActive: true });
        setSessions(updatedSessions);
      }

      return result;
    } catch (err) {
      console.error('Failed to switch session:', err);
      throw err;
    }
  }, [sessions]);

  const deleteSession = useCallback(async (sessionId: string) => {
    try {
      await window.teddy.deleteSession(sessionId);

      const updatedSessions = sessions.filter(s => s.id !== sessionId);
      setSessions(updatedSessions);

      // If we deleted the active session, switch to another or create new
      if (currentSession?.id === sessionId) {
        if (updatedSessions.length > 0) {
          await switchSession(updatedSessions[0].id);
        } else {
          await createNewSession();
        }
      }
    } catch (err) {
      console.error('Failed to delete session:', err);
      throw err;
    }
  }, [sessions, currentSession, switchSession, createNewSession]);

  const updateSessionMetadata = useCallback((updates: Partial<Session>) => {
    if (!currentSession) return;

    const updated = { ...currentSession, ...updates };
    setCurrentSession(updated);

    setSessions(prev =>
      prev.map(s => (s.id === currentSession.id ? updated : s))
    );
  }, [currentSession]);

  const refresh = useCallback(async () => {
    await loadProjectSessions();
  }, [loadProjectSessions]);

  return {
    currentSession,
    sessions,
    isLoading,
    createNewSession,
    switchSession,
    deleteSession,
    updateSessionMetadata,
    refresh,
  };
}
