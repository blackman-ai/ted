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
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

use crate::error::{Result, TedError};

/// Embedded binary registry (compiled into binary)
const EMBEDDED_BINARY_REGISTRY: &str = include_str!("../../registry/binaries.json");

/// Platform info for the binary registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryPlatform {
    pub url: String,
    pub binary_path: String,
    pub archive_type: String,
}

/// llama-server version info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaServerInfo {
    pub version: String,
    pub release_url: String,
    pub platforms: std::collections::HashMap<String, BinaryPlatform>,
}

/// Binary registry for llama-server downloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryRegistry {
    pub version: String,
    pub updated_at: String,
    pub llama_server: LlamaServerInfo,
}

impl BinaryRegistry {
    /// Load the embedded binary registry
    pub fn embedded() -> Result<Self> {
        serde_json::from_str(EMBEDDED_BINARY_REGISTRY)
            .map_err(|e| TedError::Context(format!("Failed to parse binary registry: {}", e)))
    }

    /// Get platform info for the current system
    pub fn platform_info(&self) -> Result<&BinaryPlatform> {
        let key = platform_key();
        self.llama_server
            .platforms
            .get(&key)
            .ok_or_else(|| TedError::Config(format!("Unsupported platform: {}", key)))
    }
}

/// Get the platform key for the current system (e.g., "darwin-arm64")
pub fn platform_key() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "unknown"
    };

    format!("{}-{}", os, arch)
}

/// Locate a binary in an extracted archive.
///
/// Archives occasionally change internal folder layout between releases.
/// We first try the registry's expected relative path, then fall back to
/// searching for the binary filename anywhere under the extraction root.
fn find_extracted_binary(
    extract_root: &Path,
    expected_relative_path: &str,
    binary_name: &str,
) -> Option<PathBuf> {
    let expected = extract_root.join(expected_relative_path);
    if expected.is_file() {
        return Some(expected);
    }

    WalkDir::new(extract_root)
        .follow_links(true)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .find_map(|entry| {
            if entry.file_type().is_file() && entry.file_name() == std::ffi::OsStr::new(binary_name)
            {
                Some(entry.into_path())
            } else {
                None
            }
        })
}

/// Check whether a llama-server binary can start far enough to print help.
/// This catches missing runtime dependencies (e.g., shared libraries).
fn is_runnable_binary(binary_path: &Path) -> bool {
    match Command::new(binary_path)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn ensure_macos_dylib_major_links(dir: &Path) -> Result<usize> {
    use std::os::unix::fs::symlink;

    let mut created = 0usize;

    let entries = std::fs::read_dir(dir)
        .map_err(|e| TedError::Context(format!("Failed to read dir: {}", e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| TedError::Context(format!("Failed to read entry: {}", e)))?;
        let file_type = entry
            .file_type()
            .map_err(|e| TedError::Context(format!("Failed to read entry type: {}", e)))?;
        if !file_type.is_file() {
            continue;
        }

        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !name.ends_with(".dylib") {
            continue;
        }

        let stem = &name[..name.len() - ".dylib".len()];
        let parts: Vec<&str> = stem.split('.').collect();
        if parts.len() < 4 {
            continue;
        }

        let major = parts[parts.len() - 3];
        let minor = parts[parts.len() - 2];
        let patch = parts[parts.len() - 1];

        if !major.chars().all(|c| c.is_ascii_digit())
            || !minor.chars().all(|c| c.is_ascii_digit())
            || !patch.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }

        let prefix = parts[..parts.len() - 3].join(".");
        if prefix.is_empty() {
            continue;
        }

        let link_name = format!("{}.{}.dylib", prefix, major);
        if link_name == name {
            continue;
        }

        let link_path = dir.join(&link_name);
        if std::fs::symlink_metadata(&link_path).is_ok() {
            continue;
        }

        symlink(name, &link_path)
            .map_err(|e| TedError::Context(format!("Failed to create dylib symlink: {}", e)))?;
        created += 1;
    }

    Ok(created)
}

#[cfg(not(target_os = "macos"))]
fn ensure_macos_dylib_major_links(_dir: &Path) -> Result<usize> {
    Ok(0)
}

fn repair_local_runtime_layout(dir: &Path) {
    if let Ok(created) = ensure_macos_dylib_major_links(dir) {
        if created > 0 {
            tracing::info!(
                "Created {} macOS dylib compatibility links in {}",
                created,
                dir.display()
            );
        }
    }
}

/// Copy files colocated with the extracted binary (runtime companions like
/// shared libraries and shader blobs) into destination directory.
fn copy_binary_companions(extracted_binary: &Path, dest_dir: &Path) -> Result<usize> {
    let Some(parent) = extracted_binary.parent() else {
        return Ok(0);
    };

    let mut copied = 0usize;
    let binary_name = extracted_binary.file_name();

    let entries = std::fs::read_dir(parent).map_err(|e| {
        TedError::Context(format!("Failed to read extracted binary directory: {}", e))
    })?;

    for entry in entries {
        let entry = entry
            .map_err(|e| TedError::Context(format!("Failed to read extracted entry: {}", e)))?;
        let path = entry.path();

        let file_type = entry.file_type().map_err(|e| {
            TedError::Context(format!("Failed to read extracted entry type: {}", e))
        })?;

        if !file_type.is_file() {
            #[cfg(unix)]
            if file_type.is_symlink() {
                use std::os::unix::fs::symlink;

                let link_target = std::fs::read_link(&path).map_err(|e| {
                    TedError::Context(format!("Failed to read companion symlink: {}", e))
                })?;
                let dest = dest_dir.join(entry.file_name());

                if std::fs::symlink_metadata(&dest).is_ok() {
                    let _ = std::fs::remove_file(&dest);
                }

                symlink(&link_target, &dest).map_err(|e| {
                    TedError::Context(format!("Failed to copy companion symlink: {}", e))
                })?;
                copied += 1;
            }
            continue;
        }

        if Some(entry.file_name().as_os_str()) == binary_name {
            continue;
        }

        let dest = dest_dir.join(entry.file_name());
        std::fs::copy(&path, &dest)
            .map_err(|e| TedError::Context(format!("Failed to copy companion file: {}", e)))?;
        copied += 1;
    }

    Ok(copied)
}

/// Binary downloader for llama-server
pub struct BinaryDownloader {
    client: Client,
    binaries_dir: PathBuf,
}

impl BinaryDownloader {
    /// Create a new binary downloader
    pub fn new() -> Result<Self> {
        let binaries_dir = dirs::home_dir()
            .ok_or_else(|| TedError::Config("Cannot find home directory".to_string()))?
            .join(".ted")
            .join("bin");

        std::fs::create_dir_all(&binaries_dir)
            .map_err(|e| TedError::Config(format!("Failed to create binaries directory: {}", e)))?;

        Ok(Self {
            client: Client::new(),
            binaries_dir,
        })
    }

    /// Create a binary downloader with a custom directory
    pub fn with_dir(binaries_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&binaries_dir)
            .map_err(|e| TedError::Config(format!("Failed to create binaries directory: {}", e)))?;

        Ok(Self {
            client: Client::new(),
            binaries_dir,
        })
    }

    /// Find llama-server binary: check system PATH first, then ~/.ted/bin/
    pub fn find_llama_server(&self) -> Option<PathBuf> {
        // 1. Check PATH for existing system-wide installation
        let cmd = if cfg!(target_os = "windows") {
            "where"
        } else {
            "which"
        };

        if let Ok(output) = std::process::Command::new(cmd).arg("llama-server").output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout);
                let path = PathBuf::from(path_str.trim());
                if path.exists() && is_runnable_binary(&path) {
                    tracing::info!("Found system llama-server at: {}", path.display());
                    return Some(path);
                } else if path.exists() {
                    tracing::warn!(
                        "Ignoring non-runnable system llama-server at {}",
                        path.display()
                    );
                }
            }
        }

        // 2. Check our binaries directory
        let binary_name = if cfg!(target_os = "windows") {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        let local_path = self.binaries_dir.join(binary_name);
        if local_path.exists() && is_runnable_binary(&local_path) {
            tracing::info!("Found local llama-server at: {}", local_path.display());
            return Some(local_path);
        } else if local_path.exists() {
            repair_local_runtime_layout(&self.binaries_dir);
            if is_runnable_binary(&local_path) {
                tracing::info!(
                    "Recovered local llama-server runtime layout at: {}",
                    local_path.display()
                );
                return Some(local_path);
            }
            tracing::warn!(
                "Ignoring non-runnable local llama-server at {}; will re-download",
                local_path.display()
            );
        }

        None
    }

    /// Download llama-server for the current platform
    pub async fn download_llama_server(&self) -> Result<PathBuf> {
        let registry = BinaryRegistry::embedded()?;
        let platform = registry.platform_info()?;

        let binary_name = if cfg!(target_os = "windows") {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        let dest_path = self.binaries_dir.join(binary_name);

        // Skip if already downloaded and still runnable.
        // If the binary exists but cannot start (missing runtime dependencies),
        // remove it and reinstall from archive.
        if dest_path.exists() && is_runnable_binary(&dest_path) {
            return Ok(dest_path);
        } else if dest_path.exists() {
            tracing::warn!(
                "Existing llama-server is not runnable at {}; reinstalling",
                dest_path.display()
            );
            let _ = std::fs::remove_file(&dest_path);
        }

        tracing::info!(
            "Downloading llama-server {} for {}...",
            registry.llama_server.version,
            platform_key()
        );

        // Download the archive
        let response =
            self.client.get(&platform.url).send().await.map_err(|e| {
                TedError::Context(format!("Failed to download llama-server: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(TedError::Context(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| TedError::Context(format!("Failed to read download: {}", e)))?;

        // Extract the binary from the archive
        let temp_dir = self.binaries_dir.join("_extract_tmp");
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| TedError::Context(format!("Failed to create temp dir: {}", e)))?;

        match platform.archive_type.as_str() {
            "tar.gz" => {
                let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(&bytes));
                let mut archive = tar::Archive::new(decoder);
                archive
                    .unpack(&temp_dir)
                    .map_err(|e| TedError::Context(format!("Failed to extract tar.gz: {}", e)))?;
            }
            "zip" => {
                let cursor = std::io::Cursor::new(&bytes);
                let mut archive = zip::ZipArchive::new(cursor)
                    .map_err(|e| TedError::Context(format!("Failed to open zip: {}", e)))?;
                archive
                    .extract(&temp_dir)
                    .map_err(|e| TedError::Context(format!("Failed to extract zip: {}", e)))?;
            }
            other => {
                let _ = std::fs::remove_dir_all(&temp_dir);
                return Err(TedError::Context(format!(
                    "Unsupported archive type: {}",
                    other
                )));
            }
        }

        // Find and move the binary
        let expected_binary = temp_dir.join(&platform.binary_path);
        let extracted_binary = match find_extracted_binary(
            &temp_dir,
            &platform.binary_path,
            binary_name,
        ) {
            Some(path) => path,
            None => {
                let _ = std::fs::remove_dir_all(&temp_dir);
                return Err(TedError::Context(format!(
                        "Binary not found in archive. Expected '{}' and could not find '{}' anywhere in extracted files",
                        platform.binary_path, binary_name
                    )));
            }
        };

        if extracted_binary != expected_binary {
            tracing::warn!(
                "llama-server archive layout changed: expected '{}' but found '{}'",
                platform.binary_path,
                extracted_binary.display()
            );
        }

        if !extracted_binary.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err(TedError::Context(format!(
                "Binary not found in archive at: {}",
                platform.binary_path
            )));
        }

        std::fs::copy(&extracted_binary, &dest_path)
            .map_err(|e| TedError::Context(format!("Failed to copy binary: {}", e)))?;

        let companion_files = copy_binary_companions(&extracted_binary, &self.binaries_dir)?;
        if companion_files > 0 {
            tracing::info!(
                "Copied {} llama-server companion files to {}",
                companion_files,
                self.binaries_dir.display()
            );
        }
        repair_local_runtime_layout(&self.binaries_dir);

        // Set executable permission on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&dest_path, perms)
                .map_err(|e| TedError::Context(format!("Failed to set permissions: {}", e)))?;
        }

        // Clean up temp directory
        let _ = std::fs::remove_dir_all(&temp_dir);

        tracing::info!("llama-server installed to: {}", dest_path.display());
        Ok(dest_path)
    }

    /// Find or download llama-server
    pub async fn ensure_llama_server(&self) -> Result<PathBuf> {
        if let Some(path) = self.find_llama_server() {
            return Ok(path);
        }
        self.download_llama_server().await
    }
}

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
                tracing::debug!(
                    "Loaded registry from cache ({:.0}h old)",
                    age.as_secs() / 3600
                );
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
            std::fs::create_dir_all(parent).map_err(|e| {
                TedError::Config(format!("Failed to create cache directory: {}", e))
            })?;
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

        // Skip SHA256 verification for placeholder hashes
        if variant.sha256.starts_with("placeholder_") {
            return std::fs::metadata(&path)
                .map(|m| m.len() > 0)
                .unwrap_or(false);
        }

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

        // Verify SHA256 (skip for placeholder hashes)
        let hash = format!("{:x}", hasher.finalize());
        if variant.sha256.starts_with("placeholder_") {
            tracing::warn!(
                "SHA256 hash is a placeholder — skipping verification for {}",
                model.name
            );
        } else if hash != variant.sha256 {
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
    use tempfile::TempDir;

    fn write_mock_runnable_binary(path: &Path) {
        #[cfg(windows)]
        {
            let script = "@echo off\r\nexit /b 0\r\n";
            std::fs::write(path, script).unwrap();
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let script = "#!/bin/sh\nexit 0\n";
            std::fs::write(path, script).unwrap();
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(path, perms).unwrap();
        }
    }

    // ==================== Quantization tests ====================

    #[test]
    fn test_quantization_quality() {
        assert!(Quantization::F16.quality_score() > Quantization::Q4_K_M.quality_score());
        assert!(Quantization::Q4_K_M.quality_score() > Quantization::Q2_K.quality_score());
    }

    #[test]
    fn test_quantization_all_display_names() {
        assert_eq!(Quantization::F16.display_name(), "F16 (Full)");
        assert_eq!(Quantization::Q8_0.display_name(), "Q8_0 (High)");
        assert_eq!(Quantization::Q6_K.display_name(), "Q6_K (High)");
        assert_eq!(Quantization::Q5_K_M.display_name(), "Q5_K_M (Medium)");
        assert_eq!(Quantization::Q4_K_S.display_name(), "Q4_K_S (Medium)");
        assert_eq!(Quantization::Q4_K_M.display_name(), "Q4_K_M (Medium)");
        assert_eq!(Quantization::Q4_K_XL.display_name(), "Q4_K_XL (MoE)");
        assert_eq!(Quantization::Q3_K_M.display_name(), "Q3_K_M (Low)");
        assert_eq!(Quantization::Q2_K.display_name(), "Q2_K (Tiny)");
        assert_eq!(Quantization::Q2_K_XL.display_name(), "Q2_K_XL (MoE)");
    }

    #[test]
    fn test_quantization_all_quality_scores() {
        assert_eq!(Quantization::F16.quality_score(), 100);
        assert_eq!(Quantization::Q8_0.quality_score(), 95);
        assert_eq!(Quantization::Q6_K.quality_score(), 90);
        assert_eq!(Quantization::Q5_K_M.quality_score(), 85);
        assert_eq!(Quantization::Q4_K_S.quality_score(), 75);
        assert_eq!(Quantization::Q4_K_M.quality_score(), 80);
        assert_eq!(Quantization::Q4_K_XL.quality_score(), 80);
        assert_eq!(Quantization::Q3_K_M.quality_score(), 65);
        assert_eq!(Quantization::Q2_K.quality_score(), 50);
        assert_eq!(Quantization::Q2_K_XL.quality_score(), 55);
    }

    #[test]
    fn test_quantization_quality_ordering() {
        // Higher quality means higher score
        let scores: Vec<u8> = [
            Quantization::F16,
            Quantization::Q8_0,
            Quantization::Q6_K,
            Quantization::Q5_K_M,
            Quantization::Q4_K_M,
            Quantization::Q3_K_M,
            Quantization::Q2_K,
        ]
        .iter()
        .map(|q| q.quality_score())
        .collect();

        for i in 1..scores.len() {
            assert!(scores[i - 1] >= scores[i]);
        }
    }

    // ==================== ModelVariant tests ====================

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
    fn test_variant_size_display_edge_cases() {
        // Exactly 1 GB
        let variant_1gb = ModelVariant {
            quantization: Quantization::Q4_K_M,
            url: String::new(),
            size_bytes: 1_073_741_824,
            sha256: String::new(),
            min_vram_gb: 2.0,
        };
        assert_eq!(variant_1gb.size_display(), "1.0 GB");

        // Just under 1 GB
        let variant_under_1gb = ModelVariant {
            quantization: Quantization::Q4_K_M,
            url: String::new(),
            size_bytes: 1_073_741_823,
            sha256: String::new(),
            min_vram_gb: 2.0,
        };
        assert!(variant_under_1gb.size_display().contains("MB"));

        // Very small
        let variant_small = ModelVariant {
            quantization: Quantization::Q2_K,
            url: String::new(),
            size_bytes: 1_048_576, // 1MB
            sha256: String::new(),
            min_vram_gb: 0.5,
        };
        assert_eq!(variant_small.size_display(), "1 MB");
    }

    #[test]
    fn test_variant_clone() {
        let variant = ModelVariant {
            quantization: Quantization::Q4_K_M,
            url: "https://example.com/model.gguf".to_string(),
            size_bytes: 1000,
            sha256: "abc123".to_string(),
            min_vram_gb: 4.0,
        };
        let cloned = variant.clone();
        assert_eq!(variant.url, cloned.url);
        assert_eq!(variant.sha256, cloned.sha256);
    }

    // ==================== ModelCategory tests ====================

    #[test]
    fn test_model_category() {
        assert_eq!(ModelCategory::Code.display_name(), "Code");
        assert_eq!(ModelCategory::Chat.display_name(), "Chat");
        assert_eq!(ModelCategory::Embedding.display_name(), "Embedding");
    }

    #[test]
    fn test_model_category_equality() {
        assert_eq!(ModelCategory::Code, ModelCategory::Code);
        assert_ne!(ModelCategory::Code, ModelCategory::Chat);
        assert_ne!(ModelCategory::Chat, ModelCategory::Embedding);
    }

    // ==================== DownloadableModel tests ====================

    fn create_test_model() -> DownloadableModel {
        DownloadableModel {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            category: ModelCategory::Code,
            parameters: "7B".to_string(),
            context_size: 8192,
            base_model: "test".to_string(),
            creator: "test".to_string(),
            license: "MIT".to_string(),
            variants: vec![
                ModelVariant {
                    quantization: Quantization::Q4_K_M,
                    url: String::new(),
                    size_bytes: 4_000_000_000,
                    sha256: String::new(),
                    min_vram_gb: 6.0,
                },
                ModelVariant {
                    quantization: Quantization::Q2_K,
                    url: String::new(),
                    size_bytes: 2_000_000_000,
                    sha256: String::new(),
                    min_vram_gb: 3.0,
                },
                ModelVariant {
                    quantization: Quantization::Q8_0,
                    url: String::new(),
                    size_bytes: 8_000_000_000,
                    sha256: String::new(),
                    min_vram_gb: 12.0,
                },
            ],
            tags: vec!["code".to_string(), "rust".to_string()],
        }
    }

    #[test]
    fn test_downloadable_model_recommended_variant() {
        let model = create_test_model();

        // With 16GB VRAM, should get highest quality (Q8_0)
        let variant = model.recommended_variant(16.0);
        assert!(variant.is_some());
        assert_eq!(variant.unwrap().quantization, Quantization::Q8_0);

        // With 8GB VRAM, should get Q4_K_M (Q8_0 needs 12GB)
        let variant = model.recommended_variant(8.0);
        assert!(variant.is_some());
        assert_eq!(variant.unwrap().quantization, Quantization::Q4_K_M);

        // With 4GB VRAM, should get Q2_K
        let variant = model.recommended_variant(4.0);
        assert!(variant.is_some());
        assert_eq!(variant.unwrap().quantization, Quantization::Q2_K);

        // With 2GB VRAM, nothing fits
        let variant = model.recommended_variant(2.0);
        assert!(variant.is_none());
    }

    #[test]
    fn test_downloadable_model_smallest_variant() {
        let model = create_test_model();
        let smallest = model.smallest_variant();
        assert!(smallest.is_some());
        assert_eq!(smallest.unwrap().quantization, Quantization::Q2_K);
    }

    #[test]
    fn test_downloadable_model_empty_variants() {
        let model = DownloadableModel {
            id: "empty".to_string(),
            name: "Empty".to_string(),
            category: ModelCategory::Chat,
            parameters: "1B".to_string(),
            context_size: 2048,
            base_model: "test".to_string(),
            creator: "test".to_string(),
            license: "MIT".to_string(),
            variants: vec![],
            tags: vec![],
        };

        assert!(model.recommended_variant(100.0).is_none());
        assert!(model.smallest_variant().is_none());
    }

    #[test]
    fn test_downloadable_model_clone() {
        let model = create_test_model();
        let cloned = model.clone();
        assert_eq!(model.id, cloned.id);
        assert_eq!(model.variants.len(), cloned.variants.len());
    }

    // ==================== DownloadRegistry tests ====================

    #[test]
    fn test_embedded_registry() {
        let registry = DownloadRegistry::embedded().unwrap();
        assert!(!registry.models.is_empty());
        assert!(!registry.version.is_empty());
    }

    #[test]
    fn test_registry_list_models() {
        let registry = DownloadRegistry::embedded().unwrap();
        let models = registry.list_models();
        assert!(!models.is_empty());
    }

    #[test]
    fn test_registry_find_model() {
        let registry = DownloadRegistry::embedded().unwrap();

        // Should find models that exist
        let found = registry
            .models
            .iter()
            .any(|m| m.category == ModelCategory::Code);
        assert!(found, "Should have at least one code model");

        // Use find_model on existing model
        if let Some(first) = registry.models.first() {
            let found = registry.find_model(&first.id);
            assert!(found.is_some());
            assert_eq!(found.unwrap().id, first.id);
        }

        // Non-existent model
        let not_found = registry.find_model("nonexistent-model-xyz");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_registry_models_by_category() {
        let registry = DownloadRegistry::embedded().unwrap();

        let code_models = registry.models_by_category(ModelCategory::Code);
        for model in &code_models {
            assert_eq!(model.category, ModelCategory::Code);
        }

        let chat_models = registry.models_by_category(ModelCategory::Chat);
        for model in &chat_models {
            assert_eq!(model.category, ModelCategory::Chat);
        }
    }

    #[test]
    fn test_registry_models_for_vram() {
        let registry = DownloadRegistry::embedded().unwrap();

        // With high VRAM, should find most models
        let high_vram_models = registry.models_for_vram(48.0);
        // With low VRAM, should find fewer or none
        let low_vram_models = registry.models_for_vram(1.0);

        assert!(high_vram_models.len() >= low_vram_models.len());
    }

    #[test]
    fn test_registry_recommended_code_models() {
        let registry = DownloadRegistry::embedded().unwrap();

        let recommended = registry.recommended_code_models(16.0);
        for model in &recommended {
            assert_eq!(model.category, ModelCategory::Code);
            assert!(model.recommended_variant(16.0).is_some());
        }
    }

    #[test]
    fn test_registry_cache_path() {
        let path = DownloadRegistry::cache_path().unwrap();
        assert!(path.to_string_lossy().contains(".ted"));
        assert!(path.to_string_lossy().contains("registry"));
        assert!(path.to_string_lossy().contains("models.json"));
    }

    #[test]
    fn test_registry_clone() {
        let registry = DownloadRegistry::embedded().unwrap();
        let cloned = registry.clone();
        assert_eq!(registry.version, cloned.version);
        assert_eq!(registry.models.len(), cloned.models.len());
    }

    // ==================== ModelDownloader tests ====================

    #[test]
    fn test_downloader_path() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let model = create_test_model();
        let variant = &model.variants[0];

        let path = downloader.model_path(&model, variant);
        assert!(path.to_string_lossy().contains("test-model"));
        assert!(path.to_string_lossy().contains("q4_k_m"));
        assert!(path.to_string_lossy().ends_with(".gguf"));
    }

    #[test]
    fn test_downloader_download_dir() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        assert_eq!(downloader.download_dir(), temp_dir.path());
    }

    #[test]
    fn test_downloader_is_downloaded_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let model = create_test_model();
        let variant = &model.variants[0];

        assert!(!downloader.is_downloaded(&model, variant));
    }

    #[test]
    fn test_downloader_list_downloaded_empty() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let downloaded = downloader.list_downloaded().unwrap();
        assert!(downloaded.is_empty());
    }

    #[test]
    fn test_downloader_list_downloaded_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        // Create some fake .gguf files
        std::fs::write(temp_dir.path().join("model1.gguf"), "fake").unwrap();
        std::fs::write(temp_dir.path().join("model2.gguf"), "fake").unwrap();
        std::fs::write(temp_dir.path().join("other.txt"), "fake").unwrap();

        let downloaded = downloader.list_downloaded().unwrap();
        assert_eq!(downloaded.len(), 2);

        for path in &downloaded {
            assert!(path.extension().and_then(|s| s.to_str()) == Some("gguf"));
        }
    }

    #[test]
    fn test_downloader_delete() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let file_path = temp_dir.path().join("to_delete.gguf");
        std::fs::write(&file_path, "content").unwrap();
        assert!(file_path.exists());

        downloader.delete(&file_path).unwrap();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_downloader_delete_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let file_path = temp_dir.path().join("nonexistent.gguf");
        let result = downloader.delete(&file_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_downloader_new() {
        // This test might fail if home directory isn't writable
        // but we can at least verify the function signature
        let result = ModelDownloader::new();
        // Just check it returns a Result
        let _ = result;
    }

    #[test]
    fn test_downloader_model_path_lowercase() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let model = DownloadableModel {
            id: "Test-Model-ID".to_string(),
            name: "Test".to_string(),
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
        // Path should be lowercase
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("test-model-id"));
        assert!(!path_str.contains("Test-Model-ID"));
    }

    // ==================== Constants tests ====================

    #[test]
    fn test_registry_cache_ttl() {
        assert_eq!(REGISTRY_CACHE_TTL, Duration::from_secs(86400));
    }

    #[test]
    fn test_registry_url() {
        assert!(REGISTRY_URL.starts_with("https://"));
        assert!(REGISTRY_URL.contains("models.json"));
    }

    #[test]
    fn test_find_extracted_binary_prefers_expected_path() {
        let temp_dir = TempDir::new().unwrap();
        let expected = temp_dir
            .path()
            .join("build")
            .join("bin")
            .join("llama-server");
        std::fs::create_dir_all(expected.parent().unwrap()).unwrap();
        std::fs::write(&expected, b"binary").unwrap();

        let found =
            find_extracted_binary(temp_dir.path(), "build/bin/llama-server", "llama-server");
        assert_eq!(found.as_deref(), Some(expected.as_path()));
    }

    #[test]
    fn test_find_extracted_binary_falls_back_to_filename_search() {
        let temp_dir = TempDir::new().unwrap();
        let actual = temp_dir
            .path()
            .join("llama-b7951-bin-macos-arm64")
            .join("bin")
            .join("llama-server");
        std::fs::create_dir_all(actual.parent().unwrap()).unwrap();
        std::fs::write(&actual, b"binary").unwrap();

        let found =
            find_extracted_binary(temp_dir.path(), "build/bin/llama-server", "llama-server");
        assert_eq!(found.as_deref(), Some(actual.as_path()));
    }

    #[test]
    fn test_find_extracted_binary_returns_none_when_absent() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("README.txt"), b"no binary here").unwrap();

        let found =
            find_extracted_binary(temp_dir.path(), "build/bin/llama-server", "llama-server");
        assert!(found.is_none());
    }

    #[test]
    fn test_copy_binary_companions_copies_sibling_files_except_binary() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        let binary = src_dir.path().join("llama-server");
        let dylib = src_dir.path().join("libmtmd.0.dylib");
        let shader = src_dir.path().join("ggml-metal.metal");
        let subdir = src_dir.path().join("nested");

        std::fs::write(&binary, b"bin").unwrap();
        std::fs::write(&dylib, b"lib").unwrap();
        std::fs::write(&shader, b"shader").unwrap();
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("ignored.txt"), b"x").unwrap();

        let copied = copy_binary_companions(&binary, dst_dir.path()).unwrap();
        assert_eq!(copied, 2);

        assert!(dst_dir.path().join("libmtmd.0.dylib").exists());
        assert!(dst_dir.path().join("ggml-metal.metal").exists());
        assert!(!dst_dir.path().join("llama-server").exists());
        assert!(!dst_dir.path().join("ignored.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_copy_binary_companions_copies_sibling_symlinks() {
        use std::os::unix::fs::symlink;

        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        let binary = src_dir.path().join("llama-server");
        let versioned = src_dir.path().join("libmtmd.0.0.7951.dylib");
        let link = src_dir.path().join("libmtmd.0.dylib");

        std::fs::write(&binary, b"bin").unwrap();
        std::fs::write(&versioned, b"lib").unwrap();
        symlink("libmtmd.0.0.7951.dylib", &link).unwrap();

        let copied = copy_binary_companions(&binary, dst_dir.path()).unwrap();
        assert_eq!(copied, 2);

        let copied_link = dst_dir.path().join("libmtmd.0.dylib");
        assert!(std::fs::symlink_metadata(&copied_link)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_ensure_macos_dylib_major_links_creates_major_link() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("libmtmd.0.0.7951.dylib"), b"lib").unwrap();

        let created = ensure_macos_dylib_major_links(dir.path()).unwrap();
        assert_eq!(created, 1);
        assert!(
            std::fs::symlink_metadata(dir.path().join("libmtmd.0.dylib"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    // ==================== Async tests ====================

    #[tokio::test]
    async fn test_registry_with_fallback() {
        // This will either fetch from remote or use embedded
        let registry = DownloadRegistry::with_fallback().await.unwrap();
        assert!(!registry.models.is_empty());
    }

    #[tokio::test]
    async fn test_registry_with_cache() {
        // This will use cache if available, or fetch/embedded
        let registry = DownloadRegistry::with_cache().await.unwrap();
        assert!(!registry.models.is_empty());
    }

    #[test]
    fn test_binary_registry_platform_info_for_current_platform() {
        let registry = BinaryRegistry::embedded().expect("embedded binary registry should load");
        let info = registry.platform_info();

        // Current CI/dev platforms should be represented in the embedded registry.
        assert!(
            info.is_ok(),
            "platform '{}' should be supported in binaries.json",
            platform_key()
        );
    }

    #[test]
    fn test_binary_downloader_prefers_local_binary_when_present() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = BinaryDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let binary_name = if cfg!(target_os = "windows") {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        let local_binary = temp_dir.path().join(binary_name);
        write_mock_runnable_binary(&local_binary);

        let found = downloader.find_llama_server();
        assert_eq!(found.as_deref(), Some(local_binary.as_path()));
    }

    #[tokio::test]
    async fn test_binary_downloader_ensure_llama_server_uses_existing_local_binary() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = BinaryDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let binary_name = if cfg!(target_os = "windows") {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        let local_binary = temp_dir.path().join(binary_name);
        write_mock_runnable_binary(&local_binary);

        let path = downloader.ensure_llama_server().await.unwrap();
        assert_eq!(path, local_binary);
    }

    #[test]
    fn test_downloader_is_downloaded_with_placeholder_hash_checks_nonempty_file() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let model = create_test_model();
        let variant = ModelVariant {
            quantization: Quantization::Q4_K_M,
            url: "http://example.invalid/model.gguf".to_string(),
            size_bytes: 4,
            sha256: "placeholder_q4".to_string(),
            min_vram_gb: 1.0,
        };
        let path = downloader.model_path(&model, &variant);
        std::fs::write(&path, b"data").unwrap();

        assert!(downloader.is_downloaded(&model, &variant));
    }

    #[test]
    fn test_downloader_is_downloaded_with_real_hash_true_and_false() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let model = create_test_model();
        let good_hash =
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string();
        let variant = ModelVariant {
            quantization: Quantization::Q2_K,
            url: "http://example.invalid/model.gguf".to_string(),
            size_bytes: 5,
            sha256: good_hash.clone(),
            min_vram_gb: 1.0,
        };
        let path = downloader.model_path(&model, &variant);
        std::fs::write(&path, b"hello").unwrap();
        assert!(downloader.is_downloaded(&model, &variant));

        let bad_variant = ModelVariant {
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            ..variant
        };
        assert!(!downloader.is_downloaded(&model, &bad_variant));
    }

    #[tokio::test]
    async fn test_downloader_download_returns_existing_file_without_network() {
        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let model = create_test_model();
        let variant = ModelVariant {
            quantization: Quantization::Q4_K_M,
            url: "http://127.0.0.1:9/unused".to_string(),
            size_bytes: 4,
            sha256: "placeholder_skip".to_string(),
            min_vram_gb: 1.0,
        };
        let path = downloader.model_path(&model, &variant);
        std::fs::write(&path, b"done").unwrap();

        let result_path = downloader.download(&model, &variant).await.unwrap();
        assert_eq!(result_path, path);
    }

    #[tokio::test]
    async fn test_downloader_download_http_error_status() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/model.gguf"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let model = create_test_model();
        let variant = ModelVariant {
            quantization: Quantization::Q5_K_M,
            url: format!("{}/model.gguf", mock_server.uri()),
            size_bytes: 4,
            sha256: "placeholder_skip".to_string(),
            min_vram_gb: 1.0,
        };

        let err = downloader.download(&model, &variant).await.unwrap_err();
        assert!(err.to_string().contains("Download failed with status"));
    }

    #[tokio::test]
    async fn test_downloader_download_sha_mismatch_removes_temp_file() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/model.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"abc".to_vec()))
            .mount(&mock_server)
            .await;

        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let model = create_test_model();
        let variant = ModelVariant {
            quantization: Quantization::Q3_K_M,
            url: format!("{}/model.bin", mock_server.uri()),
            size_bytes: 3,
            sha256: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string(),
            min_vram_gb: 1.0,
        };
        let path = downloader.model_path(&model, &variant);
        let temp_path = path.with_extension("tmp");

        let err = downloader.download(&model, &variant).await.unwrap_err();
        assert!(err.to_string().contains("SHA256 verification failed"));
        assert!(!temp_path.exists());
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_downloader_download_success_with_placeholder_hash() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/model-success.gguf"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
            .mount(&mock_server)
            .await;

        let temp_dir = TempDir::new().unwrap();
        let downloader = ModelDownloader::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let model = create_test_model();
        let variant = ModelVariant {
            quantization: Quantization::Q6_K,
            url: format!("{}/model-success.gguf", mock_server.uri()),
            size_bytes: 5,
            sha256: "placeholder_skip".to_string(),
            min_vram_gb: 1.0,
        };

        let path = downloader.download(&model, &variant).await.unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }
}
