# Nodalync Install Script for Windows
# Usage: irm https://raw.githubusercontent.com/gdgiangi/nodalync-protocol/main/install.ps1 | iex
#
# This script downloads and installs the latest nodalync binary for Windows.

$ErrorActionPreference = "Stop"

$REPO = "gdgiangi/nodalync-protocol"
$BINARY_NAME = "nodalync.exe"
$INSTALL_DIR = "$env:LOCALAPPDATA\nodalync"

function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Blue
}

function Write-Success {
    param([string]$Message)
    Write-Host "[OK] $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Error {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
    exit 1
}

function Get-LatestVersion {
    Write-Info "Fetching latest release..."
    
    try {
        $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$REPO/releases" -UseBasicParsing
        # Find the first v* tag (CLI release, not protocol-v* tags)
        $latestRelease = $releases | Where-Object { $_.tag_name -match "^v\d" } | Select-Object -First 1
        
        if (-not $latestRelease) {
            Write-Error "Could not find a valid release"
        }
        
        $version = $latestRelease.tag_name
        Write-Info "Latest version: $version"
        return $version
    }
    catch {
        Write-Error "Failed to fetch releases: $_"
    }
}

function Install-Nodalync {
    param([string]$Version)
    
    $platform = "x86_64-pc-windows-msvc"
    $downloadUrl = "https://github.com/$REPO/releases/download/$Version/nodalync-$platform.zip"
    
    Write-Info "Downloading from: $downloadUrl"
    
    # Create install directory if it doesn't exist
    if (-not (Test-Path $INSTALL_DIR)) {
        New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null
    }
    
    # Create temp directory
    $tempDir = Join-Path $env:TEMP "nodalync-install-$(Get-Random)"
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
    
    try {
        # Download
        $zipPath = Join-Path $tempDir "nodalync.zip"
        Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -UseBasicParsing
        
        # Extract
        Expand-Archive -Path $zipPath -DestinationPath $tempDir -Force
        
        # Install
        $binaryPath = Join-Path $tempDir $BINARY_NAME
        if (-not (Test-Path $binaryPath)) {
            # Try finding the binary in subdirectories
            $binaryPath = Get-ChildItem -Path $tempDir -Name $BINARY_NAME -Recurse | Select-Object -First 1
            if ($binaryPath) {
                $binaryPath = Join-Path $tempDir $binaryPath
            }
        }
        
        if (-not (Test-Path $binaryPath)) {
            Write-Error "Could not find $BINARY_NAME in downloaded archive"
        }
        
        Copy-Item -Path $binaryPath -Destination (Join-Path $INSTALL_DIR $BINARY_NAME) -Force
        Write-Success "Installed $BINARY_NAME to $INSTALL_DIR\$BINARY_NAME"
    }
    finally {
        # Cleanup
        Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Add-ToPath {
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    
    if ($currentPath -notlike "*$INSTALL_DIR*") {
        Write-Info "Adding $INSTALL_DIR to user PATH..."
        $newPath = "$currentPath;$INSTALL_DIR"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        $env:Path = "$env:Path;$INSTALL_DIR"
        Write-Success "Added to PATH (restart your terminal for changes to take effect)"
    }
    else {
        Write-Info "$INSTALL_DIR already in PATH"
    }
}

function Test-Installation {
    $nodalyncPath = Join-Path $INSTALL_DIR $BINARY_NAME
    
    if (Test-Path $nodalyncPath) {
        try {
            $version = & $nodalyncPath --version 2>&1
            Write-Success "nodalync $version is ready!"
        }
        catch {
            Write-Success "nodalync installed successfully!"
        }
    }
    else {
        Write-Warn "Installation complete, but binary not found at expected location"
    }
}

function Show-NextSteps {
    Write-Host ""
    Write-Host "Installation complete!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Next steps:" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  1. Restart your terminal (or run: `$env:Path = [Environment]::GetEnvironmentVariable('Path', 'User'))"
    Write-Host ""
    Write-Host "  2. Initialize your identity:"
    Write-Host '     $env:NODALYNC_PASSWORD = "your-secure-password"'
    Write-Host "     nodalync init --wizard"
    Write-Host ""
    Write-Host "  3. Start your node:"
    Write-Host "     nodalync start"
    Write-Host ""
    Write-Host "  4. Publish content:"
    Write-Host '     nodalync publish my-document.md --title "My Knowledge"'
    Write-Host ""
    Write-Host "Documentation: https://github.com/$REPO#readme"
    Write-Host ""
}

# Main
function Main {
    Write-Host ""
    Write-Host "Nodalync Installer for Windows" -ForegroundColor Blue
    Write-Host "==============================="
    Write-Host ""
    
    $version = Get-LatestVersion
    Install-Nodalync -Version $version
    Add-ToPath
    Test-Installation
    Show-NextSteps
}

Main
