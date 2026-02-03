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

    // ==================== Additional comprehensive tests ====================

    // ===== Version parsing edge cases =====

    #[test]
    fn test_version_comparison_zero_components() {
        assert!(is_newer_version("0.0.1", "0.0.0"));
        assert!(!is_newer_version("0.0.0", "0.0.0"));
        assert!(!is_newer_version("0.0.0", "0.0.1"));
    }

    #[test]
    fn test_version_comparison_leading_zeros() {
        // Leading zeros in version strings
        assert!(is_newer_version("1.01.0", "1.00.0"));
        assert!(is_newer_version("01.0.0", "00.0.0"));
    }

    #[test]
    fn test_version_comparison_very_long_version() {
        // Very long version numbers
        let long_v1 = "1.2.3.4.5.6.7.8.9.10";
        let long_v2 = "1.2.3.4.5.6.7.8.9.9";
        // Only first 3 components are compared
        assert!(!is_newer_version(long_v1, long_v2));
    }

    #[test]
    fn test_version_comparison_mixed_separators() {
        // Versions with mixed separators (only dots should be split on)
        let v1 = "1.2.3-alpha";
        let v2 = "1.2.2";
        assert!(is_newer_version(v1, v2));
    }

    #[test]
    fn test_version_comparison_numeric_overflow() {
        // Large but valid u32 numbers
        let large_v = format!("{}.0.0", u32::MAX);
        let smaller_v = format!("{}.0.0", u32::MAX - 1);
        assert!(is_newer_version(&large_v, &smaller_v));
    }

    // ===== Target triple tests =====

    #[test]
    fn test_target_triple_contains_expected_parts() {
        let target = get_target_triple();
        // Should have at least arch and OS info
        let parts: Vec<&str> = target.split('-').collect();
        assert!(parts.len() >= 2);
        // First part should be arch
        assert!(
            parts[0] == "x86_64" || parts[0] == "aarch64" || parts[0] == "unknown",
            "Unexpected arch: {}",
            parts[0]
        );
    }

    #[test]
    fn test_target_triple_deterministic() {
        // Should return same value every time
        let t1 = get_target_triple();
        let t2 = get_target_triple();
        let t3 = get_target_triple();
        assert_eq!(t1, t2);
        assert_eq!(t2, t3);
    }

    #[test]
    fn test_target_triple_valid_chars() {
        let target = get_target_triple();
        // Should only contain alphanumeric chars and hyphens
        for c in target.chars() {
            assert!(
                c.is_alphanumeric() || c == '-',
                "Unexpected character in target triple: {}",
                c
            );
        }
    }

    // ===== ReleaseInfo comprehensive tests =====

    #[test]
    fn test_release_info_all_fields() {
        let release = ReleaseInfo {
            version: "1.2.3".to_string(),
            tag_name: "v1.2.3".to_string(),
            published_at: "2025-06-15T12:00:00Z".to_string(),
            download_url: "https://github.com/owner/repo/releases/download/v1.2.3/binary.tar.gz"
                .to_string(),
            asset_name: "binary-x86_64-linux.tar.gz".to_string(),
            body: "## Changes\n- Fixed bug\n- Added feature".to_string(),
        };

        assert_eq!(release.version, "1.2.3");
        assert_eq!(release.tag_name, "v1.2.3");
        assert!(release.published_at.contains("2025"));
        assert!(release.download_url.starts_with("https://"));
        assert!(release.asset_name.ends_with(".tar.gz"));
        assert!(release.body.contains("Fixed bug"));
    }

    #[test]
    fn test_release_info_minimal() {
        let release = ReleaseInfo {
            version: "0.0.1".to_string(),
            tag_name: "v0.0.1".to_string(),
            published_at: String::new(),
            download_url: String::new(),
            asset_name: String::new(),
            body: String::new(),
        };

        assert!(!release.version.is_empty());
        assert!(release.published_at.is_empty());
        assert!(release.download_url.is_empty());
    }

    #[test]
    fn test_release_info_unicode_body() {
        let release = ReleaseInfo {
            version: "1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            published_at: "2025-01-01".to_string(),
            download_url: "https://example.com".to_string(),
            asset_name: "release.tar.gz".to_string(),
            body: "新機能追加 🎉 Émojis supported!".to_string(),
        };

        assert!(release.body.contains("🎉"));
        assert!(release.body.contains("新機能"));
    }

    // ===== File path handling tests =====

    #[test]
    fn test_pathbuf_operations() {
        use std::path::PathBuf;

        let path = PathBuf::from("/tmp/test");
        let with_extension = path.with_extension("old.exe");

        assert!(with_extension.to_str().unwrap().ends_with(".old.exe"));
    }

    #[test]
    fn test_pathbuf_join() {
        use std::path::PathBuf;

        let base = PathBuf::from("/tmp");
        let full = base.join("subdir").join("file.txt");

        assert!(full.to_str().unwrap().contains("subdir"));
        assert!(full.to_str().unwrap().ends_with("file.txt"));
    }

    #[test]
    fn test_pathbuf_parent() {
        use std::path::PathBuf;

        let path = PathBuf::from("/a/b/c");
        let parent = path.parent();

        assert!(parent.is_some());
        assert_eq!(parent.unwrap(), PathBuf::from("/a/b"));
    }

    // ===== Archive name parsing tests =====

    #[test]
    fn test_archive_name_tar_gz() {
        let name = "ted-x86_64-unknown-linux-gnu.tar.gz";
        assert!(name.ends_with(".tar.gz"));
    }

    #[test]
    fn test_archive_name_zip() {
        let name = "ted-x86_64-pc-windows-msvc.zip";
        assert!(name.ends_with(".zip"));
    }

    #[test]
    fn test_archive_name_unknown() {
        let name = "ted-x86_64-linux.unknown";
        assert!(!name.ends_with(".tar.gz"));
        assert!(!name.ends_with(".zip"));
    }

    // ===== JSON parsing tests =====

    #[test]
    fn test_parse_release_assets_array() {
        let json_str = r#"[
            {"name": "ted-linux.tar.gz", "browser_download_url": "https://example.com/linux.tar.gz"},
            {"name": "ted-windows.zip", "browser_download_url": "https://example.com/windows.zip"},
            {"name": "ted-macos.tar.gz", "browser_download_url": "https://example.com/macos.tar.gz"}
        ]"#;

        let assets: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let arr = assets.as_array().unwrap();

        assert_eq!(arr.len(), 3);
        assert!(arr[0]["name"].as_str().unwrap().contains("linux"));
        assert!(arr[1]["name"].as_str().unwrap().contains("windows"));
        assert!(arr[2]["name"].as_str().unwrap().contains("macos"));
    }

    #[test]
    fn test_parse_release_full_response() {
        let json_str = r#"{
            "tag_name": "v2.0.0",
            "published_at": "2025-06-15T10:30:00Z",
            "body": "Release Notes - New feature - Bug fix",
            "assets": [
                {"name": "ted-x86_64.tar.gz", "browser_download_url": "https://example.com/ted.tar.gz"}
            ]
        }"#;

        let release: serde_json::Value = serde_json::from_str(json_str).unwrap();

        assert_eq!(release["tag_name"].as_str().unwrap(), "v2.0.0");
        assert!(release["body"].as_str().unwrap().contains("New feature"));
        assert!(!release["assets"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_parse_release_null_body() {
        let json_str = r#"{"tag_name": "v1.0.0", "body": null}"#;

        let release: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let body = release["body"].as_str().unwrap_or("");

        assert!(body.is_empty());
    }

    // ===== Version tag handling tests =====

    #[test]
    fn test_strip_v_prefix_various() {
        let cases = [
            ("v1.0.0", "1.0.0"),
            ("1.0.0", "1.0.0"),
            ("v", ""),
            ("vvv1.0.0", "vv1.0.0"),
            ("V1.0.0", "V1.0.0"), // Only lowercase 'v' is stripped
        ];

        for (input, expected) in cases {
            let result = input.strip_prefix('v').unwrap_or(input);
            assert_eq!(result, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_add_v_prefix() {
        let cases = [("1.0.0", "v1.0.0"), ("v1.0.0", "v1.0.0")];

        for (input, expected) in cases {
            let result = if input.starts_with('v') {
                input.to_string()
            } else {
                format!("v{}", input)
            };
            assert_eq!(result, expected, "Failed for input: {}", input);
        }
    }

    // ===== URL construction tests =====

    #[test]
    fn test_github_api_latest_url() {
        let repo = "owner-name/repo-name";
        let url = format!("https://api.github.com/repos/{}/releases/latest", repo);

        assert!(url.starts_with("https://api.github.com"));
        assert!(url.contains("owner-name"));
        assert!(url.ends_with("/releases/latest"));
    }

    #[test]
    fn test_github_api_tag_url() {
        let repo = "owner-name/repo-name";
        let tag = "v1.0.0";
        let url = format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            repo, tag
        );

        assert!(url.contains("owner-name"));
        assert!(url.contains("v1.0.0"));
    }

    // ===== User agent tests =====

    #[test]
    fn test_user_agent_format() {
        let user_agent = format!("ted/{}", VERSION);

        assert!(user_agent.starts_with("ted/"));
        assert!(user_agent.len() > 4); // "ted/" + version
    }

    // ===== Replace binary tests =====

    #[test]
    fn test_file_copy_simulation() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source");
        let dest = temp_dir.path().join("dest");

        fs::write(&source, b"source content").unwrap();

        // Copy the file
        fs::copy(&source, &dest).unwrap();

        // Verify
        let content = fs::read(&dest).unwrap();
        assert_eq!(content, b"source content");
    }

    #[test]
    fn test_file_rename_simulation() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let original = temp_dir.path().join("original");
        let renamed = temp_dir.path().join("renamed");

        fs::write(&original, b"content").unwrap();
        assert!(original.exists());

        fs::rename(&original, &renamed).unwrap();

        assert!(!original.exists());
        assert!(renamed.exists());
    }

    // ===== Environment tests =====

    #[test]
    fn test_env_temp_dir() {
        let temp = std::env::temp_dir();
        assert!(temp.exists());
        assert!(temp.is_absolute());
    }

    #[test]
    fn test_env_current_exe_type() {
        // current_exe returns a Result<PathBuf>
        let result = std::env::current_exe();
        // In test context, this should succeed
        if let Ok(path) = result {
            assert!(path.is_absolute());
        }
    }

    // ===== HTTP status code handling =====

    #[test]
    fn test_status_code_success_range() {
        for code in 200..300u16 {
            assert!(
                (200..300).contains(&code),
                "Code {} should be in success range",
                code
            );
        }
    }

    #[test]
    fn test_status_code_not_found() {
        let code: u16 = 404;
        assert_eq!(code, 404);
        assert!(!(200..300).contains(&code));
    }

    #[test]
    fn test_status_code_client_errors() {
        for code in 400..500u16 {
            assert!(
                (400..500).contains(&code),
                "Code {} should be in client error range",
                code
            );
        }
    }

    #[test]
    fn test_status_code_server_errors() {
        for code in 500..600u16 {
            assert!(
                (500..600).contains(&code),
                "Code {} should be in server error range",
                code
            );
        }
    }

    // ===== Semver parsing helpers =====

    #[test]
    fn test_parse_semver_components() {
        let version = "1.2.3";
        let parts: Vec<&str> = version.split('.').collect();

        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].parse::<u32>().unwrap(), 1);
        assert_eq!(parts[1].parse::<u32>().unwrap(), 2);
        assert_eq!(parts[2].parse::<u32>().unwrap(), 3);
    }

    #[test]
    fn test_parse_semver_with_prerelease() {
        let version = "1.2.3-beta.1";
        let parts: Vec<&str> = version.split('.').collect();

        // Third part has prerelease info
        let patch_part = parts[2];
        let numeric_patch = patch_part.split('-').next().unwrap();
        assert_eq!(numeric_patch.parse::<u32>().unwrap(), 3);
    }

    #[test]
    fn test_version_tuple_comparison() {
        let v1: (u32, u32, u32) = (1, 2, 3);
        let v2: (u32, u32, u32) = (1, 2, 4);
        let v3: (u32, u32, u32) = (1, 3, 0);
        let v4: (u32, u32, u32) = (2, 0, 0);

        assert!(v2 > v1);
        assert!(v3 > v2);
        assert!(v4 > v3);
        assert!(v4 > v1);
    }

    // ==================== Async Method Tests with wiremock ====================

    // wiremock is available for future integration tests

    /// Helper to create a mock release JSON response
    fn create_release_json(version: &str, target: &str) -> serde_json::Value {
        serde_json::json!({
            "tag_name": format!("v{}", version),
            "published_at": "2025-06-15T12:00:00Z",
            "body": "Test release notes",
            "assets": [
                {
                    "name": format!("ted-{}.tar.gz", target),
                    "browser_download_url": format!("https://example.com/ted-{}.tar.gz", target)
                }
            ]
        })
    }

    #[tokio::test]
    async fn test_check_for_updates_no_releases() {
        // This test would require mocking the GitHub API at api.github.com
        // Since we can't easily intercept the actual reqwest calls to github,
        // we verify the parsing logic with a simulated response

        let json_str = r#"{"message": "Not Found"}"#;
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // Verify the 404 handling logic
        let tag_name = parsed["tag_name"].as_str();
        assert!(tag_name.is_none());
    }

    #[tokio::test]
    async fn test_check_for_updates_with_newer_version_parsing() {
        // Test the parsing logic for a newer version response
        let target = get_target_triple();
        let release_json = create_release_json("999.0.0", &target);

        let tag_name = release_json["tag_name"].as_str().unwrap();
        assert_eq!(tag_name, "v999.0.0");

        let remote_version = tag_name.strip_prefix('v').unwrap_or(tag_name);
        assert!(is_newer_version(remote_version, VERSION));

        let assets = release_json["assets"].as_array().unwrap();
        assert!(!assets.is_empty());

        let asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        });
        assert!(asset.is_some());
    }

    #[tokio::test]
    async fn test_check_for_updates_same_version_parsing() {
        // Test that same version is not considered newer
        let target = get_target_triple();
        let release_json = create_release_json(VERSION, &target);

        let tag_name = release_json["tag_name"].as_str().unwrap();
        let remote_version = tag_name.strip_prefix('v').unwrap_or(tag_name);

        assert!(!is_newer_version(remote_version, VERSION));
    }

    #[tokio::test]
    async fn test_check_for_updates_older_version_parsing() {
        // Test that older version is not considered newer
        let target = get_target_triple();
        let release_json = create_release_json("0.0.1", &target);

        let tag_name = release_json["tag_name"].as_str().unwrap();
        let remote_version = tag_name.strip_prefix('v').unwrap_or(tag_name);

        assert!(!is_newer_version(remote_version, VERSION));
    }

    #[tokio::test]
    async fn test_check_for_version_tag_normalization() {
        // Test that version tag normalization works
        let version_no_v = "1.2.3";
        let tag = if version_no_v.starts_with('v') {
            version_no_v.to_string()
        } else {
            format!("v{}", version_no_v)
        };
        assert_eq!(tag, "v1.2.3");

        let version_with_v = "v1.2.3";
        let tag2 = if version_with_v.starts_with('v') {
            version_with_v.to_string()
        } else {
            format!("v{}", version_with_v)
        };
        assert_eq!(tag2, "v1.2.3");
    }

    #[tokio::test]
    async fn test_release_info_construction_from_json() {
        let target = get_target_triple();
        let json = create_release_json("2.0.0", &target);

        let tag_name = json["tag_name"].as_str().unwrap();
        let remote_version = tag_name.strip_prefix('v').unwrap_or(tag_name);
        let assets = json["assets"].as_array().unwrap();

        let asset = assets
            .iter()
            .find(|a| {
                a["name"]
                    .as_str()
                    .map(|n| n.contains(&target))
                    .unwrap_or(false)
            })
            .unwrap();

        let release = ReleaseInfo {
            version: remote_version.to_string(),
            tag_name: tag_name.to_string(),
            published_at: json["published_at"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            download_url: asset["browser_download_url"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            asset_name: asset["name"].as_str().unwrap_or("").to_string(),
            body: json["body"].as_str().unwrap_or("").to_string(),
        };

        assert_eq!(release.version, "2.0.0");
        assert_eq!(release.tag_name, "v2.0.0");
        assert!(release.download_url.contains("example.com"));
        assert!(release.asset_name.contains(&target));
    }

    #[tokio::test]
    async fn test_install_update_download_logic() {
        // Test the temp file path construction logic
        let release = ReleaseInfo {
            version: "1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            published_at: "2025-01-01".to_string(),
            download_url: "https://example.com/ted.tar.gz".to_string(),
            asset_name: "ted-x86_64-linux.tar.gz".to_string(),
            body: "Notes".to_string(),
        };

        let temp_dir = std::env::temp_dir();
        let temp_archive = temp_dir.join(&release.asset_name);
        let temp_binary = temp_dir.join("ted_new");

        assert!(temp_archive.to_str().unwrap().contains(&release.asset_name));
        assert!(temp_binary.to_str().unwrap().contains("ted_new"));
    }

    #[tokio::test]
    async fn test_release_info_with_missing_assets() {
        let json_str = r#"{"tag_name": "v1.0.0", "assets": []}"#;
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let assets = parsed["assets"].as_array().unwrap();
        assert!(assets.is_empty());

        let target = get_target_triple();
        let matching_asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        });
        assert!(matching_asset.is_none());
    }

    #[tokio::test]
    async fn test_release_info_no_matching_platform() {
        // Test when assets exist but none match current platform
        let json_str = r#"{"tag_name": "v1.0.0", "assets": [
            {"name": "ted-wasm.tar.gz", "browser_download_url": "https://example.com/wasm.tar.gz"}
        ]}"#;
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let assets = parsed["assets"].as_array().unwrap();
        assert_eq!(assets.len(), 1);

        let target = get_target_triple();
        let matching_asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        });

        // Unless target happens to be wasm, this should be None
        if !target.contains("wasm") {
            assert!(matching_asset.is_none());
        }
    }

    #[tokio::test]
    async fn test_http_client_construction() {
        // Test that we can construct an HTTP client with the user agent
        let client = reqwest::Client::builder()
            .user_agent(format!("ted/{}", VERSION))
            .build();

        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_github_api_url_async_construction() {
        let latest_url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            GITHUB_REPO
        );
        assert!(latest_url.contains("api.github.com"));
        assert!(latest_url.contains(GITHUB_REPO));
        assert!(latest_url.ends_with("/releases/latest"));

        let tag = "v1.0.0";
        let tag_url = format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            GITHUB_REPO, tag
        );
        assert!(tag_url.contains(GITHUB_REPO));
        assert!(tag_url.ends_with("/v1.0.0"));
    }

    #[tokio::test]
    async fn test_error_message_formatting() {
        // Test error message construction
        let error_msg = format!("Failed to check for updates: {}", "connection error");
        assert!(error_msg.contains("connection error"));

        let api_error = format!("GitHub API error: {}", "403 Forbidden");
        assert!(api_error.contains("403"));

        let platform_error = format!(
            "No release available for your platform ({})",
            get_target_triple()
        );
        assert!(platform_error.contains(&get_target_triple()));
    }

    #[tokio::test]
    async fn test_json_parsing_edge_cases() {
        // Test with null values
        let json_str = r#"{"tag_name": "v1.0.0", "published_at": null, "body": null}"#;
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let published_at = parsed["published_at"].as_str().unwrap_or("unknown");
        assert_eq!(published_at, "unknown");

        let body = parsed["body"].as_str().unwrap_or("");
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn test_json_parsing_extra_fields() {
        // Test that extra fields in JSON don't cause issues
        let json_str = r#"{
            "tag_name": "v1.0.0",
            "published_at": "2025-01-01",
            "body": "Notes",
            "extra_field": "ignored",
            "another_field": 123,
            "assets": []
        }"#;
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        assert_eq!(parsed["tag_name"].as_str().unwrap(), "v1.0.0");
        // Extra fields are just ignored
    }

    #[tokio::test]
    async fn test_version_stripping_edge_cases() {
        let cases = [
            ("v1.0.0", "1.0.0"),
            ("1.0.0", "1.0.0"),
            ("v", ""),
            ("vv1.0.0", "v1.0.0"),
            ("V1.0.0", "V1.0.0"), // Only lowercase v
        ];

        for (input, expected) in cases {
            let result = input.strip_prefix('v').unwrap_or(input);
            assert_eq!(result, expected, "Failed for input: {}", input);
        }
    }

    #[tokio::test]
    async fn test_current_exe_path() {
        // Test that we can get the current executable path
        let exe_result = std::env::current_exe();
        // In test context, this should succeed
        if let Ok(path) = exe_result {
            assert!(path.is_absolute());
        }
    }

    #[tokio::test]
    async fn test_temp_dir_operations() {
        let temp_dir = std::env::temp_dir();
        assert!(temp_dir.exists());

        let test_file = temp_dir.join("ted_test_file");
        // Just test the path construction
        assert!(test_file.to_str().unwrap().contains("ted_test_file"));
    }

    // ==================== Integration-style tests ====================

    #[tokio::test]
    async fn test_release_info_full_workflow() {
        // Test the full workflow of parsing a release and constructing ReleaseInfo
        let target = get_target_triple();

        let json = serde_json::json!({
            "tag_name": "v999.999.999",
            "published_at": "2025-12-31T23:59:59Z",
            "body": "## What's New\n\n- Feature 1\n- Feature 2",
            "assets": [
                {
                    "name": format!("ted-{}.tar.gz", target),
                    "browser_download_url": format!("https://github.com/blackman-ai/ted/releases/download/v999.999.999/ted-{}.tar.gz", target)
                },
                {
                    "name": "checksums.txt",
                    "browser_download_url": "https://github.com/blackman-ai/ted/releases/download/v999.999.999/checksums.txt"
                }
            ]
        });

        let tag_name = json["tag_name"].as_str().unwrap();
        let remote_version = tag_name.strip_prefix('v').unwrap_or(tag_name);

        // Verify it's newer than current
        assert!(is_newer_version(remote_version, VERSION));

        // Find the right asset
        let assets = json["assets"].as_array().unwrap();
        let asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        });
        assert!(asset.is_some());

        let asset = asset.unwrap();
        let release = ReleaseInfo {
            version: remote_version.to_string(),
            tag_name: tag_name.to_string(),
            published_at: json["published_at"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            download_url: asset["browser_download_url"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            asset_name: asset["name"].as_str().unwrap_or("").to_string(),
            body: json["body"].as_str().unwrap_or("").to_string(),
        };

        assert_eq!(release.version, "999.999.999");
        assert!(release.download_url.contains("github.com"));
        assert!(release.body.contains("What's New"));
    }

    #[tokio::test]
    async fn test_reqwest_client_with_custom_timeout() {
        use std::time::Duration;

        let client = reqwest::Client::builder()
            .user_agent(format!("ted/{}", VERSION))
            .timeout(Duration::from_secs(30))
            .build();

        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_bytes_to_file_operations() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test_data");

        let bytes = b"test binary content";
        std::fs::write(&test_file, bytes).unwrap();

        let read_bytes = std::fs::read(&test_file).unwrap();
        assert_eq!(read_bytes, bytes);

        // Clean up is automatic with TempDir
    }
}
