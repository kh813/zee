<#
.SYNOPSIS
    Build and task automation script for zee on Windows (PowerShell alternative to make).
.DESCRIPTION
    Provides familiar make-like targets (all, gui, tui, test, check, package, clean)
    for building and testing zee on Windows without needing GNU make or bash.
.EXAMPLE
    .\make.ps1
    .\make.ps1 gui
    .\make.ps1 tui
    .\make.ps1 test
    .\make.ps1 check
    .\make.ps1 package
    .\make.ps1 clean
#>

[CmdletBinding()]
param (
    [Parameter(Position = 0)]
    [ValidateSet('all', 'default', 'local', 'gui', 'tui', 'cli', 'test', 'check', 'package', 'clean', 'help')]
    [string]$Target = 'all',

    [Parameter()]
    [bool]$Release = $true
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ProjectRoot

$DistDir = Join-Path $ProjectRoot "dist"
$TargetFolder = if ($Release) { "release" } else { "debug" }

function Ensure-DistDir {
    if (-not (Test-Path $DistDir)) {
        New-Item -ItemType Directory -Path $DistDir | Out-Null
    }
}

switch ($Target) {
    'clean' {
        Write-Host "==> Cleaning build artifacts..." -ForegroundColor Yellow
        if (Test-Path $DistDir) { Remove-Item -Recurse -Force $DistDir }
        cargo clean
        Write-Host "Clean complete." -ForegroundColor Green
    }

    'check' {
        Write-Host "==> Checking workspace..." -ForegroundColor Cyan
        cargo check --workspace --all-targets
    }

    'test' {
        Write-Host "==> Running workspace tests..." -ForegroundColor Cyan
        cargo test --workspace
    }

    { $_ -in 'tui', 'cli' } {
        Ensure-DistDir
        Write-Host "==> Building Windows CLI (zee-cli.exe)..." -ForegroundColor Cyan
        if ($Release) {
            cargo build --release -p zee-tui
        } else {
            cargo build -p zee-tui
        }
        $SrcBin = Join-Path $ProjectRoot "target\$TargetFolder\zee-cli.exe"
        if (-not (Test-Path $SrcBin)) {
            $SrcBin = Join-Path $ProjectRoot "target\$TargetFolder\zee.exe"
        }
        $DstBin = Join-Path $DistDir "zee-cli.exe"
        if (Test-Path $SrcBin) {
            Copy-Item $SrcBin $DstBin -Force
            Write-Host "Built: $DstBin" -ForegroundColor Green
        }
    }

    'gui' {
        Ensure-DistDir
        Write-Host "==> Building Windows GUI (zee.exe)..." -ForegroundColor Cyan
        if ($Release) {
            cargo build --release -p zee-gui
        } else {
            cargo build -p zee-gui
        }
        $SrcBin = Join-Path $ProjectRoot "target\$TargetFolder\zeeg.exe"
        $DstBin = Join-Path $DistDir "zee.exe"
        if (Test-Path $SrcBin) {
            Copy-Item $SrcBin $DstBin -Force
            Write-Host "Built: $DstBin" -ForegroundColor Green
        }
    }

    'package' {
        Ensure-DistDir
        Write-Host "==> Building binaries for package..." -ForegroundColor Cyan
        cargo build --release -p zee-gui
        cargo build --release -p zee-tui
        
        $GuiSrcBin = Join-Path $ProjectRoot "target\release\zeeg.exe"
        $TuiSrcBin = Join-Path $ProjectRoot "target\release\zee.exe"
        $StageDir = Join-Path $DistDir "zee-windows"
        if (Test-Path $StageDir) { Remove-Item -Recurse -Force $StageDir }
        New-Item -ItemType Directory -Path $StageDir | Out-Null
        
        Copy-Item $GuiSrcBin (Join-Path $StageDir "zee.exe") -Force
        Copy-Item $TuiSrcBin (Join-Path $StageDir "zee-cli.exe") -Force
        if (Test-Path "README.md") { Copy-Item "README.md" $StageDir -Force }
        if (Test-Path "MANUAL.md") { Copy-Item "MANUAL.md" $StageDir -Force }
        if (Test-Path "LICENSE") { Copy-Item "LICENSE" $StageDir -Force }
        
        $ZipPath = Join-Path $DistDir "zee-windows-x64.zip"
        if (Test-Path $ZipPath) { Remove-Item -Force $ZipPath }
        
        Write-Host "==> Creating zip archive: $ZipPath" -ForegroundColor Cyan
        Compress-Archive -Path "$StageDir\*" -DestinationPath $ZipPath -Force
        Remove-Item -Recurse -Force $StageDir
        Write-Host "Created: $ZipPath" -ForegroundColor Green
    }

    { $_ -in 'all', 'default', 'local' } {
        Ensure-DistDir
        Write-Host "==> Building Windows GUI (zee.exe)..." -ForegroundColor Cyan
        if ($Release) {
            cargo build --release -p zee-gui
        } else {
            cargo build -p zee-gui
        }
        $SrcBin = Join-Path $ProjectRoot "target\$TargetFolder\zeeg.exe"
        $DstBin = Join-Path $DistDir "zee.exe"
        if (Test-Path $SrcBin) {
            Copy-Item $SrcBin $DstBin -Force
            Write-Host "==> Build complete in $DistDir" -ForegroundColor Green
            Write-Host "Built: $DstBin" -ForegroundColor Green
        }
    }

    'help' {
        Write-Host "zee Windows Build Script (make.ps1)" -ForegroundColor Yellow
        Write-Host "Usage: .\make.ps1 [target] [-Release <`$true|`$false>]"
        Write-Host ""
        Write-Host "Available targets:"
        Write-Host "  .\make.ps1            - Build Windows GUI as dist/zee.exe (default)"
        Write-Host "  .\make.ps1 gui        - Build Windows GUI (dist/zee.exe)"
        Write-Host "  .\make.ps1 cli        - Build Windows CLI (dist/zee-cli.exe)"
        Write-Host "  .\make.ps1 test       - Run tests (cargo test --workspace)"
        Write-Host "  .\make.ps1 check      - Check workspace (cargo check --workspace)"
        Write-Host "  .\make.ps1 package    - Build and package into dist/zee-windows-x64.zip"
        Write-Host "  .\make.ps1 clean      - Remove build artifacts and dist/ directory"
        Write-Host "  .\make.ps1 help       - Show this help message"
    }
}
