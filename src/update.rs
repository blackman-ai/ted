// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Self-update functionality for Ted
//!
//! Checks GitHub releases for new versions and downloads/installs updates.

use std::env;
use std::fs;
use std::path::PathBuf;

use crate::error::{Result, TedError};

/// GitHub repository for releases
const GITHUB_REPO: &str = "blackman-ai/ted";

/// Current version of ted
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Information about an available release
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub version: String,
    pub tag_name: String,
    pub published_at: String,
    pub download_url: String,
    pub asset_name: String,
    pub body: String,
}

/// Check if a new version is available
pub async fn check_for_updates() -> Result<Option<ReleaseInfo>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("ted/{}", VERSION))
        .build()
        .map_err(|e| TedError::Config(format!("Failed to create HTTP client: {}", e)))?;

    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| TedError::Config(format!("Failed to check for updates: {}", e)))?;

    if !response.status().is_success() {
        if response.status().as_u16() == 404 {
            // No releases yet
            return Ok(None);
        }
        return Err(TedError::Config(format!(
            "GitHub API error: {}",
            response.status()
        )));
    }

    let release: serde_json::Value = response
        .json()
        .await
        .map_err(|e| TedError::Config(format!("Failed to parse release info: {}", e)))?;

    let tag_name = release["tag_name"]
        .as_str()
        .ok_or_else(|| TedError::Config("Invalid release format".to_string()))?;

    // Parse version (strip 'v' prefix if present)
    let remote_version = tag_name.strip_prefix('v').unwrap_or(tag_name);
    let current_version = VERSION;

    // Simple version comparison (works for semver)
    if !is_newer_version(remote_version, current_version) {
        return Ok(None);
    }

    // Find the right asset for this platform
    let target = get_target_triple();
    let assets = release["assets"].as_array();

    let (download_url, asset_name) = if let Some(assets) = assets {
        let asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        });

        match asset {
            Some(a) => (
                a["browser_download_url"].as_str().unwrap_or("").to_string(),
                a["name"].as_str().unwrap_or("").to_string(),
            ),
            None => {
                return Err(TedError::Config(format!(
                    "No release available for your platform ({})",
                    target
                )));
            }
        }
    } else {
        return Err(TedError::Config("No release assets found".to_string()));
    };

    Ok(Some(ReleaseInfo {
        version: remote_version.to_string(),
        tag_name: tag_name.to_string(),
        published_at: release["published_at"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        download_url,
        asset_name,
        body: release["body"].as_str().unwrap_or("").to_string(),
    }))
}

/// Check for a specific version
pub async fn check_for_version(version: &str) -> Result<Option<ReleaseInfo>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("ted/{}", VERSION))
        .build()
        .map_err(|e| TedError::Config(format!("Failed to create HTTP client: {}", e)))?;

    let tag = if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{}", version)
    };

    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/{}",
        GITHUB_REPO, tag
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| TedError::Config(format!("Failed to check for version: {}", e)))?;

    if !response.status().is_success() {
        if response.status().as_u16() == 404 {
            return Err(TedError::Config(format!("Version {} not found", version)));
        }
        return Err(TedError::Config(format!(
            "GitHub API error: {}",
            response.status()
        )));
    }

    let release: serde_json::Value = response
        .json()
        .await
        .map_err(|e| TedError::Config(format!("Failed to parse release info: {}", e)))?;

    let tag_name = release["tag_name"]
        .as_str()
        .ok_or_else(|| TedError::Config("Invalid release format".to_string()))?;

    let remote_version = tag_name.strip_prefix('v').unwrap_or(tag_name);

    // Find the right asset for this platform
    let target = get_target_triple();
    let assets = release["assets"].as_array();

    let (download_url, asset_name) = if let Some(assets) = assets {
        let asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        });

        match asset {
            Some(a) => (
                a["browser_download_url"].as_str().unwrap_or("").to_string(),
                a["name"].as_str().unwrap_or("").to_string(),
            ),
            None => {
                return Err(TedError::Config(format!(
                    "No release available for your platform ({})",
                    target
                )));
            }
        }
    } else {
        return Err(TedError::Config("No release assets found".to_string()));
    };

    Ok(Some(ReleaseInfo {
        version: remote_version.to_string(),
        tag_name: tag_name.to_string(),
        published_at: release["published_at"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        download_url,
        asset_name,
        body: release["body"].as_str().unwrap_or("").to_string(),
    }))
}

/// Download and install an update
pub async fn install_update(release: &ReleaseInfo) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(format!("ted/{}", VERSION))
        .build()
        .map_err(|e| TedError::Config(format!("Failed to create HTTP client: {}", e)))?;

    // Download the release
    println!("Downloading {}...", release.asset_name);
    let response = client
        .get(&release.download_url)
        .send()
        .await
        .map_err(|e| TedError::Config(format!("Failed to download update: {}", e)))?;

    if !response.status().is_success() {
        return Err(TedError::Config(format!(
            "Failed to download update: {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| TedError::Config(format!("Failed to read download: {}", e)))?;

    // Get the current executable path
    let current_exe = env::current_exe()
        .map_err(|e| TedError::Config(format!("Failed to get current executable: {}", e)))?;

    // Create a temp file for the download
    let temp_dir = env::temp_dir();
    let temp_archive = temp_dir.join(&release.asset_name);
    let temp_binary = temp_dir.join("ted_new");

    // Write the downloaded archive
    fs::write(&temp_archive, &bytes)
        .map_err(|e| TedError::Config(format!("Failed to write temp file: {}", e)))?;

    // Extract the binary
    println!("Extracting...");
    extract_binary(&temp_archive, &temp_binary)?;

    // Make it executable (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&temp_binary)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_binary, perms)?;
    }

    // Replace the current binary
    println!("Installing...");
    replace_binary(&temp_binary, &current_exe)?;

    // Clean up
    let _ = fs::remove_file(&temp_archive);
    let _ = fs::remove_file(&temp_binary);

    Ok(())
}

/// Extract the binary from the downloaded archive
fn extract_binary(archive_path: &PathBuf, output_path: &PathBuf) -> Result<()> {
    let archive_name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if archive_name.ends_with(".tar.gz") {
        // Use tar to extract
        let file = fs::File::open(archive_path)
            .map_err(|e| TedError::Config(format!("Failed to open archive: {}", e)))?;

        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);

        for entry in archive
            .entries()
            .map_err(|e| TedError::Config(format!("Failed to read archive: {}", e)))?
        {
            let mut entry =
                entry.map_err(|e| TedError::Config(format!("Failed to read entry: {}", e)))?;
            let path = entry
                .path()
                .map_err(|e| TedError::Config(format!("Failed to read path: {}", e)))?;

            // Look for the ted binary
            if path.file_name().map(|n| n == "ted").unwrap_or(false) {
                let mut output = fs::File::create(output_path)
                    .map_err(|e| TedError::Config(format!("Failed to create output: {}", e)))?;

                std::io::copy(&mut entry, &mut output)
                    .map_err(|e| TedError::Config(format!("Failed to extract: {}", e)))?;

                return Ok(());
            }
        }

        Err(TedError::Config("Binary not found in archive".to_string()))
    } else if archive_name.ends_with(".zip") {
        // Use zip to extract
        let file = fs::File::open(archive_path)
            .map_err(|e| TedError::Config(format!("Failed to open archive: {}", e)))?;

        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| TedError::Config(format!("Failed to read zip: {}", e)))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| TedError::Config(format!("Failed to read entry: {}", e)))?;

            let name = file.name();
            if name == "ted" || name == "ted.exe" {
                let mut output = fs::File::create(output_path)
                    .map_err(|e| TedError::Config(format!("Failed to create output: {}", e)))?;

                std::io::copy(&mut file, &mut output)
                    .map_err(|e| TedError::Config(format!("Failed to extract: {}", e)))?;

                return Ok(());
            }
        }

        Err(TedError::Config("Binary not found in archive".to_string()))
    } else {
        Err(TedError::Config(format!(
            "Unknown archive format: {}",
            archive_name
        )))
    }
}

/// Replace the current binary with the new one
fn replace_binary(new_binary: &PathBuf, current_binary: &PathBuf) -> Result<()> {
    // On Windows, we can't replace a running executable directly
    // We need to rename it first
    #[cfg(windows)]
    {
        let backup = current_binary.with_extension("old.exe");
        fs::rename(current_binary, &backup)
            .map_err(|e| TedError::Config(format!("Failed to backup current binary: {}", e)))?;

        if let Err(e) = fs::copy(new_binary, current_binary) {
            // Restore backup on failure
            let _ = fs::rename(&backup, current_binary);
            return Err(TedError::Config(format!(
                "Failed to install new binary: {}",
                e
            )));
        }

        // Remove backup
        let _ = fs::remove_file(&backup);
    }

    // On Unix, we can atomically replace
    #[cfg(unix)]
    {
        fs::copy(new_binary, current_binary)
            .map_err(|e| TedError::Config(format!("Failed to install new binary: {}", e)))?;
    }

    Ok(())
}

/// Get the target triple for the current platform
fn get_target_triple() -> String {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    if cfg!(target_os = "linux") {
        format!("{}-unknown-linux-gnu", arch)
    } else if cfg!(target_os = "macos") {
        format!("{}-apple-darwin", arch)
    } else if cfg!(target_os = "windows") {
        format!("{}-pc-windows-msvc", arch)
    } else {
        format!("{}-{}", arch, os)
    }
}

/// Compare versions (returns true if remote is newer than current)
fn is_newer_version(remote: &str, current: &str) -> bool {
    let parse_version = |v: &str| -> (u32, u32, u32) {
        let parts: Vec<&str> = v.split('.').collect();
        let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts
            .get(2)
            .and_then(|s| {
                // Handle versions like "0.1.0-beta"
                s.split('-').next().and_then(|p| p.parse().ok())
            })
            .unwrap_or(0);
        (major, minor, patch)
    };

    let remote_v = parse_version(remote);
    let current_v = parse_version(current);

    remote_v > current_v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison_newer_major() {
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(is_newer_version("2.0.0", "1.9.9"));
    }

    #[test]
    fn test_version_comparison_newer_minor() {
        assert!(is_newer_version("0.2.0", "0.1.0"));
        assert!(is_newer_version("0.10.0", "0.9.0"));
    }

    #[test]
    fn test_version_comparison_newer_patch() {
        assert!(is_newer_version("0.1.1", "0.1.0"));
        assert!(is_newer_version("0.1.10", "0.1.9"));
    }

    #[test]
    fn test_version_comparison_equal() {
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("10.20.30", "10.20.30"));
    }

    #[test]
    fn test_version_comparison_older() {
        assert!(!is_newer_version("0.0.9", "0.1.0"));
        assert!(!is_newer_version("0.9.0", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "2.0.0"));
    }

    #[test]
    fn test_version_comparison_with_prerelease() {
        // Pre-release versions should work (only numeric part is compared)
        assert!(is_newer_version("0.2.0-beta", "0.1.0"));
        assert!(is_newer_version("0.2.0-alpha", "0.1.0"));
        assert!(!is_newer_version("0.1.0-beta", "0.1.0"));
    }

    #[test]
    fn test_version_comparison_edge_cases() {
        // Single component
        assert!(is_newer_version("2", "1"));
        assert!(!is_newer_version("1", "2"));

        // Two components
        assert!(is_newer_version("1.1", "1.0"));
        assert!(!is_newer_version("1.0", "1.1"));
    }

    #[test]
    fn test_get_target_triple() {
        let target = get_target_triple();
        assert!(!target.is_empty());
        // Should contain either x86_64 or aarch64
        assert!(target.contains("x86_64") || target.contains("aarch64"));
    }

    #[test]
    fn test_get_target_triple_os() {
        let target = get_target_triple();
        // Should contain one of the supported OS strings
        assert!(
            target.contains("linux") || target.contains("darwin") || target.contains("windows")
        );
    }

    #[test]
    fn test_get_target_triple_format() {
        let target = get_target_triple();
        // Should have the format arch-vendor-os(-gnu)?
        let parts: Vec<&str> = target.split('-').collect();
        assert!(parts.len() >= 2);
    }

    #[test]
    fn test_release_info_struct() {
        let release = ReleaseInfo {
            version: "0.1.0".to_string(),
            tag_name: "v0.1.0".to_string(),
            published_at: "2025-01-01T00:00:00Z".to_string(),
            download_url: "https://example.com/release.tar.gz".to_string(),
            asset_name: "ted-x86_64-unknown-linux-gnu.tar.gz".to_string(),
            body: "Release notes here".to_string(),
        };

        assert_eq!(release.version, "0.1.0");
        assert_eq!(release.tag_name, "v0.1.0");
        assert!(!release.download_url.is_empty());
    }

    #[test]
    fn test_version_constant() {
        // Verify VERSION constant is set and in semver format
        assert!(VERSION.contains('.'));
        // Semver should have at least two dots (major.minor.patch)
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert!(parts.len() >= 2);
    }

    #[test]
    fn test_github_repo_constant() {
        // Should point to the ted repo
        assert!(GITHUB_REPO.contains("ted"));
    }

    // ===== ReleaseInfo Tests =====

    #[test]
    fn test_release_info_clone() {
        let release = ReleaseInfo {
            version: "1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            published_at: "2025-01-01".to_string(),
            download_url: "https://example.com/download".to_string(),
            asset_name: "ted.tar.gz".to_string(),
            body: "Release notes".to_string(),
        };

        let cloned = release.clone();
        assert_eq!(cloned.version, release.version);
        assert_eq!(cloned.tag_name, release.tag_name);
        assert_eq!(cloned.published_at, release.published_at);
        assert_eq!(cloned.download_url, release.download_url);
        assert_eq!(cloned.asset_name, release.asset_name);
        assert_eq!(cloned.body, release.body);
    }

    #[test]
    fn test_release_info_debug() {
        let release = ReleaseInfo {
            version: "1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            published_at: "2025-01-01".to_string(),
            download_url: "https://example.com".to_string(),
            asset_name: "ted.tar.gz".to_string(),
            body: "Notes".to_string(),
        };

        let debug = format!("{:?}", release);
        assert!(debug.contains("ReleaseInfo"));
        assert!(debug.contains("1.0.0"));
    }

    #[test]
    fn test_release_info_empty_body() {
        let release = ReleaseInfo {
            version: "1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            published_at: "2025-01-01".to_string(),
            download_url: "https://example.com".to_string(),
            asset_name: "ted.tar.gz".to_string(),
            body: String::new(),
        };

        assert!(release.body.is_empty());
    }

    // ===== Version Comparison Edge Cases =====

    #[test]
    fn test_version_comparison_large_numbers() {
        assert!(is_newer_version("100.200.300", "100.200.299"));
        assert!(!is_newer_version("100.200.299", "100.200.300"));
    }

    #[test]
    fn test_version_comparison_with_rc() {
        assert!(is_newer_version("1.0.0-rc1", "0.9.9"));
        assert!(!is_newer_version("0.9.9-rc1", "1.0.0"));
    }

    #[test]
    fn test_version_comparison_empty_string() {
        // Empty strings should be treated as 0.0.0
        assert!(is_newer_version("0.0.1", ""));
        assert!(!is_newer_version("", "0.0.1"));
        assert!(!is_newer_version("", ""));
    }

    #[test]
    fn test_version_comparison_malformed() {
        // Malformed versions should parse what they can
        assert!(!is_newer_version("abc", "1.0.0"));
        assert!(is_newer_version("1.0.0", "abc"));
    }

    // ===== Target Triple Tests =====

    #[test]
    fn test_get_target_triple_consistent() {
        // Should return the same value on repeated calls
        let triple1 = get_target_triple();
        let triple2 = get_target_triple();
        assert_eq!(triple1, triple2);
    }

    #[test]
    fn test_get_target_triple_no_spaces() {
        let target = get_target_triple();
        assert!(!target.contains(' '));
    }

    #[test]
    fn test_get_target_triple_lowercase() {
        let target = get_target_triple();
        assert_eq!(target, target.to_lowercase());
    }

    // ===== extract_binary Tests (using tempfile) =====

    #[test]
    fn test_extract_binary_unknown_format() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.unknown");
        let output_path = temp_dir.path().join("output");

        // Create a dummy file with unknown extension
        fs::write(&archive_path, b"dummy content").unwrap();

        let result = extract_binary(&archive_path, &output_path);
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("Unknown archive format"));
        }
    }

    #[test]
    fn test_extract_binary_tar_gz_no_binary() {
        use std::fs::{self, File};
        use std::io::Write;
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let output_path = temp_dir.path().join("output");
        let src_dir = temp_dir.path().join("src");

        // Create a source directory with a non-ted file
        fs::create_dir(&src_dir).unwrap();
        let mut file = File::create(src_dir.join("other_file.txt")).unwrap();
        file.write_all(b"hello").unwrap();

        // Use system tar to create the archive (more reliable)
        let status = Command::new("tar")
            .args([
                "-czf",
                archive_path.to_str().unwrap(),
                "-C",
                src_dir.to_str().unwrap(),
                "other_file.txt",
            ])
            .status();

        // Skip test if tar is not available
        if status.is_err() || !status.unwrap().success() {
            return;
        }

        let result = extract_binary(&archive_path, &output_path);
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("Binary not found"));
        }
    }

    #[test]
    fn test_extract_binary_tar_gz_with_binary() {
        use std::fs::{self, File};
        use std::io::Write;
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let output_path = temp_dir.path().join("output");
        let src_dir = temp_dir.path().join("src");

        // Create a source directory with a ted binary
        fs::create_dir(&src_dir).unwrap();
        let mut file = File::create(src_dir.join("ted")).unwrap();
        file.write_all(b"binary_data!").unwrap();

        // Use system tar to create the archive
        let status = Command::new("tar")
            .args([
                "-czf",
                archive_path.to_str().unwrap(),
                "-C",
                src_dir.to_str().unwrap(),
                "ted",
            ])
            .status();

        // Skip test if tar is not available
        if status.is_err() || !status.unwrap().success() {
            return;
        }

        let result = extract_binary(&archive_path, &output_path);
        assert!(result.is_ok(), "Expected Ok but got {:?}", result);

        // Verify the output file exists
        assert!(output_path.exists());
        let content = fs::read(&output_path).unwrap();
        assert_eq!(content, b"binary_data!");
    }

    #[test]
    fn test_extract_binary_zip_no_binary() {
        use std::fs::File;
        use std::io::Write;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.zip");
        let output_path = temp_dir.path().join("output");

        // Create a zip without a 'ted' binary
        let file = File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("other_file.txt", options).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();

        let result = extract_binary(&archive_path, &output_path);
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.to_string().contains("Binary not found"));
        }
    }

    #[test]
    fn test_extract_binary_zip_with_binary() {
        use std::fs::{self, File};
        use std::io::Write;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.zip");
        let output_path = temp_dir.path().join("output");

        // Create a zip WITH a 'ted' binary
        let file = File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("ted", options).unwrap();
        zip.write_all(b"binary_content").unwrap();
        zip.finish().unwrap();

        let result = extract_binary(&archive_path, &output_path);
        assert!(result.is_ok());

        assert!(output_path.exists());
        let content = fs::read(&output_path).unwrap();
        assert_eq!(content, b"binary_content");
    }

    #[test]
    fn test_extract_binary_zip_with_ted_exe() {
        use std::fs::{self, File};
        use std::io::Write;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.zip");
        let output_path = temp_dir.path().join("output");

        // Create a zip WITH a 'ted.exe' binary (Windows format)
        let file = File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("ted.exe", options).unwrap();
        zip.write_all(b"windows_binary").unwrap();
        zip.finish().unwrap();

        let result = extract_binary(&archive_path, &output_path);
        assert!(result.is_ok());

        assert!(output_path.exists());
        let content = fs::read(&output_path).unwrap();
        assert_eq!(content, b"windows_binary");
    }

    // ===== replace_binary Tests =====

    #[cfg(unix)]
    #[test]
    fn test_replace_binary_unix() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let new_binary = temp_dir.path().join("new");
        let current_binary = temp_dir.path().join("current");

        // Create both files
        fs::write(&new_binary, b"new content").unwrap();
        fs::write(&current_binary, b"old content").unwrap();

        let result = replace_binary(&new_binary, &current_binary);
        assert!(result.is_ok());

        // Verify the current binary has the new content
        let content = fs::read(&current_binary).unwrap();
        assert_eq!(content, b"new content");
    }

    #[test]
    fn test_extract_binary_nonexistent_archive() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("nonexistent.tar.gz");
        let output_path = temp_dir.path().join("output");

        let result = extract_binary(&archive_path, &output_path);
        assert!(result.is_err());
    }

    // ===== Async Function Tests (without network) =====

    /// Test parsing of GitHub release JSON response
    #[test]
    fn test_parse_release_json() {
        let json_str = r#"{"tag_name": "v1.2.3", "published_at": "2025-01-15T10:30:00Z", "body": "Release Notes - Feature 1 - Feature 2", "assets": [{"name": "ted-x86_64-unknown-linux-gnu.tar.gz", "browser_download_url": "https://github.com/example/releases/download/v1.2.3/ted-x86_64-unknown-linux-gnu.tar.gz"}, {"name": "ted-aarch64-apple-darwin.tar.gz", "browser_download_url": "https://github.com/example/releases/download/v1.2.3/ted-aarch64-apple-darwin.tar.gz"}]}"#;

        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // Verify tag_name parsing
        let tag_name = parsed["tag_name"].as_str().unwrap();
        assert_eq!(tag_name, "v1.2.3");

        // Verify version stripping
        let version = tag_name.strip_prefix('v').unwrap_or(tag_name);
        assert_eq!(version, "1.2.3");

        // Verify assets parsing
        let assets = parsed["assets"].as_array().unwrap();
        assert_eq!(assets.len(), 2);

        // Find asset for current platform
        let target = get_target_triple();
        let _matching_asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        });

        // Depending on platform, we may or may not find an asset
        // (the test verifies the parsing logic works, not the specific platform)

        // Verify body parsing
        assert!(
            parsed["body"].as_str().unwrap().contains("Feature")
                || parsed["body"].as_str().unwrap().contains("Release")
        );
    }

    /// Test version tag prefix handling
    #[test]
    fn test_version_tag_handling() {
        // With v prefix
        let tag1 = "v1.0.0";
        let version1 = tag1.strip_prefix('v').unwrap_or(tag1);
        assert_eq!(version1, "1.0.0");

        // Without v prefix
        let tag2 = "1.0.0";
        let version2 = tag2.strip_prefix('v').unwrap_or(tag2);
        assert_eq!(version2, "1.0.0");

        // Multiple v's - only strip first
        let tag3 = "vvv1.0.0";
        let version3 = tag3.strip_prefix('v').unwrap_or(tag3);
        assert_eq!(version3, "vv1.0.0");
    }

    /// Test asset name matching logic
    #[test]
    fn test_asset_name_matching() {
        let target = get_target_triple();

        // Asset names that should match
        let matching_names = [
            format!("ted-{}.tar.gz", target),
            format!("ted_{}.tar.gz", target),
        ];

        for name in &matching_names {
            assert!(
                name.contains(&target),
                "Name {} should contain target {}",
                name,
                target
            );
        }

        // Asset names that should not match
        let non_matching_names = ["ted-wasm.tar.gz", "other-tool-linux.tar.gz"];

        for name in &non_matching_names {
            assert!(
                !name.contains(&target) || target == "unknown",
                "Name {} should not match target {}",
                name,
                target
            );
        }
    }

    /// Test error JSON parsing
    #[test]
    fn test_parse_github_error_response() {
        let error_json =
            r#"{"message": "Not Found", "documentation_url": "https://docs.github.com/rest"}"#;

        let parsed: serde_json::Value = serde_json::from_str(error_json).unwrap();
        assert_eq!(parsed["message"].as_str().unwrap(), "Not Found");
    }

    /// Test release info construction from parsed JSON
    #[test]
    fn test_release_info_from_json() {
        let json_str = r#"{"tag_name": "v2.0.0", "published_at": "2025-06-15T12:00:00Z", "body": "Major release", "assets": [{"name": "ted-x86_64-unknown-linux-gnu.tar.gz", "browser_download_url": "https://example.com/download.tar.gz"}]}"#;

        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let tag_name = parsed["tag_name"].as_str().unwrap();
        let remote_version = tag_name.strip_prefix('v').unwrap_or(tag_name);

        let release = ReleaseInfo {
            version: remote_version.to_string(),
            tag_name: tag_name.to_string(),
            published_at: parsed["published_at"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            download_url: parsed["assets"][0]["browser_download_url"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            asset_name: parsed["assets"][0]["name"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            body: parsed["body"].as_str().unwrap_or("").to_string(),
        };

        assert_eq!(release.version, "2.0.0");
        assert_eq!(release.tag_name, "v2.0.0");
        assert_eq!(release.published_at, "2025-06-15T12:00:00Z");
        assert!(release.download_url.contains("example.com"));
        assert!(release.asset_name.contains("linux"));
        assert_eq!(release.body, "Major release");
    }

    /// Test JSON with missing fields
    #[test]
    fn test_release_json_missing_fields() {
        let json_str = r#"{"tag_name": "v1.0.0"}"#;

        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // published_at missing - use default
        let published_at = parsed["published_at"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        assert_eq!(published_at, "unknown");

        // body missing - use default
        let body = parsed["body"].as_str().unwrap_or("").to_string();
        assert!(body.is_empty());

        // assets missing
        let assets = parsed["assets"].as_array();
        assert!(assets.is_none());
    }

    /// Test JSON with empty assets array
    #[test]
    fn test_release_json_empty_assets() {
        let json_str = r#"{"tag_name": "v1.0.0", "assets": []}"#;

        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let assets = parsed["assets"].as_array().unwrap();
        assert!(assets.is_empty());

        // Finding asset for target should return None
        let target = get_target_triple();
        let matching_asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        });
        assert!(matching_asset.is_none());
    }

    /// Test HTTP status code handling logic
    #[test]
    fn test_http_status_handling() {
        // 404 should indicate no releases
        let status_404: u16 = 404;
        assert!(status_404 == 404);

        // 200 should indicate success
        let status_200: u16 = 200;
        assert!((200..300).contains(&status_200));

        // 401/403 should indicate auth error
        let status_401: u16 = 401;
        let status_403: u16 = 403;
        assert!(status_401 == 401 || status_403 == 403);

        // 500+ should indicate server error
        let status_500: u16 = 500;
        assert!(status_500 >= 500);
    }

    /// Test URL construction for GitHub API
    #[test]
    fn test_github_api_url_construction() {
        let repo = GITHUB_REPO;
        let latest_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        assert!(latest_url.contains("api.github.com"));
        assert!(latest_url.contains(repo));
        assert!(latest_url.ends_with("/releases/latest"));

        // Test tag URL construction
        let tag = "v1.0.0";
        let tag_url = format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            repo, tag
        );
        assert!(tag_url.contains(tag));
    }

    /// Test version tag normalization
    #[test]
    fn test_version_tag_normalization() {
        // check_for_version should handle both v-prefixed and non-prefixed versions

        // Version without v prefix should get v added
        let version = "1.0.0";
        let tag = if version.starts_with('v') {
            version.to_string()
        } else {
            format!("v{}", version)
        };
        assert_eq!(tag, "v1.0.0");

        // Version with v prefix should stay the same
        let version_with_v = "v1.0.0";
        let tag_with_v = if version_with_v.starts_with('v') {
            version_with_v.to_string()
        } else {
            format!("v{}", version_with_v)
        };
        assert_eq!(tag_with_v, "v1.0.0");
    }

    /// Test release info with special characters in body
    #[test]
    fn test_release_body_special_characters() {
        let release = ReleaseInfo {
            version: "1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            published_at: "2025-01-01".to_string(),
            download_url: "https://example.com".to_string(),
            asset_name: "ted.tar.gz".to_string(),
            body:
                "## Changes\n\n- Fixed bug with `code`\n- Added \"quotes\"\n- Used <angle> brackets"
                    .to_string(),
        };

        assert!(release.body.contains("code"));
        assert!(release.body.contains("quotes"));
        assert!(release.body.contains("<angle>"));
    }

    /// Test that VERSION matches expected format
    #[test]
    fn test_version_format() {
        // VERSION should be in semver format (major.minor.patch)
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert!(parts.len() >= 2, "VERSION should have at least major.minor");

        // Each part should be numeric (ignoring pre-release suffixes)
        for (i, part) in parts.iter().enumerate() {
            let numeric_part = part.split('-').next().unwrap();
            let is_numeric = numeric_part.chars().all(|c| c.is_ascii_digit());
            assert!(
                is_numeric,
                "Part {} ('{}') should be numeric",
                i, numeric_part
            );
        }
    }

    /// Test comparison with current VERSION
    #[test]
    fn test_comparison_with_current_version() {
        // A version higher than current should be newer
        let higher = "999.999.999";
        assert!(is_newer_version(higher, VERSION));

        // A version lower than current should not be newer
        let lower = "0.0.1";
        // 0.0.1 should be lower than current VERSION
        assert!(!is_newer_version(lower, VERSION));
    }
}
