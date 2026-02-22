// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useRef, useEffect, useMemo } from 'react';
import './ChatPanel.css';

interface TedEvent {
  type: string;
  timestamp: number;
  data: EventData;
}

interface ChatPanelProps {
  onSendMessage: (message: string, options?: Record<string, unknown>) => void;
  onStop: () => void;
  events: TedEvent[];
  isRunning: boolean;
  focusRequestToken?: number;
}

interface EventData {
  role?: string;
  content?: string;
  delta?: boolean;
  message?: string;
  path?: string;
  command?: string;
  output?: string;
  isRunning?: boolean;
  exitCode?: number | null;
  text?: string;
  done?: boolean;
  exit_code?: number | null;
  summary?: string;
  success?: boolean;
  files_changed?: string[];
  suggested_fix?: string;
  state?: string;
  steps?: Array<{ description?: string }>;
  [key: string]: unknown;
}

interface ProcessedMessage {
  id: string;
  type: 'message' | 'plan' | 'status' | 'completion' | 'error' | 'file_create' | 'file_edit' | 'command' | 'command_output';
  role?: string;
  content: string;
  data?: EventData;
}

/**
 * Strip tool call markup from assistant messages
 * Handles multiple formats:
 * - Qwen XML: <function=name>...</function> or <tool_call>...</tool_call>
 * - JSON in code blocks: ```json {"name": "tool", ...} ```
 */
function stripToolCallMarkup(content: string): string {
  // Remove Qwen-style XML tool calls: <function=name>...</function>
  let cleaned = content.replace(/<function=\w+>[\s\S]*?<\/function>/g, '');

  // Remove <tool_call>...</tool_call> wrappers (with or without content)
  cleaned = cleaned.replace(/<\/?tool_call>/g, '');

  // Remove JSON tool calls in markdown code blocks
  cleaned = cleaned.replace(/```(?:json)?\s*\n?\s*\{[\s\S]*?"name"\s*:\s*"[\w_]+"\s*,\s*"arguments"[\s\S]*?\}\s*\n?```/g, '');

  // Remove standalone JSON tool calls
  cleaned = cleaned.replace(/\{[\s\S]*?"name"\s*:\s*"(?:glob|file_read|file_write|file_edit|shell|grep)"[\s\S]*?"arguments"[\s\S]*?\}/g, '');

  // Clean up extra whitespace left behind
  cleaned = cleaned.replace(/\n{3,}/g, '\n\n').trim();

  return cleaned;
}

export function ChatPanel({
  onSendMessage,
  onStop,
  events,
  isRunning,
  focusRequestToken,
}: ChatPanelProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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
                currentAssistantMessage.content += event.data.content || '';
              } else {
                currentAssistantMessage = {
                  id: `msg-${messageIndex++}`,
                  type: 'message',
                  role: 'assistant',
                  content: event.data.content || '',
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
                content: event.data.content || '',
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
              content: event.data.content || '',
            });
          }
          break;

        case 'status':
          // Don't finalize assistant message for status updates
          // Just show status as a separate indicator
          processed.push({
            id: `status-${messageIndex++}`,
            type: 'status',
            content: event.data.message || '',
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
            content: `Created: ${event.data.path || ''}`,
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
            content: `Edited: ${event.data.path || ''}`,
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
            content: event.data.command || '',
            data: { ...event.data, output: '', exitCode: null, isRunning: true } as EventData,
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
                    output: (processed[i].data?.output || '') + (event.data.text || ''),
                    isRunning: event.data.done ? false : processed[i].data?.isRunning,
                    exitCode: event.data.done ? event.data.exit_code : processed[i].data?.exitCode,
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
            content: event.data.summary || '',
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
            content: event.data.message || '',
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

  useEffect(() => {
    if (focusRequestToken === undefined || isRunning) {
      return;
    }
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }
    textarea.focus();
    const end = textarea.value.length;
    textarea.setSelectionRange(end, end);
  }, [focusRequestToken, isRunning]);

  const handleSend = () => {
    if (input.trim() && !isRunning) {
      onSendMessage(input.trim());
      setInput('');
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const renderMessage = (msg: ProcessedMessage) => {
    switch (msg.type) {
      case 'message': {
        // For assistant messages, strip out tool call markup before displaying
        const displayContent = msg.role === 'assistant'
          ? stripToolCallMarkup(msg.content)
          : msg.content;

        // Don't render empty messages (e.g., if the message was only tool calls)
        if (!displayContent.trim()) {
          return null;
        }

        return (
          <div key={msg.id} className={`message ${msg.role}`}>
            <div className="message-content">
              {displayContent}
            </div>
          </div>
        );
      }

      case 'plan': {
        const steps = msg.data?.steps || [];
        return (
          <div key={msg.id} className="message assistant">
            <div className="message-header">Plan</div>
            <div className="message-content">
              <ul className="plan-list">
                {steps.map((step, i) => (
                  <li key={i}>{step.description || ''}</li>
                ))}
              </ul>
            </div>
          </div>
        );
      }

      case 'status':
        return (
          <div key={msg.id} className="message status">
            <div className="message-content">
              <span className="status-indicator">{getStatusIcon(msg.data?.state || '')}</span>
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
                {msg.data?.isRunning && <span className="command-running">Running...</span>}
                {!msg.data?.isRunning && msg.data?.exitCode !== null && (
                  <span className={`command-exit ${msg.data?.exitCode === 0 ? 'success' : 'error'}`}>
                    Exit: {msg.data?.exitCode}
                  </span>
                )}
              </div>
              {msg.data?.output && (
                <pre className="command-output">{msg.data?.output}</pre>
              )}
            </div>
          </div>
        );

      case 'completion':
        return (
          <div key={msg.id} className={`message completion ${msg.data?.success ? 'success' : 'error'}`}>
            <div className="message-content">
              {msg.data?.success ? '✓' : '✗'} {msg.content}
              {(msg.data?.files_changed?.length || 0) > 0 && (
                <div className="files-changed">
                  Files changed: {msg.data?.files_changed?.join(', ')}
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
              {msg.data?.suggested_fix && (
                <div className="suggested-fix">
                  Suggestion: {msg.data?.suggested_fix}
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
          ref={textareaRef}
          className="chat-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
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
