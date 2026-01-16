// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useEffect, useRef } from 'react';
import './Console.css';

interface LogEntry {
  timestamp: number;
  level: 'info' | 'error' | 'warning';
  message: string;
}

interface ConsoleProps {
  logs: LogEntry[];
}

export function Console({ logs }: ConsoleProps) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  const formatTime = (timestamp: number): string => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString();
  };

  const getLevelIcon = (level: string): string => {
    switch (level) {
      case 'error': return '✗';
      case 'warning': return '⚠';
      default: return '•';
    }
  };

  return (
    <div className="console">
      <div className="console-header">
        <span className="console-title">Console</span>
        <button className="btn-secondary btn-small">Clear</button>
      </div>
      <div className="console-content">
        {logs.length === 0 ? (
          <div className="console-empty">
            <p>No logs yet</p>
          </div>
        ) : (
          logs.map((log, i) => (
            <div key={i} className={`log-entry ${log.level}`}>
              <span className="log-time">{formatTime(log.timestamp)}</span>
              <span className="log-icon">{getLevelIcon(log.level)}</span>
              <span className="log-message">{log.message}</span>
            </div>
          ))
        )}
        <div ref={endRef} />
      </div>
    </div>
  );
}
