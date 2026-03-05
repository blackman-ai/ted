// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { spawn } from 'child_process';
import fs from 'fs/promises';
import os from 'os';
import path from 'path';
import { pathToFileURL } from 'url';
import { debugLog } from '../utils/logger';

export type HookEventName =
  | 'BeforeApplyChanges'
  | 'AfterApplyChanges'
  | 'OnShare'
  | 'OnDeploy';

export type HookDecision = 'allow' | 'deny' | 'ask';

export type HookOperationType = 'file_create' | 'file_edit' | 'file_delete';

export interface HookOperationEnvelope {
  type: HookOperationType;
  timestamp?: number;
  session_id?: string;
  data: Record<string, unknown>;
}

export type HookMatcherPattern =
  | string
  | {
      exact?: string;
      regex?: string;
    };

export interface HookMatcher {
  op?: HookMatcherPattern;
  path?: HookMatcherPattern;
  target?: HookMatcherPattern;
}

export interface HookCommandAction {
  type: 'command';
  command?: string;
  commands?: Record<string, string>;
  timeoutMs?: number;
  cwd?: string;
  env?: Record<string, string>;
}

export interface HookNodeAction {
  type: 'node';
  module: string;
  exportName?: string;
  timeoutMs?: number;
}

export type HookAction = HookCommandAction | HookNodeAction;

export interface HookRuleConfig {
  name?: string;
  enabled?: boolean;
  matcher?: HookMatcher;
  actions: HookAction[];
}

export interface HooksConfigFile {
  hooks?: Partial<Record<HookEventName, HookRuleConfig[]>>;
}

export interface LoadedHookRule extends HookRuleConfig {
  sourcePath: string;
  sourceDir: string;
}

export interface LoadedHooksConfig {
  hooks: Record<HookEventName, LoadedHookRule[]>;
  warnings: string[];
}

export interface HookTriggerPayload {
  event: HookEventName;
  projectRoot: string;
  timestamp?: number;
  sessionId?: string;
  operations?: HookOperationEnvelope[];
  target?: string;
  metadata?: Record<string, unknown>;
}

export interface HookNonBlockingResult {
  matchedRules: number;
  messages: string[];
  errors: string[];
  url?: string;
  artifacts?: Record<string, unknown>;
}

export interface HookBlockingResult extends HookNonBlockingResult {
  decision: HookDecision;
  reason?: string;
  updatedOps?: HookOperationEnvelope[];
}

export interface HookRunnerOptions {
  projectRoot: string;
  userConfigPaths?: string[];
  projectConfigPaths?: string[];
  defaultTimeoutMs?: number;
}

const SUPPORTED_EVENTS: HookEventName[] = [
  'BeforeApplyChanges',
  'AfterApplyChanges',
  'OnShare',
  'OnDeploy',
];

const DEFAULT_TIMEOUT_MS = 30_000;
const REGEX_META_CHARS = /[|*+?()[\]{}^$\\]/;

function emptyHooks(): Record<HookEventName, LoadedHookRule[]> {
  return {
    BeforeApplyChanges: [],
    AfterApplyChanges: [],
    OnShare: [],
    OnDeploy: [],
  };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isHookEventName(value: string): value is HookEventName {
  return SUPPORTED_EVENTS.includes(value as HookEventName);
}

function asHookDecision(value: unknown): HookDecision | undefined {
  if (value === 'allow' || value === 'deny' || value === 'ask') {
    return value;
  }
  return undefined;
}

function defaultUserConfigPaths(): string[] {
  const homeDir = os.homedir();
  return [
    path.join(homeDir, '.teddy', 'hooks.json'),
    path.join(homeDir, '.ted', 'hooks.json'),
  ];
}

function defaultProjectConfigPaths(projectRoot: string): string[] {
  return [
    path.join(projectRoot, '.teddy', 'hooks.json'),
    path.join(projectRoot, '.ted', 'hooks.json'),
  ];
}

function coerceStringMap(value: unknown): Record<string, string> | undefined {
  if (!isObject(value)) {
    return undefined;
  }
  const mapped: Record<string, string> = {};
  for (const [key, item] of Object.entries(value)) {
    if (typeof item === 'string' && item.trim().length > 0) {
      mapped[key] = item;
    }
  }
  if (Object.keys(mapped).length === 0) {
    return undefined;
  }
  return mapped;
}

function coerceMatcher(value: unknown): HookMatcher | undefined {
  if (!isObject(value)) {
    return undefined;
  }
  const matcher: HookMatcher = {};
  if ('op' in value) {
    matcher.op = value.op as HookMatcherPattern;
  }
  if ('path' in value) {
    matcher.path = value.path as HookMatcherPattern;
  }
  if ('target' in value) {
    matcher.target = value.target as HookMatcherPattern;
  }
  return matcher;
}

function coerceCommandAction(action: Record<string, unknown>): HookCommandAction | null {
  const directCommand =
    typeof action.command === 'string' && action.command.trim().length > 0
      ? action.command
      : undefined;
  const commands = coerceStringMap(action.commands);

  if (!directCommand && !commands) {
    return null;
  }

  const parsed: HookCommandAction = {
    type: 'command',
    command: directCommand,
    commands,
  };

  if (typeof action.timeoutMs === 'number' && Number.isFinite(action.timeoutMs)) {
    parsed.timeoutMs = Math.max(1, Math.floor(action.timeoutMs));
  }
  if (typeof action.cwd === 'string' && action.cwd.trim().length > 0) {
    parsed.cwd = action.cwd;
  }
  const env = coerceStringMap(action.env);
  if (env) {
    parsed.env = env;
  }
  return parsed;
}

function coerceNodeAction(action: Record<string, unknown>): HookNodeAction | null {
  const modulePath =
    typeof action.module === 'string' && action.module.trim().length > 0
      ? action.module
      : null;
  if (!modulePath) {
    return null;
  }

  const parsed: HookNodeAction = {
    type: 'node',
    module: modulePath,
  };

  if (typeof action.exportName === 'string' && action.exportName.trim().length > 0) {
    parsed.exportName = action.exportName;
  }
  if (typeof action.timeoutMs === 'number' && Number.isFinite(action.timeoutMs)) {
    parsed.timeoutMs = Math.max(1, Math.floor(action.timeoutMs));
  }
  return parsed;
}

function coerceAction(value: unknown): HookAction | null {
  if (!isObject(value) || typeof value.type !== 'string') {
    return null;
  }
  if (value.type === 'command') {
    return coerceCommandAction(value);
  }
  if (value.type === 'node') {
    return coerceNodeAction(value);
  }
  return null;
}

function coerceRule(value: unknown, sourcePath: string): LoadedHookRule | null {
  if (!isObject(value)) {
    return null;
  }

  const actionsRaw = Array.isArray(value.actions) ? value.actions : [];
  const actions = actionsRaw
    .map((action) => coerceAction(action))
    .filter((action): action is HookAction => action !== null);

  if (actions.length === 0) {
    return null;
  }

  const rule: LoadedHookRule = {
    sourcePath,
    sourceDir: path.dirname(sourcePath),
    actions,
  };

  if (typeof value.name === 'string' && value.name.trim().length > 0) {
    rule.name = value.name;
  }
  if (typeof value.enabled === 'boolean') {
    rule.enabled = value.enabled;
  }
  const matcher = coerceMatcher(value.matcher);
  if (matcher) {
    rule.matcher = matcher;
  }

  return rule;
}

function parsePatternDescriptor(pattern: HookMatcherPattern): {
  kind: 'exact' | 'regex';
  value: string;
} | null {
  if (typeof pattern === 'string') {
    const trimmed = pattern.trim();
    if (!trimmed) {
      return null;
    }
    if (trimmed.startsWith('re:') && trimmed.length > 3) {
      return { kind: 'regex', value: trimmed.slice(3) };
    }
    if (trimmed.startsWith('/') && trimmed.endsWith('/') && trimmed.length > 2) {
      return { kind: 'regex', value: trimmed.slice(1, -1) };
    }
    if (REGEX_META_CHARS.test(trimmed)) {
      return { kind: 'regex', value: trimmed };
    }
    return { kind: 'exact', value: trimmed };
  }

  if (isObject(pattern)) {
    if (typeof pattern.exact === 'string' && pattern.exact.trim().length > 0) {
      return { kind: 'exact', value: pattern.exact.trim() };
    }
    if (typeof pattern.regex === 'string' && pattern.regex.trim().length > 0) {
      return { kind: 'regex', value: pattern.regex.trim() };
    }
  }

  return null;
}

export function matchesHookPattern(
  candidate: string | undefined,
  pattern: HookMatcherPattern | undefined
): boolean {
  if (!pattern) {
    return true;
  }
  if (!candidate) {
    return false;
  }

  const descriptor = parsePatternDescriptor(pattern);
  if (!descriptor) {
    return false;
  }
  if (descriptor.kind === 'exact') {
    return candidate === descriptor.value;
  }

  try {
    return new RegExp(descriptor.value).test(candidate);
  } catch {
    return false;
  }
}

function opPath(op: HookOperationEnvelope): string | undefined {
  const rawPath = op.data.path;
  return typeof rawPath === 'string' ? rawPath : undefined;
}

export function ruleMatchesPayload(
  matcher: HookMatcher | undefined,
  payload: HookTriggerPayload
): boolean {
  if (!matcher) {
    return true;
  }

  const targetMatch = matchesHookPattern(payload.target, matcher.target);
  if (!targetMatch) {
    return false;
  }

  if (!matcher.op && !matcher.path) {
    return true;
  }

  const operations = payload.operations ?? [];
  if (operations.length === 0) {
    return false;
  }

  return operations.some((operation) => {
    const operationMatch = matchesHookPattern(operation.type, matcher.op);
    const pathMatch = matchesHookPattern(opPath(operation), matcher.path);
    return operationMatch && pathMatch;
  });
}

function normalizeOperations(value: unknown): HookOperationEnvelope[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }

  const normalized: HookOperationEnvelope[] = [];
  for (const item of value) {
    if (!isObject(item)) {
      continue;
    }
    if (
      item.type !== 'file_create' &&
      item.type !== 'file_edit' &&
      item.type !== 'file_delete'
    ) {
      continue;
    }
    if (!isObject(item.data)) {
      continue;
    }
    const operation: HookOperationEnvelope = {
      type: item.type,
      data: item.data,
    };
    if (typeof item.timestamp === 'number' && Number.isFinite(item.timestamp)) {
      operation.timestamp = item.timestamp;
    }
    if (typeof item.session_id === 'string' && item.session_id.trim().length > 0) {
      operation.session_id = item.session_id;
    }
    normalized.push(operation);
  }

  return normalized.length > 0 ? normalized : undefined;
}

interface ActionExecutionResult {
  ok: boolean;
  stdout: string;
  stderr: string;
  response?: Record<string, unknown>;
  error?: string;
}

interface HookResponseSummary {
  message?: string;
  url?: string;
  artifacts?: Record<string, unknown>;
  reason?: string;
  decision?: HookDecision;
  updatedOps?: HookOperationEnvelope[];
}

function parseStructuredOutput(stdout: string): Record<string, unknown> | undefined {
  const trimmed = stdout.trim();
  if (!trimmed) {
    return undefined;
  }

  try {
    const parsed = JSON.parse(trimmed);
    if (isObject(parsed)) {
      return parsed;
    }
  } catch {
    // Fall through to final-line parsing.
  }

  const lines = trimmed
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    try {
      const parsed = JSON.parse(lines[index]);
      if (isObject(parsed)) {
        return parsed;
      }
    } catch {
      // Continue.
    }
  }

  return undefined;
}

function summarizeResponse(response: Record<string, unknown>): HookResponseSummary {
  const summary: HookResponseSummary = {};

  if (typeof response.message === 'string' && response.message.trim().length > 0) {
    summary.message = response.message;
  }
  if (typeof response.url === 'string' && response.url.trim().length > 0) {
    summary.url = response.url;
  }
  if (isObject(response.artifacts)) {
    summary.artifacts = response.artifacts;
  }
  if (typeof response.reason === 'string' && response.reason.trim().length > 0) {
    summary.reason = response.reason;
  }
  const decision = asHookDecision(response.decision);
  if (decision) {
    summary.decision = decision;
  }

  const updatedOps = normalizeOperations(response.updatedOps);
  if (updatedOps) {
    summary.updatedOps = updatedOps;
  }

  return summary;
}

function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  timeoutMessage: string
): Promise<T> {
  let timeoutHandle: NodeJS.Timeout | null = null;
  const timeoutPromise = new Promise<T>((_, reject) => {
    timeoutHandle = setTimeout(() => reject(new Error(timeoutMessage)), timeoutMs);
  });

  return Promise.race([promise, timeoutPromise]).finally(() => {
    if (timeoutHandle) {
      clearTimeout(timeoutHandle);
    }
  }) as Promise<T>;
}

function resolveActionPath(baseDir: string, modulePath: string): string {
  if (path.isAbsolute(modulePath)) {
    return modulePath;
  }
  return path.resolve(baseDir, modulePath);
}

async function executeCommandAction(
  action: HookCommandAction,
  payload: HookTriggerPayload,
  sourceDir: string,
  timeoutMs: number
): Promise<ActionExecutionResult> {
  const platform = process.platform;
  const command =
    action.commands?.[platform] ??
    (platform === 'win32' ? action.commands?.windows : undefined) ??
    action.commands?.default ??
    action.command;

  if (!command) {
    return {
      ok: false,
      stdout: '',
      stderr: '',
      error: 'Hook command action missing command',
    };
  }

  const cwd = action.cwd
    ? resolveActionPath(sourceDir, action.cwd)
    : payload.projectRoot;

  const resolvedTimeout = action.timeoutMs ?? timeoutMs;

  return new Promise<ActionExecutionResult>((resolve) => {
    const child = spawn(command, {
      cwd,
      shell: true,
      windowsHide: true,
      env: {
        ...process.env,
        ...(action.env ?? {}),
      },
    });

    let stdout = '';
    let stderr = '';
    let timedOut = false;

    const timeoutHandle = setTimeout(() => {
      timedOut = true;
      child.kill('SIGTERM');
      setTimeout(() => {
        if (!child.killed) {
          child.kill('SIGKILL');
        }
      }, 500);
    }, resolvedTimeout);

    child.stdout?.on('data', (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr?.on('data', (chunk) => {
      stderr += chunk.toString();
    });

    child.on('error', (err) => {
      clearTimeout(timeoutHandle);
      resolve({
        ok: false,
        stdout,
        stderr,
        error: `Hook command failed to start: ${err.message}`,
      });
    });

    child.on('close', (code) => {
      clearTimeout(timeoutHandle);

      if (timedOut) {
        resolve({
          ok: false,
          stdout,
          stderr,
          error: `Hook command timed out after ${resolvedTimeout}ms`,
        });
        return;
      }

      if (code !== 0) {
        resolve({
          ok: false,
          stdout,
          stderr,
          error: `Hook command exited with code ${code}`,
        });
        return;
      }

      const response = parseStructuredOutput(stdout);
      resolve({
        ok: true,
        stdout,
        stderr,
        response,
      });
    });

    try {
      child.stdin?.write(JSON.stringify(payload));
      child.stdin?.end();
    } catch (err) {
      clearTimeout(timeoutHandle);
      resolve({
        ok: false,
        stdout,
        stderr,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  });
}

async function executeNodeAction(
  action: HookNodeAction,
  payload: HookTriggerPayload,
  sourceDir: string,
  timeoutMs: number
): Promise<ActionExecutionResult> {
  try {
    const modulePath = resolveActionPath(sourceDir, action.module);
    const imported = await import(pathToFileURL(modulePath).href);
    const exportName = action.exportName ?? 'default';
    const handler = imported[exportName];
    if (typeof handler !== 'function') {
      return {
        ok: false,
        stdout: '',
        stderr: '',
        error: `Hook node action export '${exportName}' is not a function`,
      };
    }

    const resolvedTimeout = action.timeoutMs ?? timeoutMs;
    const rawResult = await withTimeout(
      Promise.resolve(handler(payload)),
      resolvedTimeout,
      `Hook node action timed out after ${resolvedTimeout}ms`
    );

    if (isObject(rawResult)) {
      return {
        ok: true,
        stdout: JSON.stringify(rawResult),
        stderr: '',
        response: rawResult,
      };
    }
    if (typeof rawResult === 'string' && rawResult.trim().length > 0) {
      return {
        ok: true,
        stdout: rawResult,
        stderr: '',
        response: { message: rawResult },
      };
    }
    return {
      ok: true,
      stdout: '',
      stderr: '',
      response: {},
    };
  } catch (err) {
    return {
      ok: false,
      stdout: '',
      stderr: '',
      error: err instanceof Error ? err.message : String(err),
    };
  }
}

export async function loadHooksConfig(options: HookRunnerOptions): Promise<LoadedHooksConfig> {
  const loaded: LoadedHooksConfig = {
    hooks: emptyHooks(),
    warnings: [],
  };

  const userPaths = options.userConfigPaths ?? defaultUserConfigPaths();
  const projectPaths =
    options.projectConfigPaths ?? defaultProjectConfigPaths(options.projectRoot);
  const candidatePaths = [...userPaths, ...projectPaths];

  for (const configPath of candidatePaths) {
    let content: string;
    try {
      content = await fs.readFile(configPath, 'utf-8');
    } catch {
      continue;
    }

    let parsed: unknown;
    try {
      parsed = JSON.parse(content);
    } catch (err) {
      loaded.warnings.push(
        `Failed to parse hooks config '${configPath}': ${
          err instanceof Error ? err.message : String(err)
        }`
      );
      continue;
    }

    if (!isObject(parsed) || !isObject(parsed.hooks)) {
      loaded.warnings.push(`Invalid hooks config structure in '${configPath}'`);
      continue;
    }

    for (const [eventName, rulesRaw] of Object.entries(parsed.hooks)) {
      if (!isHookEventName(eventName)) {
        loaded.warnings.push(
          `Unsupported hook event '${eventName}' in '${configPath}'`
        );
        continue;
      }
      if (!Array.isArray(rulesRaw)) {
        loaded.warnings.push(
          `Hooks for '${eventName}' must be an array in '${configPath}'`
        );
        continue;
      }

      for (const rawRule of rulesRaw) {
        const rule = coerceRule(rawRule, configPath);
        if (!rule) {
          loaded.warnings.push(
            `Skipping invalid hook rule in '${configPath}' for event '${eventName}'`
          );
          continue;
        }
        if (rule.enabled === false) {
          continue;
        }
        loaded.hooks[eventName].push(rule);
      }
    }
  }

  return loaded;
}

export class HookRunner {
  private readonly options: HookRunnerOptions;

  constructor(options: HookRunnerOptions) {
    this.options = options;
  }

  private async executeAction(
    action: HookAction,
    payload: HookTriggerPayload,
    sourceDir: string
  ): Promise<ActionExecutionResult> {
    const defaultTimeout = this.options.defaultTimeoutMs ?? DEFAULT_TIMEOUT_MS;
    if (action.type === 'command') {
      return executeCommandAction(action, payload, sourceDir, defaultTimeout);
    }
    return executeNodeAction(action, payload, sourceDir, defaultTimeout);
  }

  private async runBlockingEvent(payload: HookTriggerPayload): Promise<HookBlockingResult> {
    const config = await loadHooksConfig(this.options);
    const rules = config.hooks[payload.event];
    const result: HookBlockingResult = {
      decision: 'allow',
      matchedRules: 0,
      messages: [],
      errors: [...config.warnings],
    };

    if (rules.length === 0) {
      return result;
    }

    const mutablePayload: HookTriggerPayload = {
      ...payload,
      timestamp: payload.timestamp ?? Date.now(),
      operations: payload.operations ? [...payload.operations] : undefined,
    };

    for (const rule of rules) {
      if (!ruleMatchesPayload(rule.matcher, mutablePayload)) {
        continue;
      }

      result.matchedRules += 1;
      for (const action of rule.actions) {
        const actionResult = await this.executeAction(action, mutablePayload, rule.sourceDir);
        if (!actionResult.ok) {
          const reason = actionResult.error ?? 'Hook action failed';
          result.errors.push(reason);
          result.decision = 'deny';
          result.reason = `Blocking hook failed: ${reason}`;
          return result;
        }

        if (!actionResult.response) {
          result.decision = 'deny';
          result.reason =
            'Blocking hook action must emit JSON response on stdout';
          result.errors.push(result.reason);
          return result;
        }

        const summary = summarizeResponse(actionResult.response);
        if (summary.message) {
          result.messages.push(summary.message);
        }
        if (summary.url && !result.url) {
          result.url = summary.url;
        }
        if (summary.artifacts) {
          result.artifacts = {
            ...(result.artifacts ?? {}),
            ...summary.artifacts,
          };
        }
        if (summary.updatedOps) {
          mutablePayload.operations = summary.updatedOps;
          result.updatedOps = summary.updatedOps;
        }

        if (summary.decision === 'deny') {
          result.decision = 'deny';
          result.reason = summary.reason ?? 'Blocked by BeforeApplyChanges hook';
          return result;
        }
        if (summary.decision === 'ask') {
          result.decision = 'ask';
          result.reason =
            summary.reason ?? 'BeforeApplyChanges hook requires confirmation';
          return result;
        }
      }
    }

    return result;
  }

  private async runNonBlockingEvent(
    payload: HookTriggerPayload
  ): Promise<HookNonBlockingResult> {
    const config = await loadHooksConfig(this.options);
    const rules = config.hooks[payload.event];
    const result: HookNonBlockingResult = {
      matchedRules: 0,
      messages: [],
      errors: [...config.warnings],
    };

    if (rules.length === 0) {
      return result;
    }

    const mutablePayload: HookTriggerPayload = {
      ...payload,
      timestamp: payload.timestamp ?? Date.now(),
      operations: payload.operations ? [...payload.operations] : undefined,
    };

    for (const rule of rules) {
      if (!ruleMatchesPayload(rule.matcher, mutablePayload)) {
        continue;
      }
      result.matchedRules += 1;

      for (const action of rule.actions) {
        const actionResult = await this.executeAction(action, mutablePayload, rule.sourceDir);
        if (!actionResult.ok) {
          const reason = actionResult.error ?? 'Hook action failed';
          result.errors.push(reason);
          continue;
        }

        if (actionResult.response) {
          const summary = summarizeResponse(actionResult.response);
          if (summary.message) {
            result.messages.push(summary.message);
          }
          if (summary.url && !result.url) {
            result.url = summary.url;
          }
          if (summary.artifacts) {
            result.artifacts = {
              ...(result.artifacts ?? {}),
              ...summary.artifacts,
            };
          }
          if (summary.updatedOps) {
            mutablePayload.operations = summary.updatedOps;
          }
          continue;
        }

        const fallbackMessage = actionResult.stdout.trim();
        if (fallbackMessage) {
          result.messages.push(fallbackMessage);
        }
      }
    }

    return result;
  }

  async runBeforeApply(
    payload: Omit<HookTriggerPayload, 'event'>
  ): Promise<HookBlockingResult> {
    return this.runBlockingEvent({
      ...payload,
      event: 'BeforeApplyChanges',
      timestamp: payload.timestamp ?? Date.now(),
    });
  }

  async runAfterApply(
    payload: Omit<HookTriggerPayload, 'event'>
  ): Promise<HookNonBlockingResult> {
    return this.runNonBlockingEvent({
      ...payload,
      event: 'AfterApplyChanges',
      timestamp: payload.timestamp ?? Date.now(),
    });
  }

  async runOnShare(
    payload: Omit<HookTriggerPayload, 'event'>
  ): Promise<HookNonBlockingResult> {
    return this.runNonBlockingEvent({
      ...payload,
      event: 'OnShare',
      timestamp: payload.timestamp ?? Date.now(),
    });
  }

  async runOnDeploy(
    payload: Omit<HookTriggerPayload, 'event'>
  ): Promise<HookNonBlockingResult> {
    return this.runNonBlockingEvent({
      ...payload,
      event: 'OnDeploy',
      timestamp: payload.timestamp ?? Date.now(),
    });
  }
}

export function logHookResult(scope: string, result: HookNonBlockingResult): void {
  if (result.matchedRules > 0) {
    debugLog(`HOOK:${scope}`, `Matched ${result.matchedRules} rule(s)`);
  }
  for (const message of result.messages) {
    debugLog(`HOOK:${scope}`, message);
  }
  for (const err of result.errors) {
    debugLog(`HOOK:${scope}:ERROR`, err);
  }
}
