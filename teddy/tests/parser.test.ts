// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import assert from 'node:assert/strict';
import test from 'node:test';
import { TedParser } from '../electron/ted/parser';
import { TedEvent } from '../electron/types/protocol';

const BASE_EVENT = {
  timestamp: 1700000000,
  session_id: 'session-test',
};

function collectEvents(parser: TedParser): TedEvent[] {
  const events: TedEvent[] = [];
  parser.on('event', (event) => events.push(event));
  return events;
}

function feedJsonLine(parser: TedParser, value: unknown): void {
  parser.feed(`${JSON.stringify(value)}\n`);
}

test('parses and emits native events', () => {
  const parser = new TedParser();
  const events = collectEvents(parser);

  feedJsonLine(parser, {
    type: 'status',
    ...BASE_EVENT,
    data: {
      state: 'thinking',
      message: 'Planning',
    },
  });

  assert.equal(events.length, 1);
  assert.equal(events[0].type, 'status');
});

test('synthesizes file_create from assistant message JSON', () => {
  const parser = new TedParser();
  const events = collectEvents(parser);

  const toolCall = {
    type: 'tool_use',
    name: 'file_create',
    input: {
      path: 'src/new.ts',
      content: 'export const value = 42;\n',
    },
  };

  feedJsonLine(parser, {
    type: 'message',
    ...BASE_EVENT,
    data: {
      role: 'assistant',
      content: JSON.stringify(toolCall),
    },
  });

  const creates = events.filter(
    (event): event is Extract<TedEvent, { type: 'file_create' }> => event.type === 'file_create'
  );
  assert.equal(creates.length, 1);
  assert.equal(creates[0].data.path, 'src/new.ts');
  assert.equal(creates[0].data.content, 'export const value = 42;');
});

test('synthesizes command from OpenAI-style tool_calls payload', () => {
  const parser = new TedParser();
  const events = collectEvents(parser);

  const toolPayload = {
    tool_calls: [
      {
        function: {
          name: 'shell',
          arguments: JSON.stringify({
            command: 'ls -la',
            cwd: '/tmp',
          }),
        },
      },
    ],
  };

  feedJsonLine(parser, {
    type: 'message',
    ...BASE_EVENT,
    data: {
      role: 'assistant',
      content: `\`\`\`json\n${JSON.stringify(toolPayload)}\n\`\`\``,
    },
  });

  const commands = events.filter(
    (event): event is Extract<TedEvent, { type: 'command' }> => event.type === 'command'
  );
  assert.equal(commands.length, 1);
  assert.equal(commands[0].data.command, 'ls -la');
  assert.equal(commands[0].data.cwd, '/tmp');
});

test('does not synthesize events from streaming delta chunks', () => {
  const parser = new TedParser();
  const events = collectEvents(parser);

  const toolCall = {
    type: 'tool_use',
    name: 'file_delete',
    input: {
      path: 'src/old.ts',
    },
  };

  feedJsonLine(parser, {
    type: 'message',
    ...BASE_EVENT,
    data: {
      role: 'assistant',
      delta: true,
      content: JSON.stringify(toolCall),
    },
  });

  const deletes = events.filter((event) => event.type === 'file_delete');
  assert.equal(deletes.length, 0);
});

test('deduplicates synthesized events from repeated tool calls', () => {
  const parser = new TedParser();
  const events = collectEvents(parser);

  const repeatedCalls = [
    {
      name: 'file_delete',
      input: { path: 'src/dup.ts' },
    },
    {
      name: 'file_delete',
      input: { path: 'src/dup.ts' },
    },
  ];

  feedJsonLine(parser, {
    type: 'message',
    ...BASE_EVENT,
    data: {
      role: 'assistant',
      content: JSON.stringify(repeatedCalls),
    },
  });

  const deletes = events.filter(
    (event): event is Extract<TedEvent, { type: 'file_delete' }> => event.type === 'file_delete'
  );
  assert.equal(deletes.length, 1);
  assert.equal(deletes[0].data.path, 'src/dup.ts');
});

test('synthesizes file_create events from markdown file snippets', () => {
  const parser = new TedParser();
  const events = collectEvents(parser);

  feedJsonLine(parser, {
    type: 'message',
    ...BASE_EVENT,
    data: {
      role: 'assistant',
      content: [
        '### `app/page.tsx`',
        '```tsx',
        'export default function Page() {',
        '  return <main>Hello</main>;',
        '}',
        '```',
      ].join('\n'),
    },
  });

  const creates = events.filter(
    (event): event is Extract<TedEvent, { type: 'file_create' }> => event.type === 'file_create'
  );
  assert.equal(creates.length, 1);
  assert.equal(creates[0].data.path, 'app/page.tsx');
  assert.equal(
    creates[0].data.content,
    ['export default function Page() {', '  return <main>Hello</main>;', '}'].join('\n')
  );
});

test('does not synthesize markdown file events without a file path marker', () => {
  const parser = new TedParser();
  const events = collectEvents(parser);

  feedJsonLine(parser, {
    type: 'message',
    ...BASE_EVENT,
    data: {
      role: 'assistant',
      content: [
        'Here is an example:',
        '```tsx',
        'export default function Page() {',
        '  return <main>Hello</main>;',
        '}',
        '```',
      ].join('\n'),
    },
  });

  const creates = events.filter((event) => event.type === 'file_create');
  assert.equal(creates.length, 0);
});

test('does not synthesize fallback tool events when disabled', () => {
  const parser = new TedParser({ enableSyntheticEvents: false });
  const events = collectEvents(parser);

  const toolCall = {
    type: 'tool_use',
    name: 'shell',
    input: {
      command: 'echo hello',
    },
  };

  feedJsonLine(parser, {
    type: 'message',
    ...BASE_EVENT,
    data: {
      role: 'assistant',
      content: JSON.stringify(toolCall),
    },
  });

  const commands = events.filter((event) => event.type === 'command');
  assert.equal(commands.length, 0);
});
