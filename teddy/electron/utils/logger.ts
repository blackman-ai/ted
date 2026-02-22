// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

const DEBUG_ENABLED = process.env.NODE_ENV !== 'production' || process.env.TEDDY_DEBUG === '1';

export function debugLog(scope: string, ...args: unknown[]): void {
  if (!DEBUG_ENABLED) {
    return;
  }

  console.log(`[${scope}]`, ...args);
}
