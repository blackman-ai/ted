// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useRef, useEffect, useMemo } from 'react';
import './ChatPanel.css';

interface TedEvent {
  type: string;
  timestamp: number;
  data: any;
}

interface ChatPanelProps {
  onSendMessage: (message: string, options?: any) => void;
  onStop: () => void;
  events: TedEvent[];
  isRunning: boolean;
}

interface ProcessedMessage {
  id: string;
  type: 'message' | 'plan' | 'status' | 'completion' | 'error' | 'file_create' | 'file_edit' | 'command' | 'command_output';
  role?: string;
  content: string;
  data?: any;
}

export function ChatPanel({ onSendMessage, onStop, events, isRunning }: ChatPanelProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Process events to combine streaming deltas into complete messages
  const messages = useMemo(() => {
    const processed: ProcessedMessage[] = [];
    let currentAssistantMessage: ProcessedMessage | null = null;
    let messageIndex = 0;

    for (const event of events) {
      switch (event.type) {
        case 'message':
          if (event.data.role === 'assistant') {
            if (event.data.delta) {
              // Streaming delta - accumulate into current message
              if (currentAssistantMessage) {
                currentAssistantMessage.content += event.data.content;
              } else {
                currentAssistantMessage = {
                  id: `msg-${messageIndex++}`,
                  type: 'message',
                  role: 'assistant',
                  content: event.data.content,
                };
              }
            } else {
              // Complete message - finalize and add
              if (currentAssistantMessage) {
                processed.push(currentAssistantMessage);
                currentAssistantMessage = null;
              }
              processed.push({
                id: `msg-${messageIndex++}`,
                type: 'message',
                role: 'assistant',
                content: event.data.content,
              });
            }
          } else {
            // User message
            if (currentAssistantMessage) {
              processed.push(currentAssistantMessage);
              currentAssistantMessage = null;
            }
            processed.push({
              id: `msg-${messageIndex++}`,
              type: 'message',
              role: event.data.role,
              content: event.data.content,
            });
          }
          break;

        case 'status':
          // Don't finalize assistant message for status updates
          // Just show status as a separate indicator
          processed.push({
            id: `status-${messageIndex++}`,
            type: 'status',
            content: event.data.message,
            data: event.data,
          });
          break;

        case 'plan':
          if (currentAssistantMessage) {
            processed.push(currentAssistantMessage);
            currentAssistantMessage = null;
          }
          processed.push({
            id: `plan-${messageIndex++}`,
            type: 'plan',
            content: '',
            data: event.data,
          });
          break;

        case 'file_create':
          if (currentAssistantMessage) {
            processed.push(currentAssistantMessage);
            currentAssistantMessage = null;
          }
          processed.push({
            id: `file-${messageIndex++}`,
            type: 'file_create',
            content: `Created: ${event.data.path}`,
            data: event.data,
          });
          break;

        case 'file_edit':
          if (currentAssistantMessage) {
            processed.push(currentAssistantMessage);
            currentAssistantMessage = null;
          }
          processed.push({
            id: `edit-${messageIndex++}`,
            type: 'file_edit',
            content: `Edited: ${event.data.path}`,
            data: event.data,
          });
          break;

        case 'command':
          if (currentAssistantMessage) {
            processed.push(currentAssistantMessage);
            currentAssistantMessage = null;
          }
          processed.push({
            id: `cmd-${messageIndex++}`,
            type: 'command',
            content: event.data.command,
            data: { ...event.data, output: '', exitCode: null, isRunning: true },
          });
          break;

        case 'command_output':
          // Find the most recent command message and append output
          // Create a new object to trigger React re-render
          for (let i = processed.length - 1; i >= 0; i--) {
            if (processed[i].type === 'command') {
              processed[i] = {
                ...processed[i],
                data: {
                  ...processed[i].data,
                  output: (processed[i].data.output || '') + event.data.text,
                  isRunning: event.data.done ? false : processed[i].data.isRunning,
                  exitCode: event.data.done ? event.data.exit_code : processed[i].data.exitCode,
                },
              };
              break;
            }
          }
          break;

        case 'completion':
          if (currentAssistantMessage) {
            processed.push(currentAssistantMessage);
            currentAssistantMessage = null;
          }
          processed.push({
            id: `complete-${messageIndex++}`,
            type: 'completion',
            content: event.data.summary,
            data: event.data,
          });
          break;

        case 'error':
          if (currentAssistantMessage) {
            processed.push(currentAssistantMessage);
            currentAssistantMessage = null;
          }
          processed.push({
            id: `error-${messageIndex++}`,
            type: 'error',
            content: event.data.message,
            data: event.data,
          });
          break;
      }
    }

    // Add any pending assistant message (for streaming in progress)
    if (currentAssistantMessage) {
      processed.push(currentAssistantMessage);
    }

    return processed;
  }, [events]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSend = () => {
    if (input.trim() && !isRunning) {
      onSendMessage(input.trim());
      setInput('');
    }
  };

  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const renderMessage = (msg: ProcessedMessage) => {
    switch (msg.type) {
      case 'message':
        return (
          <div key={msg.id} className={`message ${msg.role}`}>
            <div className="message-content">
              {msg.content}
            </div>
          </div>
        );

      case 'plan':
        return (
          <div key={msg.id} className="message assistant">
            <div className="message-header">Plan</div>
            <div className="message-content">
              <ul className="plan-list">
                {msg.data.steps.map((step: any, i: number) => (
                  <li key={i}>{step.description}</li>
                ))}
              </ul>
            </div>
          </div>
        );

      case 'status':
        return (
          <div key={msg.id} className="message status">
            <div className="message-content">
              <span className="status-indicator">{getStatusIcon(msg.data.state)}</span>
              {msg.content}
            </div>
          </div>
        );

      case 'file_create':
        return (
          <div key={msg.id} className="message file-operation">
            <div className="message-content">
              <span className="file-icon">📄</span> {msg.content}
            </div>
          </div>
        );

      case 'file_edit':
        return (
          <div key={msg.id} className="message file-operation">
            <div className="message-content">
              <span className="file-icon">✏️</span> {msg.content}
            </div>
          </div>
        );

      case 'command':
        return (
          <div key={msg.id} className="message command">
            <div className="message-content">
              <div className="command-header">
                <span className="command-icon">$</span>
                <code>{msg.content}</code>
                {msg.data.isRunning && <span className="command-running">Running...</span>}
                {!msg.data.isRunning && msg.data.exitCode !== null && (
                  <span className={`command-exit ${msg.data.exitCode === 0 ? 'success' : 'error'}`}>
                    Exit: {msg.data.exitCode}
                  </span>
                )}
              </div>
              {msg.data.output && (
                <pre className="command-output">{msg.data.output}</pre>
              )}
            </div>
          </div>
        );

      case 'completion':
        return (
          <div key={msg.id} className={`message completion ${msg.data.success ? 'success' : 'error'}`}>
            <div className="message-content">
              {msg.data.success ? '✓' : '✗'} {msg.content}
              {msg.data.files_changed?.length > 0 && (
                <div className="files-changed">
                  Files changed: {msg.data.files_changed.join(', ')}
                </div>
              )}
            </div>
          </div>
        );

      case 'error':
        return (
          <div key={msg.id} className="message error">
            <div className="message-header">Error</div>
            <div className="message-content">
              {msg.content}
              {msg.data.suggested_fix && (
                <div className="suggested-fix">
                  Suggestion: {msg.data.suggested_fix}
                </div>
              )}
            </div>
          </div>
        );

      default:
        return null;
    }
  };

  const getStatusIcon = (state: string): string => {
    const icons: Record<string, string> = {
      thinking: '🤔',
      reading: '📖',
      writing: '✍️',
      running: '⚙️',
    };
    return icons[state] || '•';
  };

  return (
    <div className="chat-panel">
      <div className="messages">
        {messages.length === 0 && (
          <div className="empty-chat">
            <p>Start a conversation with Teddy</p>
            <p className="empty-hint">
              Ask me to create, modify, or explain code
            </p>
          </div>
        )}
        {messages.map(renderMessage)}
        <div ref={messagesEndRef} />
      </div>

      <div className="input-area">
        <textarea
          className="chat-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyPress={handleKeyPress}
          placeholder={isRunning ? "Teddy is working..." : "Ask Teddy to code something..."}
          disabled={isRunning}
          rows={3}
        />
        {isRunning ? (
          <button
            className="btn-primary stop"
            onClick={onStop}
          >
            Stop
          </button>
        ) : (
          <button
            className="btn-primary"
            onClick={handleSend}
            disabled={!input.trim()}
          >
            Send
          </button>
        )}
      </div>
    </div>
  );
}
