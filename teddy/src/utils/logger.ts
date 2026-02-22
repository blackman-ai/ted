// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

function isDebugEnabled(): boolean {
  const importMeta = import.meta as ImportMeta & {
    env?: {
      DEV?: boolean;
    };
  };

  if (importMeta.env?.DEV) {
    return true;
  }

  try {
    return window.localStorage.getItem('teddy:debug') === '1';
  } catch {
    return false;
  }
}

export function debugLog(scope: string, ...args: unknown[]): void {
  if (!isDebugEnabled()) {
    return;
  }

  console.log(`[${scope}]`, ...args);
}
