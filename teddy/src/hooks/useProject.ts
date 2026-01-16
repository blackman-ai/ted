// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect } from 'react';

export interface Project {
  path: string;
  name: string;
}

export function useProject() {
  const [project, setProjectState] = useState<Project | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // Load current project from Electron
    window.teddy.getProject().then((result) => {
      if (result.hasProject && result.path) {
        const name = result.path.split('/').pop() || 'Project';
        setProjectState({ path: result.path, name });
      }
      setLoading(false);
    });
  }, []);

  const setProject = async (path: string | null) => {
    if (path === null) {
      setProjectState(null);
      return;
    }

    const name = path.split('/').pop() || 'Project';
    await window.teddy.setProject(path);
    setProjectState({ path, name });
  };

  return {
    project,
    setProject,
    loading,
  };
}
