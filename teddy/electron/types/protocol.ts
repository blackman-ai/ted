// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Ted <-> Teddy JSONL Protocol
 *
 * Streaming protocol for embedded Ted communication.
 * Events are emitted as line-delimited JSON (JSONL) on stdout.
 */

export interface BaseEvent {
  type: string;
  timestamp: number;
  session_id: string;
}

/**
 * PLAN: Shows AI's planned steps before execution
 */
export interface PlanEvent extends BaseEvent {
  type: 'plan';
  data: {
    steps: Array<{
      id: string;
      description: string;
      estimated_files?: string[];
    }>;
  };
}

/**
 * FILE_CREATE: Create a new file with content
 */
export interface FileCreateEvent extends BaseEvent {
  type: 'file_create';
  data: {
    path: string;           // Relative to project root
    content: string;        // Full file contents
    mode?: number;          // Unix permissions (default: 0o644)
  };
}

/**
 * FILE_EDIT: Modify an existing file
 */
export interface FileEditEvent extends BaseEvent {
  type: 'file_edit';
  data: {
    path: string;
    operation: 'replace' | 'insert' | 'delete';
    // For replace:
    old_text?: string;
    new_text?: string;
    // For insert/delete:
    line?: number;
    text?: string;
  };
}

/**
 * FILE_DELETE: Remove a file
 */
export interface FileDeleteEvent extends BaseEvent {
  type: 'file_delete';
  data: {
    path: string;
  };
}

/**
 * COMMAND: Execute a shell command
 */
export interface CommandEvent extends BaseEvent {
  type: 'command';
  data: {
    command: string;
    cwd?: string;
    env?: Record<string, string>;
  };
}

/**
 * COMMAND_OUTPUT: Streaming output from a shell command
 */
export interface CommandOutputEvent extends BaseEvent {
  type: 'command_output';
  data: {
    stream: 'stdout' | 'stderr';
    text: string;
    done?: boolean;
    exit_code?: number;
  };
}

/**
 * ERROR: Something went wrong
 */
export interface ErrorEvent extends BaseEvent {
  type: 'error';
  data: {
    code: string;           // ERROR_FILE_NOT_FOUND, etc.
    message: string;
    suggested_fix?: string;
    context?: unknown;
  };
}

/**
 * STATUS: Progress updates and state changes
 */
export interface StatusEvent extends BaseEvent {
  type: 'status';
  data: {
    state: 'thinking' | 'reading' | 'writing' | 'running';
    message: string;
    progress?: number;      // 0-100
  };
}

/**
 * COMPLETION: Task finished successfully or with errors
 */
export interface CompletionEvent extends BaseEvent {
  type: 'completion';
  data: {
    success: boolean;
    summary: string;
    files_changed: string[];
  };
}

/**
 * MESSAGE: AI assistant message (streamed text)
 */
export interface MessageEvent extends BaseEvent {
  type: 'message';
  data: {
    role: 'assistant' | 'user';
    content: string;
    delta?: boolean;  // true if streaming chunk, false if complete
  };
}

/**
 * CONVERSATION_HISTORY: Emitted at end of turn for multi-turn persistence
 */
export interface ConversationHistoryEvent extends BaseEvent {
  type: 'conversation_history';
  data: {
    messages: Array<{
      role: 'user' | 'assistant';
      content: string;
    }>;
  };
}

/**
 * Union type of all possible events
 */
export type TedEvent =
  | PlanEvent
  | FileCreateEvent
  | FileEditEvent
  | FileDeleteEvent
  | CommandEvent
  | CommandOutputEvent
  | ErrorEvent
  | StatusEvent
  | CompletionEvent
  | MessageEvent
  | ConversationHistoryEvent;

/**
 * Type guard to check if an object is a valid TedEvent
 */
export function isTedEvent(obj: unknown): obj is TedEvent {
  if (typeof obj !== 'object' || obj === null) return false;
  const event = obj as Partial<BaseEvent>;
  return (
    typeof event.type === 'string' &&
    typeof event.timestamp === 'number' &&
    typeof event.session_id === 'string' &&
    'data' in event
  );
}
