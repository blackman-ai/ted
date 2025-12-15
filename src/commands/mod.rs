// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Custom commands system
//!
//! Discovers and executes user-defined scripts from .ted/commands/ directories.
//!
//! Search paths (in priority order):
//! 1. `./.ted/commands/` - Project-local commands
//! 2. `~/.ted/commands/` - User-global commands

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Settings;
use crate::error::Result;

/// Information about a custom command
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// Name of the command (filename without extension)
    pub name: String,
    /// Full path to the script
    pub path: PathBuf,
    /// Whether it's project-local or user-global
    pub is_local: bool,
}

/// Discover custom commands from filesystem
pub fn discover_commands() -> Result<HashMap<String, CustomCommand>> {
    let mut commands = HashMap::new();

    // User-global commands (lowest priority)
    let user_commands_dir = Settings::commands_dir();
    if user_commands_dir.exists() {
        for cmd in list_commands_in_dir(&user_commands_dir, false)? {
            commands.insert(cmd.name.clone(), cmd);
        }
    }

    // Project-local commands (highest priority, overrides user-global)
    if let Ok(cwd) = std::env::current_dir() {
        let project_commands_dir = cwd.join(".ted").join("commands");
        if project_commands_dir.exists() {
            for cmd in list_commands_in_dir(&project_commands_dir, true)? {
                commands.insert(cmd.name.clone(), cmd);
            }
        }
    }

    Ok(commands)
}

/// List commands in a directory
fn list_commands_in_dir(dir: &PathBuf, is_local: bool) -> Result<Vec<CustomCommand>> {
    let mut commands = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Check if file is executable
        #[cfg(unix)]
        let is_executable = {
            use std::os::unix::fs::PermissionsExt;
            entry.metadata()?.permissions().mode() & 0o111 != 0
        };

        #[cfg(not(unix))]
        let is_executable = {
            // On Windows, check for common script extensions
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| {
                    matches!(
                        e.to_lowercase().as_str(),
                        "bat" | "cmd" | "ps1" | "py" | "js"
                    )
                })
                .unwrap_or(false)
        };

        if !is_executable {
            continue;
        }

        // Get command name (filename without extension)
        if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
            commands.push(CustomCommand {
                name: name.to_string(),
                path,
                is_local,
            });
        }
    }

    Ok(commands)
}

/// Execute a custom command
pub fn execute_command(cmd: &CustomCommand, args: &[String]) -> Result<i32> {
    let mut process = Command::new(&cmd.path);

    // Add remaining arguments
    process.args(args);

    // Set environment variables
    if let Ok(cwd) = std::env::current_dir() {
        process.env("TED_WORKING_DIR", &cwd);

        // Try to find project root
        if let Some(root) = find_project_root(&cwd) {
            process.env("TED_PROJECT_ROOT", &root);
        }
    }

    // Execute
    let status = process.status()?;

    Ok(status.code().unwrap_or(1))
}

/// Find project root from a starting directory
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let manifest_files = [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "pom.xml",
        "build.gradle",
        ".git",
    ];

    let mut current = start;
    loop {
        for manifest in &manifest_files {
            if current.join(manifest).exists() {
                return Some(current.to_path_buf());
            }
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// List all available custom command names
pub fn list_command_names() -> Result<Vec<String>> {
    let commands = discover_commands()?;
    let mut names: Vec<_> = commands.keys().cloned().collect();
    names.sort();
    Ok(names)
}

/// Check if a command name matches a custom command
pub fn get_command(name: &str) -> Result<Option<CustomCommand>> {
    let commands = discover_commands()?;
    Ok(commands.get(name).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_command_discovery() {
        // Just ensure it doesn't panic
        let _ = discover_commands();
    }

    #[test]
    fn test_custom_command_struct() {
        let cmd = CustomCommand {
            name: "test-cmd".to_string(),
            path: PathBuf::from("/path/to/script.sh"),
            is_local: true,
        };

        assert_eq!(cmd.name, "test-cmd");
        assert_eq!(cmd.path, PathBuf::from("/path/to/script.sh"));
        assert!(cmd.is_local);
    }

    #[test]
    fn test_find_project_root_with_cargo() {
        // Create a temp directory with Cargo.toml
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        let result = find_project_root(temp_dir.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_find_project_root_with_package_json() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        let result = find_project_root(temp_dir.path());
        assert!(result.is_some());
    }

    #[test]
    fn test_find_project_root_with_pyproject() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("pyproject.toml"), "[tool.poetry]").unwrap();

        let result = find_project_root(temp_dir.path());
        assert!(result.is_some());
    }

    #[test]
    fn test_find_project_root_with_go_mod() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("go.mod"), "module example.com/test").unwrap();

        let result = find_project_root(temp_dir.path());
        assert!(result.is_some());
    }

    #[test]
    fn test_find_project_root_with_git() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let result = find_project_root(temp_dir.path());
        assert!(result.is_some());
    }

    #[test]
    fn test_find_project_root_none() {
        let temp_dir = TempDir::new().unwrap();
        // Empty directory, no manifest files
        let result = find_project_root(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_find_project_root_nested() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        // Create a nested directory
        let nested = temp_dir.path().join("src").join("utils");
        std::fs::create_dir_all(&nested).unwrap();

        // Finding root from nested should return the parent with Cargo.toml
        let result = find_project_root(&nested);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_list_command_names() {
        // Should not panic
        let _ = list_command_names();
    }

    #[test]
    fn test_get_command_nonexistent() {
        let result = get_command("nonexistent-command-xyz-123");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    #[cfg(unix)]
    fn test_list_commands_in_dir() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("test-script.sh");
        std::fs::write(&script_path, "#!/bin/bash\necho hello").unwrap();

        // Make it executable
        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();

        let commands = list_commands_in_dir(&temp_dir.path().to_path_buf(), true).unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "test-script");
        assert!(commands[0].is_local);
    }

    #[test]
    fn test_list_commands_in_dir_skips_directories() {
        let temp_dir = TempDir::new().unwrap();

        // Create a subdirectory (should be skipped)
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let commands = list_commands_in_dir(&temp_dir.path().to_path_buf(), false).unwrap();

        // Directory should be skipped
        assert!(commands.is_empty() || !commands.iter().any(|c| c.name == "subdir"));
    }

    #[test]
    fn test_custom_command_clone() {
        let cmd = CustomCommand {
            name: "test".to_string(),
            path: PathBuf::from("/path/to/test"),
            is_local: false,
        };

        let cloned = cmd.clone();
        assert_eq!(cloned.name, cmd.name);
        assert_eq!(cloned.path, cmd.path);
        assert_eq!(cloned.is_local, cmd.is_local);
    }

    #[test]
    fn test_custom_command_debug() {
        let cmd = CustomCommand {
            name: "debug-test".to_string(),
            path: PathBuf::from("/tmp/script.sh"),
            is_local: true,
        };

        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains("script.sh"));
    }

    #[test]
    fn test_find_project_root_with_pom() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("pom.xml"), "<project></project>").unwrap();

        let result = find_project_root(temp_dir.path());
        assert!(result.is_some());
    }

    #[test]
    fn test_find_project_root_with_gradle() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("build.gradle"), "plugins {}").unwrap();

        let result = find_project_root(temp_dir.path());
        assert!(result.is_some());
    }

    #[test]
    #[cfg(unix)]
    fn test_list_commands_in_dir_skips_non_executable() {
        let temp_dir = TempDir::new().unwrap();

        // Create a non-executable file
        let script_path = temp_dir.path().join("non-exec.sh");
        std::fs::write(&script_path, "#!/bin/bash\necho hello").unwrap();

        // Don't make it executable - leave default permissions

        let commands = list_commands_in_dir(&temp_dir.path().to_path_buf(), false).unwrap();

        // Non-executable file should be skipped
        assert!(commands.is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn test_list_commands_multiple_files() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();

        // Create multiple executable scripts
        for name in &["script1.sh", "script2.py", "script3.js"] {
            let script_path = temp_dir.path().join(name);
            std::fs::write(&script_path, "#!/bin/bash\necho hello").unwrap();

            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        let commands = list_commands_in_dir(&temp_dir.path().to_path_buf(), true).unwrap();

        assert_eq!(commands.len(), 3);

        let names: Vec<_> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"script1"));
        assert!(names.contains(&"script2"));
        assert!(names.contains(&"script3"));
    }

    #[test]
    fn test_list_commands_in_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let commands = list_commands_in_dir(&temp_dir.path().to_path_buf(), true).unwrap();
        assert!(commands.is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_command_with_true() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("true-script.sh");
        std::fs::write(&script_path, "#!/bin/bash\nexit 0").unwrap();

        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();

        let cmd = CustomCommand {
            name: "true-script".to_string(),
            path: script_path,
            is_local: true,
        };

        let result = execute_command(&cmd, &[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_command_with_false() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("false-script.sh");
        std::fs::write(&script_path, "#!/bin/bash\nexit 1").unwrap();

        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();

        let cmd = CustomCommand {
            name: "false-script".to_string(),
            path: script_path,
            is_local: true,
        };

        let result = execute_command(&cmd, &[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_command_with_args() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("echo-script.sh");
        std::fs::write(&script_path, "#!/bin/bash\necho $@").unwrap();

        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();

        let cmd = CustomCommand {
            name: "echo-script".to_string(),
            path: script_path,
            is_local: true,
        };

        let args = vec!["arg1".to_string(), "arg2".to_string()];
        let result = execute_command(&cmd, &args);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_discover_commands_returns_hashmap() {
        let commands = discover_commands();
        assert!(commands.is_ok());
    }
}
