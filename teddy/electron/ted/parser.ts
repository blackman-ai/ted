// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { EventEmitter } from 'events';
import { TedEvent, isTedEvent } from '../types/protocol';

/**
 * Streaming JSONL parser for Ted events
 *
 * Parses line-delimited JSON from Ted's stdout, emitting typed events.
 * Handles incomplete lines and buffering.
 */
export class TedParser extends EventEmitter {
  private buffer: string = '';

  /**
   * Feed data to the parser (call with each stdout chunk)
   */
  feed(data: string): void {
    this.buffer += data;
    this.processBuffer();
  }

  /**
   * Process complete lines from the buffer
   */
  private processBuffer(): void {
    const lines = this.buffer.split('\n');

    // Keep the last (possibly incomplete) line in the buffer
    this.buffer = lines.pop() || '';

    for (const line of lines) {
      if (line.trim()) {
        this.parseLine(line);
      }
    }
  }

  /**
   * Parse a single line of JSON
   */
  private parseLine(line: string): void {
    try {
      const parsed = JSON.parse(line);

      if (isTedEvent(parsed)) {
        this.emit('event', parsed);
        this.emit(parsed.type, parsed);
      } else {
        this.emit('error', new Error(`Invalid event format: ${line}`));
      }
    } catch (err) {
      this.emit('error', new Error(`Failed to parse JSON: ${line}`));
    }
  }

  /**
   * Flush any remaining data in the buffer
   */
  flush(): void {
    if (this.buffer.trim()) {
      this.parseLine(this.buffer);
      this.buffer = '';
    }
  }

  /**
   * Reset the parser state
   */
  reset(): void {
    this.buffer = '';
    this.removeAllListeners();
  }
}

/**
 * Typed event listeners for TedParser
 */
export interface TedParserEvents {
  on(event: 'event', listener: (event: TedEvent) => void): this;
  on(event: 'plan', listener: (event: Extract<TedEvent, { type: 'plan' }>) => void): this;
  on(event: 'file_create', listener: (event: Extract<TedEvent, { type: 'file_create' }>) => void): this;
  on(event: 'file_edit', listener: (event: Extract<TedEvent, { type: 'file_edit' }>) => void): this;
  on(event: 'file_delete', listener: (event: Extract<TedEvent, { type: 'file_delete' }>) => void): this;
  on(event: 'command', listener: (event: Extract<TedEvent, { type: 'command' }>) => void): this;
  on(event: 'error', listener: (error: Error) => void): this;
  on(event: 'status', listener: (event: Extract<TedEvent, { type: 'status' }>) => void): this;
  on(event: 'completion', listener: (event: Extract<TedEvent, { type: 'completion' }>) => void): this;
  on(event: 'message', listener: (event: Extract<TedEvent, { type: 'message' }>) => void): this;
  on(event: 'conversation_history', listener: (event: Extract<TedEvent, { type: 'conversation_history' }>) => void): this;
}

// Apply the interface to the class
export interface TedParser extends TedParserEvents {}
