// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! System-wide GGUF model discovery
//!
//! Scans standard directories for existing GGUF model files from
//! Ted, LM Studio, HuggingFace cache, and GPT4All.

use std::fmt;
use std::path::{Path, PathBuf};

/// Where a discovered model came from
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    /// Ted's own model directory (~/.ted/models/local/)
    Ted,
    /// LM Studio models directory
    LmStudio,
    /// HuggingFace cache (~/.cache/huggingface/hub/)
    HuggingFace,
    /// GPT4All models directory
    Gpt4All,
    /// User-specified custom path
    Custom(PathBuf),
}

impl ModelSource {
    pub fn label(&self) -> &str {
        match self {
            ModelSource::Ted => "ted",
            ModelSource::LmStudio => "LM Studio",
            ModelSource::HuggingFace => "HuggingFace",
            ModelSource::Gpt4All => "GPT4All",
            ModelSource::Custom(_) => "custom",
        }
    }
}

impl fmt::Display for ModelSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// A GGUF model file discovered on the system
#[derive(Debug, Clone)]
pub struct DiscoveredModel {
    pub path: PathBuf,
    pub filename: String,
    pub size_bytes: u64,
    pub source: ModelSource,
}

impl DiscoveredModel {
    pub fn display_name(&self) -> String {
        format!("{} (from {})", self.filename, self.source)
    }

    pub fn size_display(&self) -> String {
        let gb = self.size_bytes as f64 / 1_073_741_824.0;
        if gb >= 1.0 {
            format!("{:.1} GB", gb)
        } else {
            let mb = self.size_bytes as f64 / 1_048_576.0;
            format!("{:.0} MB", mb)
        }
    }
}

/// Scan all standard locations for GGUF model files.
///
/// Returns models sorted by source priority (Ted first) then by size descending.
pub fn scan_for_models() -> Vec<DiscoveredModel> {
    let mut models = Vec::new();

    // 1. Ted's own models (highest priority)
    if let Some(dir) = ted_models_dir() {
        scan_directory(&dir, &ModelSource::Ted, &mut models, 2);
    }

    // 2. LM Studio
    for dir in lm_studio_dirs() {
        scan_directory(&dir, &ModelSource::LmStudio, &mut models, 3);
    }

    // 3. HuggingFace cache (deeper nesting: models--org--name/snapshots/hash/)
    if let Some(dir) = huggingface_cache_dir() {
        scan_directory(&dir, &ModelSource::HuggingFace, &mut models, 5);
    }

    // 4. GPT4All
    if let Some(dir) = gpt4all_dir() {
        scan_directory(&dir, &ModelSource::Gpt4All, &mut models, 2);
    }

    // Sort: Ted first, then by size descending within each source
    models.sort_by(|a, b| {
        source_priority(&a.source)
            .cmp(&source_priority(&b.source))
            .then(b.size_bytes.cmp(&a.size_bytes))
    });

    models
}

/// Scan a specific directory for GGUF files and add to the provided path.
pub fn scan_custom_path(path: &Path) -> Vec<DiscoveredModel> {
    let mut models = Vec::new();
    let source = ModelSource::Custom(path.to_path_buf());
    scan_directory(path, &source, &mut models, 3);
    models
}

fn source_priority(source: &ModelSource) -> u8 {
    match source {
        ModelSource::Ted => 0,
        ModelSource::LmStudio => 1,
        ModelSource::HuggingFace => 2,
        ModelSource::Gpt4All => 3,
        ModelSource::Custom(_) => 4,
    }
}

fn ted_models_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ted").join("models").join("local"))
}

fn lm_studio_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        if cfg!(target_os = "macos") {
            dirs.push(home.join("Library/Application Support/lm-studio/models"));
        } else if cfg!(target_os = "linux") {
            dirs.push(home.join(".cache/lm-studio/models"));
        }
    }
    if cfg!(target_os = "windows") {
        if let Some(appdata) = dirs::config_dir() {
            dirs.push(appdata.join("lm-studio").join("models"));
        }
    }
    dirs
}

fn huggingface_cache_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".cache").join("huggingface").join("hub"))
}

fn gpt4all_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    if cfg!(target_os = "macos") {
        Some(home.join("Library/Application Support/nomic.ai/GPT4All"))
    } else if cfg!(target_os = "linux") {
        Some(home.join(".local/share/nomic.ai/GPT4All"))
    } else if cfg!(target_os = "windows") {
        dirs::config_dir().map(|d| d.join("nomic.ai").join("GPT4All"))
    } else {
        None
    }
}

fn scan_directory(
    dir: &Path,
    source: &ModelSource,
    models: &mut Vec<DiscoveredModel>,
    max_depth: u32,
) {
    if !dir.exists() {
        return;
    }
    scan_recursive(dir, source, models, 0, max_depth);
}

fn scan_recursive(
    dir: &Path,
    source: &ModelSource,
    models: &mut Vec<DiscoveredModel>,
    depth: u32,
    max_depth: u32,
) {
    if depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_recursive(&path, source, models, depth + 1, max_depth);
        } else if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
            if let Ok(metadata) = std::fs::metadata(&path) {
                // Skip tiny files (likely corrupt or incomplete)
                if metadata.len() < 1_000_000 {
                    continue;
                }
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                models.push(DiscoveredModel {
                    path,
                    filename,
                    size_bytes: metadata.len(),
                    source: source.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_fake_gguf(dir: &Path, name: &str, size: usize) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let data = vec![0u8; size];
        fs::write(&path, data).unwrap();
        path
    }

    #[test]
    fn test_scan_finds_gguf_files() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        create_fake_gguf(dir, "model-a.gguf", 2_000_000);
        create_fake_gguf(dir, "model-b.gguf", 5_000_000);
        create_fake_gguf(dir, "not-a-model.bin", 2_000_000);

        let mut models = Vec::new();
        scan_directory(dir, &ModelSource::Ted, &mut models, 3);
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn test_scan_recursive_with_depth() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        create_fake_gguf(dir, "top.gguf", 2_000_000);
        create_fake_gguf(dir, "sub/nested.gguf", 3_000_000);
        create_fake_gguf(dir, "a/b/c/d/too-deep.gguf", 4_000_000);

        let mut models = Vec::new();
        scan_directory(dir, &ModelSource::Ted, &mut models, 2);
        assert_eq!(models.len(), 2); // top.gguf and sub/nested.gguf
    }

    #[test]
    fn test_skips_tiny_files() {
        let tmp = TempDir::new().unwrap();
        create_fake_gguf(tmp.path(), "tiny.gguf", 100); // too small
        create_fake_gguf(tmp.path(), "real.gguf", 2_000_000);

        let mut models = Vec::new();
        scan_directory(tmp.path(), &ModelSource::Ted, &mut models, 2);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].filename, "real.gguf");
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let mut models = Vec::new();
        scan_directory(
            Path::new("/nonexistent/path"),
            &ModelSource::Ted,
            &mut models,
            3,
        );
        assert!(models.is_empty());
    }

    #[test]
    fn test_discovered_model_display() {
        let model = DiscoveredModel {
            path: PathBuf::from("/path/to/model.gguf"),
            filename: "model.gguf".to_string(),
            size_bytes: 5_000_000_000,
            source: ModelSource::LmStudio,
        };
        assert_eq!(model.display_name(), "model.gguf (from LM Studio)");
        assert_eq!(model.size_display(), "4.7 GB");
    }

    #[test]
    fn test_discovered_model_size_mb() {
        let model = DiscoveredModel {
            path: PathBuf::from("/path/to/small.gguf"),
            filename: "small.gguf".to_string(),
            size_bytes: 500_000_000,
            source: ModelSource::Ted,
        };
        assert_eq!(model.size_display(), "477 MB");
    }

    #[test]
    fn test_source_labels() {
        assert_eq!(ModelSource::Ted.label(), "ted");
        assert_eq!(ModelSource::LmStudio.label(), "LM Studio");
        assert_eq!(ModelSource::HuggingFace.label(), "HuggingFace");
        assert_eq!(ModelSource::Gpt4All.label(), "GPT4All");
        assert_eq!(ModelSource::Custom(PathBuf::from("/foo")).label(), "custom");
    }

    #[test]
    fn test_scan_custom_path() {
        let tmp = TempDir::new().unwrap();
        create_fake_gguf(tmp.path(), "custom-model.gguf", 3_000_000);

        let models = scan_custom_path(tmp.path());
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0].source,
            ModelSource::Custom(tmp.path().to_path_buf())
        );
    }

    #[test]
    fn test_sorting_order() {
        let models = vec![
            DiscoveredModel {
                path: PathBuf::from("a.gguf"),
                filename: "a.gguf".to_string(),
                size_bytes: 1_000_000,
                source: ModelSource::LmStudio,
            },
            DiscoveredModel {
                path: PathBuf::from("b.gguf"),
                filename: "b.gguf".to_string(),
                size_bytes: 5_000_000,
                source: ModelSource::Ted,
            },
            DiscoveredModel {
                path: PathBuf::from("c.gguf"),
                filename: "c.gguf".to_string(),
                size_bytes: 2_000_000,
                source: ModelSource::Ted,
            },
        ];

        let mut sorted = models;
        sorted.sort_by(|a, b| {
            source_priority(&a.source)
                .cmp(&source_priority(&b.source))
                .then(b.size_bytes.cmp(&a.size_bytes))
        });

        // Ted models first, larger first within same source
        assert_eq!(sorted[0].filename, "b.gguf"); // Ted, 5MB
        assert_eq!(sorted[1].filename, "c.gguf"); // Ted, 2MB
        assert_eq!(sorted[2].filename, "a.gguf"); // LM Studio
    }
}
