// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { EventEmitter } from 'events';
import { TedEvent, isTedEvent } from '../types/protocol';
import { debugLog } from '../utils/logger';

type MessageEvent = Extract<TedEvent, { type: 'message' }>;

interface ParsedToolCall {
  name: string;
  input: Record<string, unknown>;
}

interface TedParserOptions {
  enableSyntheticEvents?: boolean;
}

/**
 * Streaming JSONL parser for Ted events
 *
 * Parses line-delimited JSON from Ted's stdout, emitting typed events.
 * Handles incomplete lines and buffering.
 */
export class TedParser extends EventEmitter {
  private buffer: string = '';
  private readonly enableSyntheticEvents: boolean;

  constructor(options: TedParserOptions = {}) {
    super();
    this.enableSyntheticEvents = options.enableSyntheticEvents ?? true;
  }

  on(event: 'event', listener: (event: TedEvent) => void): this;
  on(event: 'plan', listener: (event: Extract<TedEvent, { type: 'plan' }>) => void): this;
  on(event: 'file_create', listener: (event: Extract<TedEvent, { type: 'file_create' }>) => void): this;
  on(event: 'file_edit', listener: (event: Extract<TedEvent, { type: 'file_edit' }>) => void): this;
  on(event: 'file_delete', listener: (event: Extract<TedEvent, { type: 'file_delete' }>) => void): this;
  on(event: 'command', listener: (event: Extract<TedEvent, { type: 'command' }>) => void): this;
  on(event: 'status', listener: (event: Extract<TedEvent, { type: 'status' }>) => void): this;
  on(event: 'completion', listener: (event: Extract<TedEvent, { type: 'completion' }>) => void): this;
  on(event: 'message', listener: (event: Extract<TedEvent, { type: 'message' }>) => void): this;
  on(
    event: 'conversation_history',
    listener: (event: Extract<TedEvent, { type: 'conversation_history' }>) => void
  ): this;
  on(event: 'error', listener: (error: Error) => void): this;
  on(event: string | symbol, listener: Parameters<EventEmitter['on']>[1]): this {
    return super.on(event, listener);
  }

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

        if (this.enableSyntheticEvents && parsed.type === 'message') {
          const syntheticEvents = this.extractSyntheticEventsFromMessage(parsed);
          for (const syntheticEvent of syntheticEvents) {
            this.emit('event', syntheticEvent);
            this.emit(syntheticEvent.type, syntheticEvent);
          }
        }
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

  private extractSyntheticEventsFromMessage(event: MessageEvent): TedEvent[] {
    const content = event.data?.content;
    if (
      event.data?.role !== 'assistant' ||
      typeof content !== 'string' ||
      content.trim().length === 0 ||
      event.data?.delta === true
    ) {
      return [];
    }

    const dedupe = new Set<string>();
    const mapped: TedEvent[] = [];

    const candidates = this.extractJsonCandidates(content);
    const toolCalls: ParsedToolCall[] = [];
    for (const candidate of candidates) {
      toolCalls.push(...this.extractToolCalls(candidate));
    }

    for (const call of toolCalls) {
      const mappedEvent = this.mapToolCallToEvent(call, event);
      if (!mappedEvent) {
        continue;
      }
      mapped.push(mappedEvent);
    }

    const markdownFileEvents = this.extractMarkdownFileEvents(content, event);
    mapped.push(...markdownFileEvents);

    const deduped: TedEvent[] = [];
    for (const mappedEvent of mapped) {
      const key = JSON.stringify(mappedEvent);
      if (dedupe.has(key)) {
        continue;
      }
      dedupe.add(key);
      deduped.push(mappedEvent);
    }

    if (deduped.length > 0) {
      debugLog(
        'TED_PARSER',
        `Synthesized ${deduped.length} tool event(s) from assistant message fallback parsing`
      );
    }

    return deduped;
  }

  private extractJsonCandidates(content: string): unknown[] {
    const candidates: unknown[] = [];
    const seen = new Set<string>();

    const tryParse = (raw: string) => {
      const trimmed = raw.trim();
      if (!trimmed || seen.has(trimmed)) {
        return;
      }
      seen.add(trimmed);

      const parsed = this.tryParseJson(trimmed);
      if (parsed !== null) {
        candidates.push(parsed);
        return;
      }

      const firstBrace = trimmed.indexOf('{');
      const lastBrace = trimmed.lastIndexOf('}');
      if (firstBrace >= 0 && lastBrace > firstBrace) {
        const objectSlice = trimmed.slice(firstBrace, lastBrace + 1).trim();
        const objectParsed = this.tryParseJson(objectSlice);
        if (objectParsed !== null) {
          candidates.push(objectParsed);
        }
      }

      const firstBracket = trimmed.indexOf('[');
      const lastBracket = trimmed.lastIndexOf(']');
      if (firstBracket >= 0 && lastBracket > firstBracket) {
        const arraySlice = trimmed.slice(firstBracket, lastBracket + 1).trim();
        const arrayParsed = this.tryParseJson(arraySlice);
        if (arrayParsed !== null) {
          candidates.push(arrayParsed);
        }
      }
    };

    tryParse(content);

    const fencedPattern = /```(?:json)?\s*([\s\S]*?)```/gi;
    let match: RegExpExecArray | null;
    while ((match = fencedPattern.exec(content)) !== null) {
      if (match[1]) {
        tryParse(match[1]);
      }
    }

    return candidates;
  }

  private tryParseJson(value: string): unknown | null {
    try {
      return JSON.parse(value);
    } catch {
      return null;
    }
  }

  private extractToolCalls(value: unknown): ParsedToolCall[] {
    if (Array.isArray(value)) {
      return value.flatMap(item => this.extractToolCalls(item));
    }

    const record = this.asRecord(value);
    if (!record) {
      return [];
    }

    const calls: ParsedToolCall[] = [];

    const direct = this.parseDirectToolCall(record);
    if (direct) {
      calls.push(direct);
    }

    for (const key of ['tool_calls', 'calls', 'actions']) {
      const nested = record[key];
      if (Array.isArray(nested)) {
        calls.push(...nested.flatMap(item => this.extractToolCalls(item)));
      }
    }

    const nestedContent = record.content;
    if (Array.isArray(nestedContent)) {
      calls.push(...nestedContent.flatMap(item => this.extractToolCalls(item)));
    }

    return calls;
  }

  private extractMarkdownFileEvents(content: string, message: MessageEvent): TedEvent[] {
    const events: TedEvent[] = [];
    const blockPattern = /```[^\n]*\n([\s\S]*?)```/g;
    let match: RegExpExecArray | null;

    while ((match = blockPattern.exec(content)) !== null) {
      const rawCode = match[1];
      if (typeof rawCode !== 'string') {
        continue;
      }

      const filePath = this.findNearestFilePath(content, match.index);
      if (!filePath) {
        continue;
      }

      const normalizedCode = rawCode.replace(/\n+$/, '');
      if (normalizedCode.trim().length === 0) {
        continue;
      }

      events.push({
        type: 'file_create',
        timestamp: message.timestamp,
        session_id: message.session_id,
        data: {
          path: filePath,
          content: normalizedCode,
        },
      });
    }

    return events;
  }

  private findNearestFilePath(content: string, index: number): string | null {
    const lookbackWindow = 240;
    const start = Math.max(0, index - lookbackWindow);
    const lookback = content.slice(start, index);
    const pathCandidates = [...lookback.matchAll(/`([^`\n]{1,200})`/g)];

    for (let i = pathCandidates.length - 1; i >= 0; i -= 1) {
      const candidate = pathCandidates[i][1]?.trim();
      if (candidate && this.isLikelyFilePath(candidate)) {
        return candidate;
      }
    }

    return null;
  }

  private isLikelyFilePath(value: string): boolean {
    if (!value || value.includes('://') || value.startsWith('http')) {
      return false;
    }
    if (value.includes(' ') || value.startsWith('npx ') || value.startsWith('npm ')) {
      return false;
    }
    if (value.length > 200 || value.endsWith('/')) {
      return false;
    }

    const hasSlash = value.includes('/');
    const hasExtension = /[A-Za-z0-9_-]+\.[A-Za-z0-9._-]+$/.test(value);
    return hasSlash || hasExtension;
  }

  private parseDirectToolCall(record: Record<string, unknown>): ParsedToolCall | null {
    const normalizedType = this.getString(record, 'type')?.toLowerCase();

    if (normalizedType === 'tool_use') {
      const name = this.getString(record, 'name');
      const input = this.toRecord(record.input);
      if (name && input) {
        return { name: this.normalizeToolName(name), input };
      }
      return null;
    }

    const fn = this.asRecord(record.function);
    if (fn) {
      const fnName = this.getString(fn, 'name');
      const fnArgsRaw = fn.arguments;
      const fnInput = this.parseInput(fnArgsRaw);
      if (fnName && fnInput) {
        return { name: this.normalizeToolName(fnName), input: fnInput };
      }
    }

    const name =
      this.getString(record, 'name') ||
      this.getString(record, 'tool') ||
      this.getString(record, 'tool_name');
    if (!name) {
      return null;
    }

    const input =
      this.parseInput(record.input) ||
      this.parseInput(record.args) ||
      this.parseInput(record.arguments) ||
      this.parseInput(record.parameters) ||
      {};

    return { name: this.normalizeToolName(name), input };
  }

  private parseInput(value: unknown): Record<string, unknown> | null {
    const fromRecord = this.toRecord(value);
    if (fromRecord) {
      return fromRecord;
    }

    if (typeof value === 'string') {
      const parsed = this.tryParseJson(value);
      return this.toRecord(parsed);
    }

    return null;
  }

  private mapToolCallToEvent(call: ParsedToolCall, message: MessageEvent): TedEvent | null {
    const base = {
      timestamp: message.timestamp,
      session_id: message.session_id,
    };

    switch (call.name) {
      case 'file_write':
      case 'write_file':
      case 'create_file':
      case 'file_create': {
        const path = this.pickPath(call.input);
        const content =
          this.getString(call.input, 'content') ??
          this.getString(call.input, 'text') ??
          this.getString(call.input, 'new_content');
        if (!path || content === null) {
          return null;
        }
        return {
          type: 'file_create',
          ...base,
          data: {
            path,
            content,
          },
        };
      }

      case 'file_edit':
      case 'edit_file':
      case 'replace_in_file':
      case 'str_replace_editor': {
        const path = this.pickPath(call.input);
        if (!path) {
          return null;
        }

        const operation = this.getString(call.input, 'operation');
        const oldText =
          this.getString(call.input, 'old_text') ??
          this.getString(call.input, 'oldText') ??
          this.getString(call.input, 'find');
        const newText =
          this.getString(call.input, 'new_text') ??
          this.getString(call.input, 'newText') ??
          this.getString(call.input, 'replace');
        const line = this.getNumber(call.input, 'line');
        const text = this.getString(call.input, 'text');

        if (operation === 'insert' && line !== null && text !== null) {
          return {
            type: 'file_edit',
            ...base,
            data: {
              path,
              operation: 'insert',
              line,
              text,
            },
          };
        }

        if (operation === 'delete' && line !== null) {
          return {
            type: 'file_edit',
            ...base,
            data: {
              path,
              operation: 'delete',
              line,
            },
          };
        }

        if (oldText !== null || newText !== null) {
          return {
            type: 'file_edit',
            ...base,
            data: {
              path,
              operation: 'replace',
              old_text: oldText ?? '',
              new_text: newText ?? '',
            },
          };
        }

        return null;
      }

      case 'file_delete':
      case 'delete_file':
      case 'remove_file': {
        const path = this.pickPath(call.input);
        if (!path) {
          return null;
        }
        return {
          type: 'file_delete',
          ...base,
          data: {
            path,
          },
        };
      }

      case 'shell':
      case 'command':
      case 'run_command':
      case 'bash': {
        const command =
          this.getString(call.input, 'command') ??
          this.getString(call.input, 'cmd') ??
          this.getString(call.input, 'script');
        if (!command) {
          return null;
        }
        const cwd = this.getString(call.input, 'cwd') ?? undefined;
        return {
          type: 'command',
          ...base,
          data: {
            command,
            cwd,
          },
        };
      }

      default:
        return null;
    }
  }

  private pickPath(input: Record<string, unknown>): string | null {
    return (
      this.getString(input, 'path') ??
      this.getString(input, 'file_path') ??
      this.getString(input, 'filepath') ??
      this.getString(input, 'file') ??
      this.getString(input, 'target')
    );
  }

  private normalizeToolName(name: string): string {
    return name.trim().toLowerCase().replace(/\s+/g, '_');
  }

  private asRecord(value: unknown): Record<string, unknown> | null {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      return null;
    }
    return value as Record<string, unknown>;
  }

  private toRecord(value: unknown): Record<string, unknown> | null {
    return this.asRecord(value);
  }

  private getString(record: Record<string, unknown>, key: string): string | null {
    const value = record[key];
    if (typeof value !== 'string') {
      return null;
    }
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  private getNumber(record: Record<string, unknown>, key: string): number | null {
    const value = record[key];
    if (typeof value === 'number' && Number.isFinite(value)) {
      return value;
    }
    if (typeof value === 'string') {
      const parsed = Number(value);
      if (Number.isFinite(parsed)) {
        return parsed;
      }
    }
    return null;
  }
}
