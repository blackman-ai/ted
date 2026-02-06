// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Downloadable model registry
//!
//! Provides a registry of GGUF models that can be downloaded from HuggingFace
//! for local inference. Includes an embedded fallback registry and supports
//! fetching updated registry from a remote URL.
//!
//! # Architecture
//!
//! 1. **Embedded Registry**: Built-in registry with popular models (always available)
//! 2. **Remote Registry**: Fetches updated registry from GitHub Pages / ted.dev
//! 3. **Local Cache**: Caches downloaded models in ~/.ted/models/
//! 4. **Verification**: SHA256 verification for all downloads
//!
//! # Usage
//!
//! ```rust,ignore
//! use ted::models::download::{DownloadRegistry, ModelDownloader};
//!
//! let registry = DownloadRegistry::new().await?;
//! let models = registry.list_models();
//!
//! // Download a model
//! let downloader = ModelDownloader::new()?;
//! let path = downloader.download(&models[0]).await?;
//! ```

use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::{Result, TedError};

/// Remote registry URL (GitHub Pages or ted.dev)
const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/blackman-ai/ted/main/registry/models.json";

/// Fallback embedded registry (compiled into binary)
const EMBEDDED_REGISTRY: &str = include_str!("../../registry/models.json");

/// Cache TTL for registry (24 hours)
const REGISTRY_CACHE_TTL: Duration = Duration::from_secs(86400);

/// Model category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelCategory {
    /// General-purpose chat models
    Chat,
    /// Code-focused models
    Code,
    /// Embedding models
    Embedding,
}

impl ModelCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            ModelCategory::Chat => "Chat",
            ModelCategory::Code => "Code",
            ModelCategory::Embedding => "Embedding",
        }
    }
}

/// Quantization level
///
/// Uses standard llama.cpp naming conventions for quantization levels.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quantization {
    /// Full precision (F16)
    F16,
    /// 8-bit quantization
    Q8_0,
    /// 6-bit quantization
    Q6_K,
    /// 5-bit quantization
    Q5_K_M,
    /// 4-bit quantization (small)
    Q4_K_S,
    /// 4-bit quantization (medium)
    Q4_K_M,
    /// 4-bit quantization (extra large, for MoE models)
    Q4_K_XL,
    /// 3-bit quantization
    Q3_K_M,
    /// 2-bit quantization
    Q2_K,
    /// 2-bit quantization (extra large, for MoE models)
    Q2_K_XL,
}

impl Quantization {
    pub fn display_name(&self) -> &'static str {
        match self {
            Quantization::F16 => "F16 (Full)",
            Quantization::Q8_0 => "Q8_0 (High)",
            Quantization::Q6_K => "Q6_K (High)",
            Quantization::Q5_K_M => "Q5_K_M (Medium)",
            Quantization::Q4_K_S => "Q4_K_S (Medium)",
            Quantization::Q4_K_M => "Q4_K_M (Medium)",
            Quantization::Q4_K_XL => "Q4_K_XL (MoE)",
            Quantization::Q3_K_M => "Q3_K_M (Low)",
            Quantization::Q2_K => "Q2_K (Tiny)",
            Quantization::Q2_K_XL => "Q2_K_XL (MoE)",
        }
    }

    /// Get approximate quality score (0-100)
    pub fn quality_score(&self) -> u8 {
        match self {
            Quantization::F16 => 100,
            Quantization::Q8_0 => 95,
            Quantization::Q6_K => 90,
            Quantization::Q5_K_M => 85,
            Quantization::Q4_K_S => 75,
            Quantization::Q4_K_M => 80,
            Quantization::Q4_K_XL => 80,
            Quantization::Q3_K_M => 65,
            Quantization::Q2_K => 50,
            Quantization::Q2_K_XL => 55,
        }
    }
}

/// A downloadable model variant (specific quantization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVariant {
    /// Quantization level
    pub quantization: Quantization,
    /// Download URL (typically HuggingFace)
    pub url: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// SHA256 hash for verification
    pub sha256: String,
    /// Minimum VRAM required in GB
    pub min_vram_gb: f32,
}

impl ModelVariant {
    /// Get human-readable file size
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

/// A downloadable model with all its variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadableModel {
    /// Unique model ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Model category
    pub category: ModelCategory,
    /// Parameter count (e.g., "7B", "13B")
    pub parameters: String,
    /// Context window size
    pub context_size: u32,
    /// Base model (e.g., "Qwen2.5-Coder")
    pub base_model: String,
    /// Model creator/organization
    pub creator: String,
    /// License
    pub license: String,
    /// Available quantization variants
    pub variants: Vec<ModelVariant>,
    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
}

impl DownloadableModel {
    /// Get the recommended variant for given VRAM budget
    pub fn recommended_variant(&self, available_vram_gb: f32) -> Option<&ModelVariant> {
        // Find the highest quality variant that fits in VRAM
        self.variants
            .iter()
            .filter(|v| v.min_vram_gb <= available_vram_gb)
            .max_by_key(|v| v.quantization.quality_score())
    }

    /// Get the smallest variant (for low-memory systems)
    pub fn smallest_variant(&self) -> Option<&ModelVariant> {
        self.variants.iter().min_by_key(|v| v.size_bytes)
    }
}

/// The downloadable models registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRegistry {
    /// Registry version
    pub version: String,
    /// Last updated timestamp
    pub updated_at: String,
    /// Available models
    pub models: Vec<DownloadableModel>,
}

impl DownloadRegistry {
    /// Create a new registry from embedded fallback
    pub fn embedded() -> Result<Self> {
        serde_json::from_str(EMBEDDED_REGISTRY)
            .map_err(|e| TedError::Context(format!("Failed to parse embedded registry: {}", e)))
    }

    /// Fetch the latest registry from remote URL
    pub async fn fetch() -> Result<Self> {
        let client = Client::new();
        let response = client
            .get(REGISTRY_URL)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| TedError::Context(format!("Failed to fetch registry: {}", e)))?;

        if !response.status().is_success() {
            return Err(TedError::Context(format!(
                "Registry fetch failed with status: {}",
                response.status()
            )));
        }

        let text = response
            .text()
            .await
            .map_err(|e| TedError::Context(format!("Failed to read registry response: {}", e)))?;

        serde_json::from_str(&text)
            .map_err(|e| TedError::Context(format!("Failed to parse registry: {}", e)))
    }

    /// Create registry with fallback (try remote first, fall back to embedded)
    pub async fn with_fallback() -> Result<Self> {
        match Self::fetch().await {
            Ok(registry) => Ok(registry),
            Err(e) => {
                tracing::warn!("Failed to fetch remote registry, using embedded: {}", e);
                Self::embedded()
            }
        }
    }

    /// Get the cache file path
    fn cache_path() -> Result<PathBuf> {
        Ok(dirs::home_dir()
            .ok_or_else(|| TedError::Config("Cannot find home directory".to_string()))?
            .join(".ted")
            .join("registry")
            .join("models.json"))
    }

    /// Load registry from cache if fresh
    fn load_from_cache() -> Option<Self> {
        let cache_path = Self::cache_path().ok()?;

        // Check if cache file exists and is fresh
        let metadata = std::fs::metadata(&cache_path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;

        if age > REGISTRY_CACHE_TTL {
            tracing::debug!("Registry cache is stale ({:.0}h old)", age.as_secs() / 3600);
            return None;
        }

        // Try to load from cache
        let contents = std::fs::read_to_string(&cache_path).ok()?;
        match serde_json::from_str(&contents) {
            Ok(registry) => {
                tracing::debug!("Loaded registry from cache ({:.0}h old)", age.as_secs() / 3600);
                Some(registry)
            }
            Err(e) => {
                tracing::warn!("Failed to parse cached registry: {}", e);
                None
            }
        }
    }

    /// Save registry to cache
    fn save_to_cache(&self) -> Result<()> {
        let cache_path = Self::cache_path()?;

        // Create parent directory if needed
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| TedError::Config(format!("Failed to create cache directory: {}", e)))?;
        }

        // Write to cache
        let contents = serde_json::to_string_pretty(&self)
            .map_err(|e| TedError::Context(format!("Failed to serialize registry: {}", e)))?;
        std::fs::write(&cache_path, contents)
            .map_err(|e| TedError::Config(format!("Failed to write cache: {}", e)))?;

        tracing::debug!("Saved registry to cache: {}", cache_path.display());
        Ok(())
    }

    /// Create registry with caching support
    ///
    /// Priority:
    /// 1. Fresh cache (< 24h old) - return immediately
    /// 2. Remote fetch - update cache on success
    /// 3. Stale cache - if remote fails
    /// 4. Embedded fallback - last resort
    pub async fn with_cache() -> Result<Self> {
        // Try fresh cache first
        if let Some(registry) = Self::load_from_cache() {
            return Ok(registry);
        }

        // Try remote fetch
        match Self::fetch().await {
            Ok(registry) => {
                // Cache the result (ignore errors)
                if let Err(e) = registry.save_to_cache() {
                    tracing::warn!("Failed to cache registry: {}", e);
                }
                Ok(registry)
            }
            Err(fetch_error) => {
                tracing::warn!("Failed to fetch remote registry: {}", fetch_error);

                // Try stale cache (read without TTL check)
                let cache_path = Self::cache_path()?;
                if cache_path.exists() {
                    if let Ok(contents) = std::fs::read_to_string(&cache_path) {
                        if let Ok(registry) = serde_json::from_str(&contents) {
                            tracing::info!("Using stale cached registry");
                            return Ok(registry);
                        }
                    }
                }

                // Fall back to embedded
                tracing::info!("Using embedded registry");
                Self::embedded()
            }
        }
    }

    /// List all models
    pub fn list_models(&self) -> &[DownloadableModel] {
        &self.models
    }

    /// Find a model by ID
    pub fn find_model(&self, id: &str) -> Option<&DownloadableModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Filter models by category
    pub fn models_by_category(&self, category: ModelCategory) -> Vec<&DownloadableModel> {
        self.models
            .iter()
            .filter(|m| m.category == category)
            .collect()
    }

    /// Filter models that fit in available VRAM
    pub fn models_for_vram(&self, available_vram_gb: f32) -> Vec<&DownloadableModel> {
        self.models
            .iter()
            .filter(|m| m.recommended_variant(available_vram_gb).is_some())
            .collect()
    }

    /// Get recommended code models for given VRAM budget
    pub fn recommended_code_models(&self, available_vram_gb: f32) -> Vec<&DownloadableModel> {
        self.models
            .iter()
            .filter(|m| {
                m.category == ModelCategory::Code
                    && m.recommended_variant(available_vram_gb).is_some()
            })
            .collect()
    }
}

/// Model downloader with progress tracking and verification
pub struct ModelDownloader {
    /// HTTP client
    client: Client,
    /// Download directory
    download_dir: PathBuf,
}

impl ModelDownloader {
    /// Create a new downloader with default cache directory
    pub fn new() -> Result<Self> {
        let download_dir = dirs::home_dir()
            .ok_or_else(|| TedError::Config("Cannot find home directory".to_string()))?
            .join(".ted")
            .join("models")
            .join("local");

        std::fs::create_dir_all(&download_dir)
            .map_err(|e| TedError::Config(format!("Failed to create model directory: {}", e)))?;

        Ok(Self {
            client: Client::new(),
            download_dir,
        })
    }

    /// Create a downloader with custom directory
    pub fn with_dir(download_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&download_dir)
            .map_err(|e| TedError::Config(format!("Failed to create model directory: {}", e)))?;

        Ok(Self {
            client: Client::new(),
            download_dir,
        })
    }

    /// Get the download directory
    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }

    /// Check if a model variant is already downloaded
    pub fn is_downloaded(&self, model: &DownloadableModel, variant: &ModelVariant) -> bool {
        let path = self.model_path(model, variant);
        if !path.exists() {
            return false;
        }

        // Verify SHA256
        self.verify_file(&path, &variant.sha256).unwrap_or(false)
    }

    /// Get the path where a model would be stored
    pub fn model_path(&self, model: &DownloadableModel, variant: &ModelVariant) -> PathBuf {
        let filename = format!("{}-{:?}.gguf", model.id, variant.quantization);
        self.download_dir.join(filename.to_lowercase())
    }

    /// Download a model variant
    pub async fn download(
        &self,
        model: &DownloadableModel,
        variant: &ModelVariant,
    ) -> Result<PathBuf> {
        let path = self.model_path(model, variant);

        // Check if already downloaded and valid
        if self.is_downloaded(model, variant) {
            tracing::info!("Model already downloaded: {}", path.display());
            return Ok(path);
        }

        tracing::info!("Downloading {} ({})...", model.name, variant.size_display());

        // Create temp file for download
        let temp_path = path.with_extension("tmp");

        let response = self
            .client
            .get(&variant.url)
            .send()
            .await
            .map_err(|e| TedError::Context(format!("Download failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(TedError::Context(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        // Stream download to file
        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| TedError::Context(format!("Failed to create temp file: {}", e)))?;

        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;
        let total = variant.size_bytes;

        let mut stream = response.bytes_stream();
        use futures::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| TedError::Context(format!("Download error: {}", e)))?;
            file.write_all(&chunk)
                .map_err(|e| TedError::Context(format!("Write error: {}", e)))?;
            hasher.update(&chunk);

            downloaded += chunk.len() as u64;
            let progress = (downloaded as f64 / total as f64 * 100.0) as u8;
            if downloaded % (50 * 1024 * 1024) < chunk.len() as u64 {
                // Log every ~50MB
                tracing::info!("Download progress: {}%", progress);
            }
        }

        // Verify SHA256
        let hash = format!("{:x}", hasher.finalize());
        if hash != variant.sha256 {
            std::fs::remove_file(&temp_path).ok();
            return Err(TedError::Context(format!(
                "SHA256 verification failed. Expected: {}, Got: {}",
                variant.sha256, hash
            )));
        }

        // Move to final location
        std::fs::rename(&temp_path, &path)
            .map_err(|e| TedError::Context(format!("Failed to move download: {}", e)))?;

        tracing::info!("Download complete: {}", path.display());
        Ok(path)
    }

    /// Verify a downloaded file's SHA256
    fn verify_file(&self, path: &Path, expected_sha256: &str) -> Result<bool> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| TedError::Context(format!("Failed to open file: {}", e)))?;

        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)
            .map_err(|e| TedError::Context(format!("Failed to read file: {}", e)))?;

        let hash = format!("{:x}", hasher.finalize());
        Ok(hash == expected_sha256)
    }

    /// List all downloaded models
    pub fn list_downloaded(&self) -> Result<Vec<PathBuf>> {
        let mut models = Vec::new();

        for entry in std::fs::read_dir(&self.download_dir)
            .map_err(|e| TedError::Context(format!("Failed to read directory: {}", e)))?
        {
            let entry =
                entry.map_err(|e| TedError::Context(format!("Failed to read entry: {}", e)))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                models.push(path);
            }
        }

        Ok(models)
    }

    /// Delete a downloaded model
    pub fn delete(&self, path: &Path) -> Result<()> {
        std::fs::remove_file(path)
            .map_err(|e| TedError::Context(format!("Failed to delete model: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantization_quality() {
        assert!(Quantization::F16.quality_score() > Quantization::Q4_K_M.quality_score());
        assert!(Quantization::Q4_K_M.quality_score() > Quantization::Q2_K.quality_score());
    }

    #[test]
    fn test_variant_size_display() {
        let variant = ModelVariant {
            quantization: Quantization::Q4_K_M,
            url: String::new(),
            size_bytes: 4_294_967_296, // 4GB
            sha256: String::new(),
            min_vram_gb: 6.0,
        };
        assert_eq!(variant.size_display(), "4.0 GB");

        let small_variant = ModelVariant {
            quantization: Quantization::Q2_K,
            url: String::new(),
            size_bytes: 524_288_000, // 500MB
            sha256: String::new(),
            min_vram_gb: 2.0,
        };
        assert_eq!(small_variant.size_display(), "500 MB");
    }

    #[test]
    fn test_model_category() {
        assert_eq!(ModelCategory::Code.display_name(), "Code");
        assert_eq!(ModelCategory::Chat.display_name(), "Chat");
    }

    #[test]
    fn test_downloader_path() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let model = DownloadableModel {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            category: ModelCategory::Code,
            parameters: "7B".to_string(),
            context_size: 8192,
            base_model: "test".to_string(),
            creator: "test".to_string(),
            license: "MIT".to_string(),
            variants: vec![],
            tags: vec![],
        };

        let variant = ModelVariant {
            quantization: Quantization::Q4_K_M,
            url: String::new(),
            size_bytes: 0,
            sha256: String::new(),
            min_vram_gb: 0.0,
        };

        let path = downloader.model_path(&model, &variant);
        assert!(path.to_string_lossy().contains("test-model"));
        assert!(path.to_string_lossy().contains("q4_k_m"));
    }

    #[test]
    fn test_embedded_registry() {
        let registry = DownloadRegistry::embedded().unwrap();
        assert!(!registry.models.is_empty());
        assert!(!registry.version.is_empty());
    }

    #[test]
    fn test_registry_find_model() {
        let registry = DownloadRegistry::embedded().unwrap();

        // Should find models that exist
        let found = registry.models.iter().any(|m| m.category == ModelCategory::Code);
        assert!(found, "Should have at least one code model");
    }

    #[test]
    fn test_registry_cache_path() {
        let path = DownloadRegistry::cache_path().unwrap();
        assert!(path.to_string_lossy().contains(".ted"));
        assert!(path.to_string_lossy().contains("registry"));
        assert!(path.to_string_lossy().contains("models.json"));
    }
}
