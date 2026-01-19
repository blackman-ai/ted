// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useCallback, useEffect } from 'react';
import { PendingChange } from '../components/DiffViewer';

/**
 * Hook for managing pending file changes
 *
 * When "Review Changes" mode is enabled, file changes are queued
 * instead of being applied immediately. Users can then accept or
 * reject individual changes or all changes at once.
 */
export function usePendingChanges() {
  const [pendingChanges, setPendingChanges] = useState<PendingChange[]>([]);
  const [reviewMode, setReviewModeState] = useState(true); // Default to review mode

  // Sync review mode with main process on mount
  useEffect(() => {
    window.teddy.getReviewMode().then(({ enabled }) => {
      setReviewModeState(enabled);
    });
  }, []);

  /**
   * Set review mode and sync with main process
   */
  const setReviewMode = useCallback(async (enabled: boolean) => {
    setReviewModeState(enabled);
    await window.teddy.setReviewMode(enabled);

    // If disabling review mode, clear pending changes
    if (!enabled) {
      setPendingChanges([]);
    }
  }, []);

  /**
   * Toggle review mode
   */
  const toggleReviewMode = useCallback(async () => {
    const newValue = !reviewMode;
    await setReviewMode(newValue);
  }, [reviewMode, setReviewMode]);

  /**
   * Add a new pending change
   */
  const addPendingChange = useCallback((change: Omit<PendingChange, 'id'>) => {
    const newChange: PendingChange = {
      ...change,
      id: `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
    };
    setPendingChanges((prev) => [...prev, newChange]);
    return newChange.id;
  }, []);

  /**
   * Accept a single change and apply it
   */
  const acceptChange = useCallback(async (changeId: string): Promise<boolean> => {
    const change = pendingChanges.find((c) => c.id === changeId);
    if (!change) return false;

    try {
      // Apply the change
      if (change.type === 'create') {
        await window.teddy.writeFile(change.path, change.newContent);
      } else if (change.type === 'edit') {
        await window.teddy.writeFile(change.path, change.newContent);
      } else if (change.type === 'delete') {
        await window.teddy.deleteFile(change.path);
      }

      // Remove from pending
      setPendingChanges((prev) => prev.filter((c) => c.id !== changeId));
      return true;
    } catch (err) {
      console.error(`Failed to apply change ${changeId}:`, err);
      return false;
    }
  }, [pendingChanges]);

  /**
   * Reject a single change (discard it)
   */
  const rejectChange = useCallback((changeId: string) => {
    setPendingChanges((prev) => prev.filter((c) => c.id !== changeId));
  }, []);

  /**
   * Accept all pending changes
   */
  const acceptAllChanges = useCallback(async (): Promise<boolean> => {
    let success = true;

    // Apply changes in order
    for (const change of pendingChanges) {
      try {
        if (change.type === 'create') {
          await window.teddy.writeFile(change.path, change.newContent);
        } else if (change.type === 'edit') {
          await window.teddy.writeFile(change.path, change.newContent);
        } else if (change.type === 'delete') {
          await window.teddy.deleteFile(change.path);
        }
      } catch (err) {
        console.error(`Failed to apply change ${change.id}:`, err);
        success = false;
      }
    }

    // Clear all pending changes
    setPendingChanges([]);
    return success;
  }, [pendingChanges]);

  /**
   * Reject all pending changes (discard all)
   */
  const rejectAllChanges = useCallback(() => {
    setPendingChanges([]);
  }, []);

  /**
   * Clear all pending changes (same as reject all)
   */
  const clearPendingChanges = useCallback(() => {
    setPendingChanges([]);
  }, []);

  return {
    pendingChanges,
    reviewMode,
    setReviewMode,
    toggleReviewMode,
    addPendingChange,
    acceptChange,
    rejectChange,
    acceptAllChanges,
    rejectAllChanges,
    clearPendingChanges,
    hasPendingChanges: pendingChanges.length > 0,
  };
}
