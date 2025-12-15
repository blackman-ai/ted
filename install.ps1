# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.
#
# Ted installer script for Windows
# Usage:
#   Install:   irm https://raw.githubusercontent.com/blackman-ai/ted/master/install.ps1 | iex
#   Uninstall: & ([scriptblock]::Create((irm https://raw.githubusercontent.com/blackman-ai/ted/master/install.ps1))) -Uninstall
#
# Environment variables:
#   TED_INSTALL_DIR - Installation directory (default: %LOCALAPPDATA%\Programs\ted)
#   TED_VERSION     - Specific version to install (default: latest)

param(
    [switch]$Uninstall,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'

$Repo = "blackman-ai/ted"
$BinaryName = "ted.exe"

function Write-Info {
    param([string]$Message)
    Write-Host "==> " -ForegroundColor Blue -NoNewline
    Write-Host $Message
}

function Write-Success {
    param([string]$Message)
    Write-Host "==> " -ForegroundColor Green -NoNewline
    Write-Host $Message
}

function Write-Warn {
    param([string]$Message)
    Write-Host "Warning: " -ForegroundColor Yellow -NoNewline
    Write-Host $Message
}

function Write-Error {
    param([string]$Message)
    Write-Host "Error: " -ForegroundColor Red -NoNewline
    Write-Host $Message
    exit 1
}

function Get-Architecture {
    $arch = [System.Environment]::GetEnvironmentVariable("PROCESSOR_ARCHITECTURE")
    switch ($arch) {
        "AMD64" { return "x86_64" }
        "ARM64" { return "aarch64" }
        default { Write-Error "Unsupported architecture: $arch" }
    }
}

function Get-LatestVersion {
    $url = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $response = Invoke-RestMethod -Uri $url -UseBasicParsing
        return $response.tag_name -replace '^v', ''
    }
    catch {
        Write-Error "Failed to fetch latest version: $_"
    }
}

function Get-InstallDir {
    if ($env:TED_INSTALL_DIR) {
        return $env:TED_INSTALL_DIR
    }
    return Join-Path $env:LOCALAPPDATA "Programs\ted"
}

function Test-InPath {
    param([string]$Dir)
    $paths = $env:PATH -split ';'
    return $paths -contains $Dir
}

function Add-ToPath {
    param([string]$Dir)

    $currentPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
    if ($currentPath -notlike "*$Dir*") {
        $newPath = "$Dir;$currentPath"
        [System.Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
        $env:PATH = "$Dir;$env:PATH"
        return $true
    }
    return $false
}

function Find-Ted {
    # Check TED_INSTALL_DIR
    if ($env:TED_INSTALL_DIR) {
        $path = Join-Path $env:TED_INSTALL_DIR $BinaryName
        if (Test-Path $path) { return $path }
    }

    # Check default location
    $defaultPath = Join-Path (Join-Path $env:LOCALAPPDATA "Programs\ted") $BinaryName
    if (Test-Path $defaultPath) { return $defaultPath }

    # Check PATH
    $cmd = Get-Command ted -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    return $null
}

function Invoke-Uninstall {
    Write-Info "Uninstalling Ted..."
    Write-Host ""

    $tedPath = Find-Ted

    if (-not $tedPath) {
        Write-Error "Ted is not installed or could not be found."
        return
    }

    Write-Info "Found ted at: $tedPath"

    # Get version before removing
    try {
        $version = & $tedPath --version 2>$null | Select-Object -First 1 | ForEach-Object { $_.Split(' ')[1] }
    }
    catch {
        $version = "unknown"
    }

    # Remove binary
    try {
        Remove-Item -Path $tedPath -Force
        Write-Success "Removed $tedPath"
    }
    catch {
        Write-Error "Failed to remove $tedPath. Error: $_"
        return
    }

    # Check if directory is empty and remove it
    $installDir = Split-Path $tedPath
    if ((Get-ChildItem $installDir -ErrorAction SilentlyContinue | Measure-Object).Count -eq 0) {
        Remove-Item -Path $installDir -Force -ErrorAction SilentlyContinue
    }

    # Note about config directory
    $configDir = if ($env:TED_HOME) { $env:TED_HOME } else { Join-Path $env:USERPROFILE ".ted" }
    if (Test-Path $configDir) {
        Write-Host ""
        Write-Warn "Configuration directory exists at: $configDir"
        Write-Host "This contains your settings, session history, and custom caps."
        Write-Host "To remove it, run: Remove-Item -Recurse -Force '$configDir'"
    }

    Write-Host ""
    Write-Success "Ted v$version uninstalled successfully!"
}

function Show-Help {
    Write-Host "Ted Installer for Windows"
    Write-Host ""
    Write-Host "Usage:"
    Write-Host "  Install:   irm .../install.ps1 | iex"
    Write-Host "  Uninstall: & ([scriptblock]::Create((irm .../install.ps1))) -Uninstall"
    Write-Host ""
    Write-Host "Parameters:"
    Write-Host "  -Uninstall    Uninstall ted"
    Write-Host "  -Help         Show this help"
    Write-Host ""
    Write-Host "Environment variables:"
    Write-Host "  TED_INSTALL_DIR    Installation directory"
    Write-Host "  TED_VERSION        Specific version to install"
}

function Main {
    Write-Info "Installing Ted - AI coding assistant for your terminal"
    Write-Host ""

    # Detect platform
    $arch = Get-Architecture
    $target = "$arch-pc-windows-msvc"

    Write-Info "Detected platform: $target"

    # Get version
    $version = $env:TED_VERSION
    if (-not $version) {
        Write-Info "Fetching latest version..."
        $version = Get-LatestVersion
        if (-not $version) {
            Write-Error "Failed to fetch latest version"
        }
    }
    $version = $version -replace '^v', ''
    Write-Info "Installing version: $version"

    # Download URL
    $filename = "ted-$target.zip"
    $url = "https://github.com/$Repo/releases/download/v$version/$filename"

    # Create temp directory
    $tmpdir = Join-Path $env:TEMP ([System.IO.Path]::GetRandomFileName())
    New-Item -ItemType Directory -Path $tmpdir -Force | Out-Null

    try {
        # Download
        Write-Info "Downloading $filename..."
        $zipPath = Join-Path $tmpdir $filename
        Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing

        # Extract
        Write-Info "Extracting..."
        Expand-Archive -Path $zipPath -DestinationPath $tmpdir -Force

        # Install
        $installDir = Get-InstallDir
        Write-Info "Installing to $installDir..."

        # Create install directory if it doesn't exist
        if (-not (Test-Path $installDir)) {
            New-Item -ItemType Directory -Path $installDir -Force | Out-Null
        }

        $binaryPath = Join-Path $installDir $BinaryName

        # Check if ted already exists
        if (Test-Path $binaryPath) {
            try {
                $existingVersion = & $binaryPath --version 2>$null | Select-Object -First 1 | ForEach-Object { $_.Split(' ')[1] }
                Write-Warn "Replacing existing installation (version: $existingVersion)"
            }
            catch {
                Write-Warn "Replacing existing installation"
            }
        }

        # Copy binary
        Copy-Item -Path (Join-Path $tmpdir "ted.exe") -Destination $binaryPath -Force

        Write-Host ""
        Write-Success "Ted v$version installed successfully!"
        Write-Host ""

        # Check and update PATH
        if (-not (Test-InPath $installDir)) {
            Write-Info "Adding $installDir to PATH..."
            if (Add-ToPath $installDir) {
                Write-Success "Added to user PATH. Restart your terminal to use 'ted' command."
            }
        }

        # Quick start guide
        Write-Host ""
        Write-Host "Quick start:"
        Write-Host "  1. Set your API key:  `$env:ANTHROPIC_API_KEY = 'your-key'"
        Write-Host "  2. Start chatting:    ted"
        Write-Host ""
        Write-Host "To uninstall: & ([scriptblock]::Create((irm https://raw.githubusercontent.com/$Repo/master/install.ps1))) -Uninstall"
        Write-Host "For more info, visit: https://github.com/$Repo"
    }
    finally {
        # Cleanup
        Remove-Item -Path $tmpdir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Entry point
if ($Help) {
    Show-Help
}
elseif ($Uninstall) {
    Invoke-Uninstall
}
else {
    Main
}
