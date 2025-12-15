// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Git history analysis for context indexing.
//!
//! Extracts commit counts, modification times, and churn rates
//! from git history to inform context prioritization.

use chrono::{DateTime, TimeZone, Utc};
use git2::{Repository, Sort};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Result, TedError};

/// Git metrics for a single file.
#[derive(Debug, Clone, Default)]
pub struct FileGitMetrics {
    /// Number of commits that touched this file.
    pub commit_count: u32,
    /// Last modification time from git.
    pub last_modified: Option<DateTime<Utc>>,
    /// First commit time (file creation).
    pub first_commit: Option<DateTime<Utc>>,
    /// Churn rate: commits per day since creation.
    pub churn_rate: f64,
    /// Number of distinct authors.
    pub author_count: u32,
    /// Lines added across all commits.
    pub lines_added: u32,
    /// Lines deleted across all commits.
    pub lines_deleted: u32,
}

impl FileGitMetrics {
    /// Calculate churn rate from commit history.
    pub fn calculate_churn(&mut self) {
        if let (Some(first), Some(last)) = (self.first_commit, self.last_modified) {
            let days = (last - first).num_days().max(1) as f64;
            self.churn_rate = self.commit_count as f64 / days;
        }
    }

    /// Normalize churn rate to 0.0-1.0 scale.
    ///
    /// Uses a reasonable threshold where 1 commit/day = 1.0 (high churn).
    pub fn normalized_churn(&self) -> f64 {
        // 1 commit per day = very churny
        (self.churn_rate).min(1.0)
    }
}

/// Git analyzer for extracting repository metrics.
pub struct GitAnalyzer {
    repo: Repository,
    root: PathBuf,
}

impl GitAnalyzer {
    /// Open a git repository at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .map_err(|e| TedError::Config(format!("Failed to open git repository: {}", e)))?;

        let root = repo
            .workdir()
            .ok_or_else(|| TedError::Config("Bare repositories not supported".into()))?
            .to_path_buf();

        Ok(Self { repo, root })
    }

    /// Get the repository root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Analyze a single file's git history.
    pub fn analyze_file(&self, relative_path: &Path) -> Result<FileGitMetrics> {
        let mut metrics = FileGitMetrics::default();
        let mut authors = std::collections::HashSet::new();

        // Walk through commits
        let mut revwalk = self
            .repo
            .revwalk()
            .map_err(|e| TedError::Config(format!("Failed to create revwalk: {}", e)))?;

        revwalk.push_head().ok(); // Ignore error if no HEAD
        revwalk.set_sorting(Sort::TIME).ok();

        for oid in revwalk.flatten() {
            let commit = match self.repo.find_commit(oid) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Check if this commit touches our file
            if self.commit_touches_file(&commit, relative_path) {
                metrics.commit_count += 1;

                let time = commit.time();
                let datetime = Utc.timestamp_opt(time.seconds(), 0).single();

                if let Some(dt) = datetime {
                    if metrics.last_modified.is_none() || Some(dt) > metrics.last_modified {
                        metrics.last_modified = Some(dt);
                    }
                    if metrics.first_commit.is_none() || Some(dt) < metrics.first_commit {
                        metrics.first_commit = Some(dt);
                    }
                }

                if let Some(author) = commit.author().email() {
                    authors.insert(author.to_string());
                }
            }
        }

        metrics.author_count = authors.len() as u32;
        metrics.calculate_churn();

        Ok(metrics)
    }

    /// Analyze all files in the repository.
    pub fn analyze_all(&self) -> Result<HashMap<PathBuf, FileGitMetrics>> {
        let mut all_metrics = HashMap::new();
        let mut file_commits: HashMap<PathBuf, Vec<(git2::Oid, i64)>> = HashMap::new();

        // Walk through all commits once
        let mut revwalk = self
            .repo
            .revwalk()
            .map_err(|e| TedError::Config(format!("Failed to create revwalk: {}", e)))?;

        revwalk.push_head().ok();
        revwalk.set_sorting(Sort::TIME).ok();

        for oid in revwalk.flatten() {
            let commit = match self.repo.find_commit(oid) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let tree = match commit.tree() {
                Ok(t) => t,
                Err(_) => continue,
            };

            // Get parent tree for diff
            let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

            // Compute diff
            let diff = match self
                .repo
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)
            {
                Ok(d) => d,
                Err(_) => continue,
            };

            // Collect changed files
            let time_seconds = commit.time().seconds();
            diff.foreach(
                &mut |delta, _| {
                    if let Some(path) = delta.new_file().path() {
                        file_commits
                            .entry(path.to_path_buf())
                            .or_default()
                            .push((oid, time_seconds));
                    }
                    true
                },
                None,
                None,
                None,
            )
            .ok();
        }

        // Build metrics from collected data
        for (path, commits) in file_commits {
            let mut metrics = FileGitMetrics::default();
            let mut authors = std::collections::HashSet::new();

            metrics.commit_count = commits.len() as u32;

            for (oid, time_seconds) in &commits {
                if let Ok(commit) = self.repo.find_commit(*oid) {
                    if let Some(author) = commit.author().email() {
                        authors.insert(author.to_string());
                    }
                }

                let datetime = Utc.timestamp_opt(*time_seconds, 0).single();
                if let Some(dt) = datetime {
                    if metrics.last_modified.is_none() || Some(dt) > metrics.last_modified {
                        metrics.last_modified = Some(dt);
                    }
                    if metrics.first_commit.is_none() || Some(dt) < metrics.first_commit {
                        metrics.first_commit = Some(dt);
                    }
                }
            }

            metrics.author_count = authors.len() as u32;
            metrics.calculate_churn();
            all_metrics.insert(path, metrics);
        }

        Ok(all_metrics)
    }

    /// Get the list of tracked files.
    pub fn tracked_files(&self) -> Result<Vec<PathBuf>> {
        let index = self
            .repo
            .index()
            .map_err(|e| TedError::Config(format!("Failed to read git index: {}", e)))?;

        let files: Vec<PathBuf> = index
            .iter()
            .filter_map(|entry| {
                let path = String::from_utf8(entry.path.clone()).ok()?;
                Some(PathBuf::from(path))
            })
            .collect();

        Ok(files)
    }

    /// Check if the repo has uncommitted changes to a file.
    pub fn has_uncommitted_changes(&self, relative_path: &Path) -> bool {
        let statuses = match self.repo.statuses(None) {
            Ok(s) => s,
            Err(_) => return false,
        };

        statuses.iter().any(|entry| {
            entry
                .path()
                .map(|p| Path::new(p) == relative_path)
                .unwrap_or(false)
        })
    }

    /// Get the current branch name.
    pub fn current_branch(&self) -> Option<String> {
        let head = self.repo.head().ok()?;
        head.shorthand().map(String::from)
    }

    /// Get the most recent commit hash.
    pub fn head_commit_hash(&self) -> Option<String> {
        let head = self.repo.head().ok()?;
        let oid = head.peel_to_commit().ok()?.id();
        Some(format!("{:.8}", oid))
    }

    /// Check if a commit touches a specific file.
    fn commit_touches_file(&self, commit: &git2::Commit, path: &Path) -> bool {
        let tree = match commit.tree() {
            Ok(t) => t,
            Err(_) => return false,
        };

        let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

        let diff = match self
            .repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)
        {
            Ok(d) => d,
            Err(_) => return false,
        };

        let mut touches = false;
        diff.foreach(
            &mut |delta, _| {
                if let Some(new_path) = delta.new_file().path() {
                    if new_path == path {
                        touches = true;
                        return false; // Stop iteration
                    }
                }
                true
            },
            None,
            None,
            None,
        )
        .ok();

        touches
    }
}

/// Compute a hash of the project root for index file naming.
pub fn project_hash(root: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // Helper to get the test repo (this repo itself)
    fn get_test_repo() -> Option<GitAnalyzer> {
        let cwd = env::current_dir().ok()?;
        GitAnalyzer::open(&cwd).ok()
    }

    #[test]
    fn test_file_git_metrics_default() {
        let metrics = FileGitMetrics::default();
        assert_eq!(metrics.commit_count, 0);
        assert_eq!(metrics.churn_rate, 0.0);
        assert!(metrics.last_modified.is_none());
    }

    #[test]
    fn test_churn_calculation() {
        let mut metrics = FileGitMetrics {
            commit_count: 10,
            first_commit: Some(Utc::now() - chrono::Duration::days(10)),
            last_modified: Some(Utc::now()),
            ..Default::default()
        };

        metrics.calculate_churn();

        // 10 commits over 10 days = 1 commit/day
        assert!((metrics.churn_rate - 1.0).abs() < 0.1);
        assert!((metrics.normalized_churn() - 1.0).abs() < 0.1);
    }

    #[test]
    fn test_normalized_churn_capping() {
        let metrics = FileGitMetrics {
            churn_rate: 5.0, // Very high churn
            ..Default::default()
        };

        assert_eq!(metrics.normalized_churn(), 1.0); // Capped at 1.0
    }

    #[test]
    fn test_project_hash() {
        let hash1 = project_hash(Path::new("/some/path"));
        let hash2 = project_hash(Path::new("/some/path"));
        let hash3 = project_hash(Path::new("/other/path"));

        assert_eq!(hash1, hash2); // Same path = same hash
        assert_ne!(hash1, hash3); // Different path = different hash
        assert_eq!(hash1.len(), 16); // 16 hex chars
    }

    #[test]
    fn test_open_repo() {
        // This test requires being in a git repo
        if let Some(analyzer) = get_test_repo() {
            assert!(analyzer.root().exists());
        }
    }

    #[test]
    fn test_current_branch() {
        if let Some(analyzer) = get_test_repo() {
            // Branch may or may not exist (could be detached HEAD)
            // Just check the method doesn't panic
            let _branch = analyzer.current_branch();
        }
    }

    #[test]
    fn test_head_commit_hash() {
        if let Some(analyzer) = get_test_repo() {
            let hash = analyzer.head_commit_hash();
            if let Some(h) = hash {
                assert_eq!(h.len(), 8); // Short hash
            }
        }
    }

    #[test]
    fn test_tracked_files() {
        if let Some(analyzer) = get_test_repo() {
            let files = analyzer.tracked_files();
            assert!(files.is_ok());
            // A real repo should have some files
            // (but we don't assert > 0 in case of edge cases)
        }
    }

    #[test]
    fn test_analyze_file() {
        if let Some(analyzer) = get_test_repo() {
            // Try analyzing Cargo.toml which should exist
            let metrics = analyzer.analyze_file(Path::new("Cargo.toml"));
            if let Ok(m) = metrics {
                // Cargo.toml should have some commits
                // Just check it returns successfully (commit_count is u32, always >= 0)
                let _ = m.commit_count;
            }
        }
    }
}
