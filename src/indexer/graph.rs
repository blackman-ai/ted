// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Dependency graph building and centrality scoring.
//!
//! This module builds a graph of file dependencies and calculates
//! PageRank-style centrality scores to identify core/central files.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::languages::ParserRegistry;
use crate::error::Result;

/// A node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// File path (relative to project root).
    pub path: PathBuf,
    /// Files this node imports (outgoing edges).
    pub dependencies: Vec<PathBuf>,
    /// Files that import this node (incoming edges).
    pub dependents: Vec<PathBuf>,
    /// PageRank-style centrality score (0.0 to 1.0).
    pub centrality: f64,
    /// Number of resolved imports.
    pub import_count: usize,
    /// Number of unresolved imports (external deps).
    pub unresolved_count: usize,
}

impl GraphNode {
    /// Create a new graph node.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            centrality: 0.0,
            import_count: 0,
            unresolved_count: 0,
        }
    }

    /// Total number of connections (in + out).
    pub fn degree(&self) -> usize {
        self.dependencies.len() + self.dependents.len()
    }

    /// In-degree (number of dependents).
    pub fn in_degree(&self) -> usize {
        self.dependents.len()
    }

    /// Out-degree (number of dependencies).
    pub fn out_degree(&self) -> usize {
        self.dependencies.len()
    }
}

/// The dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// Nodes indexed by file path.
    nodes: HashMap<PathBuf, GraphNode>,
    /// Project root for path resolution.
    #[serde(skip)]
    project_root: PathBuf,
}

impl DependencyGraph {
    /// Create a new empty graph.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            nodes: HashMap::new(),
            project_root,
        }
    }

    /// Get the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the total number of edges.
    pub fn edge_count(&self) -> usize {
        self.nodes.values().map(|n| n.dependencies.len()).sum()
    }

    /// Get a node by path.
    pub fn get_node(&self, path: &Path) -> Option<&GraphNode> {
        self.nodes.get(path)
    }

    /// Get a mutable node by path.
    pub fn get_node_mut(&mut self, path: &Path) -> Option<&mut GraphNode> {
        self.nodes.get_mut(path)
    }

    /// Add or get a node for a file path.
    pub fn ensure_node(&mut self, path: PathBuf) -> &mut GraphNode {
        self.nodes
            .entry(path.clone())
            .or_insert_with(|| GraphNode::new(path))
    }

    /// Add an edge from `from` to `to`.
    pub fn add_edge(&mut self, from: PathBuf, to: PathBuf) {
        // Add dependency
        let from_node = self.ensure_node(from.clone());
        if !from_node.dependencies.contains(&to) {
            from_node.dependencies.push(to.clone());
        }

        // Add dependent
        let to_node = self.ensure_node(to);
        if !to_node.dependents.contains(&from) {
            to_node.dependents.push(from);
        }
    }

    /// Build the graph from a set of files.
    pub fn build_from_files<'a>(
        &mut self,
        files: impl Iterator<Item = (&'a Path, &'a str)>,
        registry: &ParserRegistry,
    ) -> Result<GraphStats> {
        let mut stats = GraphStats::default();

        for (path, content) in files {
            stats.files_processed += 1;

            // Ensure node exists
            let relative_path = path
                .strip_prefix(&self.project_root)
                .unwrap_or(path)
                .to_path_buf();

            self.ensure_node(relative_path.clone());

            // Parse imports
            let imports = registry.parse_imports(path, content);
            stats.imports_found += imports.len();

            // Get the parser to resolve imports
            if let Some(parser) = registry.parser_for_path(path) {
                for import in &imports {
                    if let Some(resolved) =
                        parser.resolve_import(import, &relative_path, &self.project_root)
                    {
                        self.add_edge(relative_path.clone(), resolved);
                        stats.imports_resolved += 1;
                    }
                }
            }

            // Update node stats
            if let Some(node) = self.get_node_mut(&relative_path) {
                node.import_count = imports.len();
                node.unresolved_count = imports.len() - stats.imports_resolved;
            }
        }

        // Calculate centrality
        self.calculate_centrality();

        Ok(stats)
    }

    /// Update a single file in the graph.
    pub fn update_file(
        &mut self,
        path: &Path,
        content: &str,
        registry: &ParserRegistry,
    ) -> Result<()> {
        let relative_path = path
            .strip_prefix(&self.project_root)
            .unwrap_or(path)
            .to_path_buf();

        // Remove old edges from this file
        if let Some(node) = self.nodes.get(&relative_path) {
            let old_deps = node.dependencies.clone();
            for dep in old_deps {
                if let Some(dep_node) = self.nodes.get_mut(&dep) {
                    dep_node.dependents.retain(|p| p != &relative_path);
                }
            }
        }

        // Clear and rebuild edges for this file
        if let Some(node) = self.nodes.get_mut(&relative_path) {
            node.dependencies.clear();
        }

        // Parse new imports
        let imports = registry.parse_imports(path, content);

        if let Some(parser) = registry.parser_for_path(path) {
            for import in &imports {
                if let Some(resolved) =
                    parser.resolve_import(import, &relative_path, &self.project_root)
                {
                    self.add_edge(relative_path.clone(), resolved);
                }
            }
        }

        // Update node stats
        if let Some(node) = self.get_node_mut(&relative_path) {
            let resolved = node.dependencies.len();
            node.import_count = imports.len();
            node.unresolved_count = imports.len().saturating_sub(resolved);
        }

        // Recalculate centrality
        self.calculate_centrality();

        Ok(())
    }

    /// Remove a file from the graph.
    pub fn remove_file(&mut self, path: &Path) {
        let relative_path = path
            .strip_prefix(&self.project_root)
            .unwrap_or(path)
            .to_path_buf();

        // Remove edges pointing to this file
        if let Some(node) = self.nodes.remove(&relative_path) {
            // Remove from dependents of our dependencies
            for dep in &node.dependencies {
                if let Some(dep_node) = self.nodes.get_mut(dep) {
                    dep_node.dependents.retain(|p| p != &relative_path);
                }
            }

            // Remove from dependencies of our dependents
            for dep in &node.dependents {
                if let Some(dep_node) = self.nodes.get_mut(dep) {
                    dep_node.dependencies.retain(|p| p != &relative_path);
                }
            }
        }
    }

    /// Calculate PageRank-style centrality for all nodes.
    ///
    /// Uses the power iteration method with damping factor 0.85.
    pub fn calculate_centrality(&mut self) {
        let n = self.nodes.len();
        if n == 0 {
            return;
        }

        let damping = 0.85;
        let max_iterations = 100;
        let tolerance = 1e-6;

        // Initialize scores uniformly
        let initial_score = 1.0 / n as f64;
        for node in self.nodes.values_mut() {
            node.centrality = initial_score;
        }

        // Power iteration
        for _ in 0..max_iterations {
            let mut new_scores: HashMap<PathBuf, f64> = HashMap::new();
            let mut max_diff = 0.0f64;

            for (path, node) in &self.nodes {
                // Sum contributions from dependents
                let mut score = (1.0 - damping) / n as f64;

                for dependent_path in &node.dependents {
                    if let Some(dependent) = self.nodes.get(dependent_path) {
                        let out_degree = dependent.dependencies.len();
                        if out_degree > 0 {
                            score += damping * dependent.centrality / out_degree as f64;
                        }
                    }
                }

                new_scores.insert(path.clone(), score);
            }

            // Update scores and check convergence
            for (path, new_score) in new_scores {
                if let Some(node) = self.nodes.get_mut(&path) {
                    let diff = (new_score - node.centrality).abs();
                    max_diff = max_diff.max(diff);
                    node.centrality = new_score;
                }
            }

            if max_diff < tolerance {
                break;
            }
        }

        // Normalize to 0.0-1.0 range
        let max_centrality = self
            .nodes
            .values()
            .map(|n| n.centrality)
            .fold(0.0f64, f64::max);

        if max_centrality > 0.0 {
            for node in self.nodes.values_mut() {
                node.centrality /= max_centrality;
            }
        }
    }

    /// Get nodes sorted by centrality (highest first).
    pub fn nodes_by_centrality(&self) -> Vec<&GraphNode> {
        let mut nodes: Vec<_> = self.nodes.values().collect();
        nodes.sort_by(|a, b| {
            b.centrality
                .partial_cmp(&a.centrality)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        nodes
    }

    /// Get the most central files.
    pub fn top_central(&self, n: usize) -> Vec<&GraphNode> {
        self.nodes_by_centrality().into_iter().take(n).collect()
    }

    /// Get files with no dependents (leaf files).
    pub fn leaf_files(&self) -> Vec<&GraphNode> {
        self.nodes
            .values()
            .filter(|n| n.dependents.is_empty())
            .collect()
    }

    /// Get files with no dependencies (root files).
    pub fn root_files(&self) -> Vec<&GraphNode> {
        self.nodes
            .values()
            .filter(|n| n.dependencies.is_empty())
            .collect()
    }

    /// Find all files reachable from a given file (transitive dependencies).
    pub fn transitive_dependencies(&self, path: &Path) -> HashSet<PathBuf> {
        let mut visited = HashSet::new();
        let mut stack = vec![path.to_path_buf()];

        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            if let Some(node) = self.nodes.get(&current) {
                for dep in &node.dependencies {
                    if !visited.contains(dep) {
                        stack.push(dep.clone());
                    }
                }
            }
        }

        visited.remove(path);
        visited
    }

    /// Find all files that depend on a given file (transitive dependents).
    pub fn transitive_dependents(&self, path: &Path) -> HashSet<PathBuf> {
        let mut visited = HashSet::new();
        let mut stack = vec![path.to_path_buf()];

        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            if let Some(node) = self.nodes.get(&current) {
                for dep in &node.dependents {
                    if !visited.contains(dep) {
                        stack.push(dep.clone());
                    }
                }
            }
        }

        visited.remove(path);
        visited
    }

    /// Detect cycles in the graph.
    pub fn find_cycles(&self) -> Vec<Vec<PathBuf>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path_stack = Vec::new();

        for start in self.nodes.keys() {
            if !visited.contains(start) {
                self.dfs_cycles(
                    start,
                    &mut visited,
                    &mut rec_stack,
                    &mut path_stack,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn dfs_cycles(
        &self,
        node: &PathBuf,
        visited: &mut HashSet<PathBuf>,
        rec_stack: &mut HashSet<PathBuf>,
        path_stack: &mut Vec<PathBuf>,
        cycles: &mut Vec<Vec<PathBuf>>,
    ) {
        visited.insert(node.clone());
        rec_stack.insert(node.clone());
        path_stack.push(node.clone());

        if let Some(graph_node) = self.nodes.get(node) {
            for dep in &graph_node.dependencies {
                if !visited.contains(dep) {
                    self.dfs_cycles(dep, visited, rec_stack, path_stack, cycles);
                } else if rec_stack.contains(dep) {
                    // Found a cycle
                    if let Some(start_idx) = path_stack.iter().position(|p| p == dep) {
                        let cycle: Vec<_> = path_stack[start_idx..].to_vec();
                        cycles.push(cycle);
                    }
                }
            }
        }

        rec_stack.remove(node);
        path_stack.pop();
    }

    /// Get all nodes.
    pub fn nodes(&self) -> impl Iterator<Item = &GraphNode> {
        self.nodes.values()
    }

    /// Get centrality score for a path.
    pub fn centrality(&self, path: &Path) -> f64 {
        self.nodes.get(path).map(|n| n.centrality).unwrap_or(0.0)
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new(PathBuf::new())
    }
}

/// Statistics from graph building.
#[derive(Debug, Clone, Default)]
pub struct GraphStats {
    /// Number of files processed.
    pub files_processed: usize,
    /// Total imports found.
    pub imports_found: usize,
    /// Imports successfully resolved to local files.
    pub imports_resolved: usize,
}

impl GraphStats {
    /// Resolution rate as a percentage.
    pub fn resolution_rate(&self) -> f64 {
        if self.imports_found == 0 {
            100.0
        } else {
            (self.imports_resolved as f64 / self.imports_found as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_node_creation() {
        let node = GraphNode::new(PathBuf::from("src/main.rs"));
        assert_eq!(node.path, PathBuf::from("src/main.rs"));
        assert!(node.dependencies.is_empty());
        assert!(node.dependents.is_empty());
        assert_eq!(node.centrality, 0.0);
    }

    #[test]
    fn test_graph_node_degree() {
        let mut node = GraphNode::new(PathBuf::from("src/lib.rs"));
        node.dependencies.push(PathBuf::from("src/utils.rs"));
        node.dependencies.push(PathBuf::from("src/config.rs"));
        node.dependents.push(PathBuf::from("src/main.rs"));

        assert_eq!(node.out_degree(), 2);
        assert_eq!(node.in_degree(), 1);
        assert_eq!(node.degree(), 3);
    }

    #[test]
    fn test_graph_add_edge() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));

        graph.add_edge(PathBuf::from("src/main.rs"), PathBuf::from("src/lib.rs"));

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);

        let main_node = graph.get_node(Path::new("src/main.rs")).unwrap();
        assert!(main_node
            .dependencies
            .contains(&PathBuf::from("src/lib.rs")));

        let lib_node = graph.get_node(Path::new("src/lib.rs")).unwrap();
        assert!(lib_node.dependents.contains(&PathBuf::from("src/main.rs")));
    }

    #[test]
    fn test_graph_calculate_centrality() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));

        // Create a simple graph: A -> B -> C, A -> C
        graph.add_edge(PathBuf::from("a.rs"), PathBuf::from("b.rs"));
        graph.add_edge(PathBuf::from("b.rs"), PathBuf::from("c.rs"));
        graph.add_edge(PathBuf::from("a.rs"), PathBuf::from("c.rs"));

        graph.calculate_centrality();

        // C should have highest centrality (most dependents)
        let c_centrality = graph.centrality(Path::new("c.rs"));
        let a_centrality = graph.centrality(Path::new("a.rs"));

        assert!(c_centrality > a_centrality);
    }

    #[test]
    fn test_top_central() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));

        graph.add_edge(PathBuf::from("a.rs"), PathBuf::from("core.rs"));
        graph.add_edge(PathBuf::from("b.rs"), PathBuf::from("core.rs"));
        graph.add_edge(PathBuf::from("c.rs"), PathBuf::from("core.rs"));
        graph.add_edge(PathBuf::from("d.rs"), PathBuf::from("utils.rs"));

        graph.calculate_centrality();

        let top = graph.top_central(2);

        assert_eq!(top.len(), 2);
        assert_eq!(top[0].path, PathBuf::from("core.rs"));
    }

    #[test]
    fn test_leaf_and_root_files() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));

        graph.add_edge(PathBuf::from("main.rs"), PathBuf::from("lib.rs"));
        graph.add_edge(PathBuf::from("lib.rs"), PathBuf::from("utils.rs"));

        let leaves = graph.leaf_files();
        let roots = graph.root_files();

        // main.rs has no dependents (nothing imports it)
        assert!(leaves.iter().any(|n| n.path == Path::new("main.rs")));

        // utils.rs has no dependencies
        assert!(roots.iter().any(|n| n.path == Path::new("utils.rs")));
    }

    #[test]
    fn test_transitive_dependencies() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));

        graph.add_edge(PathBuf::from("a.rs"), PathBuf::from("b.rs"));
        graph.add_edge(PathBuf::from("b.rs"), PathBuf::from("c.rs"));
        graph.add_edge(PathBuf::from("c.rs"), PathBuf::from("d.rs"));

        let deps = graph.transitive_dependencies(Path::new("a.rs"));

        assert!(deps.contains(&PathBuf::from("b.rs")));
        assert!(deps.contains(&PathBuf::from("c.rs")));
        assert!(deps.contains(&PathBuf::from("d.rs")));
        assert!(!deps.contains(&PathBuf::from("a.rs")));
    }

    #[test]
    fn test_transitive_dependents() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));

        graph.add_edge(PathBuf::from("a.rs"), PathBuf::from("b.rs"));
        graph.add_edge(PathBuf::from("b.rs"), PathBuf::from("c.rs"));
        graph.add_edge(PathBuf::from("c.rs"), PathBuf::from("d.rs"));

        let deps = graph.transitive_dependents(Path::new("d.rs"));

        assert!(deps.contains(&PathBuf::from("a.rs")));
        assert!(deps.contains(&PathBuf::from("b.rs")));
        assert!(deps.contains(&PathBuf::from("c.rs")));
        assert!(!deps.contains(&PathBuf::from("d.rs")));
    }

    #[test]
    fn test_find_cycles() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));

        // Create a cycle: a -> b -> c -> a
        graph.add_edge(PathBuf::from("a.rs"), PathBuf::from("b.rs"));
        graph.add_edge(PathBuf::from("b.rs"), PathBuf::from("c.rs"));
        graph.add_edge(PathBuf::from("c.rs"), PathBuf::from("a.rs"));

        let cycles = graph.find_cycles();

        assert!(!cycles.is_empty());
    }

    #[test]
    fn test_remove_file() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));

        graph.add_edge(PathBuf::from("a.rs"), PathBuf::from("b.rs"));
        graph.add_edge(PathBuf::from("b.rs"), PathBuf::from("c.rs"));

        assert_eq!(graph.node_count(), 3);

        graph.remove_file(Path::new("b.rs"));

        assert_eq!(graph.node_count(), 2);

        // a should no longer have b as dependency
        let a = graph.get_node(Path::new("a.rs")).unwrap();
        assert!(!a.dependencies.contains(&PathBuf::from("b.rs")));

        // c should no longer have b as dependent
        let c = graph.get_node(Path::new("c.rs")).unwrap();
        assert!(!c.dependents.contains(&PathBuf::from("b.rs")));
    }

    #[test]
    fn test_graph_stats() {
        let stats = GraphStats {
            files_processed: 10,
            imports_found: 50,
            imports_resolved: 30,
        };

        assert_eq!(stats.resolution_rate(), 60.0);
    }

    #[test]
    fn test_graph_stats_zero_imports() {
        let stats = GraphStats {
            files_processed: 5,
            imports_found: 0,
            imports_resolved: 0,
        };

        assert_eq!(stats.resolution_rate(), 100.0);
    }
}
