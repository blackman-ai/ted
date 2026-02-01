// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import fs from 'fs/promises';
import path from 'path';
import { FileCreateEvent, FileEditEvent, FileDeleteEvent } from '../types/protocol';

export interface FileApplierOptions {
  projectRoot: string;
}

/**
 * Applies file operations from Ted events to disk
 */
export class FileApplier {
  private projectRoot: string;

  constructor(options: FileApplierOptions) {
    this.projectRoot = options.projectRoot;
  }

  /**
   * Apply a file create operation
   */
  async applyCreate(event: FileCreateEvent): Promise<void> {
    const fullPath = this.resolvePath(event.data.path);

    // Ensure directory exists
    await fs.mkdir(path.dirname(fullPath), { recursive: true });

    // Write file
    await fs.writeFile(fullPath, event.data.content, 'utf-8');

    // Set permissions if specified
    if (event.data.mode !== undefined) {
      await fs.chmod(fullPath, event.data.mode);
    }
  }

  /**
   * Apply a file edit operation
   */
  async applyEdit(event: FileEditEvent): Promise<void> {
    const fullPath = this.resolvePath(event.data.path);

    // Read existing file
    const content = await fs.readFile(fullPath, 'utf-8');

    let newContent: string;

    switch (event.data.operation) {
      case 'replace':
        if (!event.data.old_text || event.data.new_text === undefined) {
          throw new Error('Replace operation requires old_text and new_text');
        }
        // Check if old_text exists in the file
        if (!content.includes(event.data.old_text)) {
          // Try to find a close match to give helpful feedback
          const firstLine = event.data.old_text.split('\n')[0].trim();
          const hasPartialMatch = firstLine && content.includes(firstLine);
          const hint = hasPartialMatch
            ? `Found partial match for "${firstLine.substring(0, 50)}..." but full text differs. The file may have changed.`
            : `The text to replace was not found in the file. The model may be hallucinating file content.`;
          throw new Error(`Edit failed: old_text not found in ${event.data.path}. ${hint}`);
        }
        newContent = content.replace(event.data.old_text, event.data.new_text);
        break;

      case 'insert':
        if (event.data.line === undefined || !event.data.text) {
          throw new Error('Insert operation requires line and text');
        }
        newContent = this.insertAtLine(content, event.data.line, event.data.text);
        break;

      case 'delete':
        if (event.data.line === undefined) {
          throw new Error('Delete operation requires line');
        }
        newContent = this.deleteAtLine(content, event.data.line);
        break;

      default:
        throw new Error(`Unknown operation: ${event.data.operation}`);
    }

    // Write updated content
    await fs.writeFile(fullPath, newContent, 'utf-8');
  }

  /**
   * Apply a file delete operation
   */
  async applyDelete(event: FileDeleteEvent): Promise<void> {
    const fullPath = this.resolvePath(event.data.path);
    await fs.unlink(fullPath);
  }

  /**
   * Resolve a relative path to an absolute path within the project
   */
  private resolvePath(relativePath: string): string {
    const resolved = path.resolve(this.projectRoot, relativePath);

    // Security check: ensure path is within project root
    if (!resolved.startsWith(this.projectRoot)) {
      throw new Error(`Path escapes project root: ${relativePath}`);
    }

    return resolved;
  }

  /**
   * Insert text at a specific line
   */
  private insertAtLine(content: string, lineNumber: number, text: string): string {
    const lines = content.split('\n');

    if (lineNumber < 0 || lineNumber > lines.length) {
      throw new Error(`Line number ${lineNumber} out of range`);
    }

    lines.splice(lineNumber, 0, text);
    return lines.join('\n');
  }

  /**
   * Delete a specific line
   */
  private deleteAtLine(content: string, lineNumber: number): string {
    const lines = content.split('\n');

    if (lineNumber < 0 || lineNumber >= lines.length) {
      throw new Error(`Line number ${lineNumber} out of range`);
    }

    lines.splice(lineNumber, 1);
    return lines.join('\n');
  }
}
