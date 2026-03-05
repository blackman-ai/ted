// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  HookTriggerPayload,
  loadHooksConfig,
  matchesHookPattern,
  ruleMatchesPayload,
} from '../electron/hooks/runner';

test('matchesHookPattern supports exact and regex matching', () => {
  assert.equal(matchesHookPattern('file_edit', 'file_edit'), true);
  assert.equal(matchesHookPattern('file_create', 'file_edit'), false);

  assert.equal(matchesHookPattern('file_create', 'file_(create|edit)'), true);
  assert.equal(matchesHookPattern('src/app.ts', { regex: '^src/.*\\.ts$' }), true);
  assert.equal(matchesHookPattern('src/app.ts', { exact: 'src/main.ts' }), false);
});

test('ruleMatchesPayload filters by op, path, and target', () => {
  const payload: HookTriggerPayload = {
    event: 'BeforeApplyChanges',
    projectRoot: '/tmp/project',
    target: 'vercel',
    operations: [
      {
        type: 'file_edit',
        data: {
          path: 'src/App.tsx',
          operation: 'replace',
        },
      },
    ],
  };

  assert.equal(
    ruleMatchesPayload(
      {
        op: 'file_(create|edit)',
        path: 'src/.*',
        target: 'vercel',
      },
      payload
    ),
    true
  );

  assert.equal(
    ruleMatchesPayload(
      {
        op: 'file_delete',
        path: 'src/.*',
        target: 'vercel',
      },
      payload
    ),
    false
  );

  assert.equal(
    ruleMatchesPayload(
      {
        op: 'file_(create|edit)',
        path: 'src/.*',
        target: 'netlify',
      },
      payload
    ),
    false
  );
});

test('loadHooksConfig merges user and project config files and skips invalid rules', async () => {
  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'teddy-hooks-test-'));
  const projectRoot = path.join(tempDir, 'project');
  const userConfigPath = path.join(tempDir, 'user-hooks.json');
  const projectConfigPath = path.join(tempDir, 'project-hooks.json');

  await fs.mkdir(projectRoot, { recursive: true });

  const userConfig = {
    hooks: {
      BeforeApplyChanges: [
        {
          name: 'protect-source',
          matcher: {
            op: 'file_(create|edit)',
            path: 'src/.*',
          },
          actions: [
            {
              type: 'command',
              command: 'echo ok',
            },
          ],
        },
        {
          name: 'invalid-rule',
          matcher: {
            op: 'file_edit',
          },
        },
      ],
      UnknownEvent: [
        {
          actions: [
            {
              type: 'command',
              command: 'echo ignored',
            },
          ],
        },
      ],
    },
  };

  const projectConfig = {
    hooks: {
      AfterApplyChanges: [
        {
          name: 'post-apply',
          matcher: {
            path: { regex: 'src/.*' },
          },
          actions: [
            {
              type: 'command',
              command: 'echo done',
            },
          ],
        },
      ],
    },
  };

  await fs.writeFile(userConfigPath, JSON.stringify(userConfig, null, 2), 'utf-8');
  await fs.writeFile(projectConfigPath, JSON.stringify(projectConfig, null, 2), 'utf-8');

  const loaded = await loadHooksConfig({
    projectRoot,
    userConfigPaths: [userConfigPath],
    projectConfigPaths: [projectConfigPath],
  });

  assert.equal(loaded.hooks.BeforeApplyChanges.length, 1);
  assert.equal(loaded.hooks.BeforeApplyChanges[0].name, 'protect-source');
  assert.equal(loaded.hooks.AfterApplyChanges.length, 1);
  assert.equal(loaded.hooks.AfterApplyChanges[0].name, 'post-apply');

  assert.equal(
    loaded.warnings.some((warning) => warning.includes("Unsupported hook event 'UnknownEvent'")),
    true
  );
  assert.equal(
    loaded.warnings.some((warning) => warning.includes('Skipping invalid hook rule')),
    true
  );

  await fs.rm(tempDir, { recursive: true, force: true });
});
