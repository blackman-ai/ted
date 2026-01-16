// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect, useCallback } from 'react';

interface TedEvent {
  type: string;
  timestamp: number;
  session_id: string;
  data: any;
}

interface LogEntry {
  timestamp: number;
  level: 'info' | 'error' | 'warning';
  message: string;
}

export function useTed() {
  const [events, setEvents] = useState<TedEvent[]>([]);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [isRunning, setIsRunning] = useState(false);

  useEffect(() => {
    // Listen for Ted events
    const unsubEvent = window.teddy.onTedEvent((event) => {
      // Debug: log command_output events
      if (event.type === 'command_output') {
        console.log('[DEBUG] Received command_output event:', event);
      }

      setEvents((prev) => [...prev, event]);

      // Log status messages
      if (event.type === 'status') {
        addLog('info', event.data.message);
      } else if (event.type === 'error') {
        addLog('error', event.data.message);
      }
    });

    const unsubStderr = window.teddy.onTedStderr((text) => {
      addLog('info', text);
    });

    const unsubError = window.teddy.onTedError((error) => {
      addLog('error', error);
      setIsRunning(false);
    });

    const unsubExit = window.teddy.onTedExit((info) => {
      if (info.code !== 0) {
        addLog('error', `Ted exited with code ${info.code}`);
      }
      setIsRunning(false);
    });

    const unsubFileChanged = window.teddy.onFileChanged((info) => {
      addLog('info', `File ${info.type}: ${info.path}`);
    });

    const unsubGitCommitted = window.teddy.onGitCommitted((info) => {
      addLog('info', `Git commit: ${info.summary}`);
    });

    return () => {
      unsubEvent();
      unsubStderr();
      unsubError();
      unsubExit();
      unsubFileChanged();
      unsubGitCommitted();
    };
  }, []);

  const addLog = (level: LogEntry['level'], message: string) => {
    setLogs((prev) => [...prev, {
      timestamp: Date.now(),
      level,
      message,
    }]);
  };

  const sendPrompt = useCallback(async (prompt: string, options?: {
    trust?: boolean;
    provider?: string;
    model?: string;
    caps?: string[];
  }) => {
    try {
      // Add user message to events immediately so it shows in chat
      const userEvent: TedEvent = {
        type: 'message',
        timestamp: Date.now(),
        session_id: 'local',
        data: {
          role: 'user',
          content: prompt,
        },
      };
      setEvents((prev) => [...prev, userEvent]);

      setIsRunning(true);
      await window.teddy.runTed(prompt, options);
    } catch (err) {
      addLog('error', `Failed to run Ted: ${err}`);
      setIsRunning(false);
    }
  }, []);

  const stop = useCallback(async () => {
    try {
      await window.teddy.stopTed();
      setIsRunning(false);
    } catch (err) {
      addLog('error', `Failed to stop Ted: ${err}`);
    }
  }, []);

  const clearEvents = useCallback(() => {
    setEvents([]);
  }, []);

  const clearLogs = useCallback(() => {
    setLogs([]);
  }, []);

  return {
    events,
    logs,
    isRunning,
    sendPrompt,
    stop,
    clearEvents,
    clearLogs,
  };
}
