param([ValidateSet('windows','macos')][string]$Platform = 'windows')
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$outputDir = Join-Path $repoRoot 'artifacts'
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
if ($Platform -eq 'windows') {
    $binary = Join-Path $repoRoot 'target/release/archiveflow.exe'
    if (!(Test-Path -LiteralPath $binary)) { throw 'Run cargo build --release first.' }
    Compress-Archive -LiteralPath $binary -DestinationPath (Join-Path $outputDir 'ArchiveFlow-windows-x64.zip') -Force
} else {
    $binary = Join-Path $repoRoot 'target/release/archiveflow'
    if (!(Test-Path -LiteralPath $binary)) { throw 'Run cargo build --release first.' }
    $app = Join-Path $outputDir 'ArchiveFlow.app/Contents'
    New-Item -ItemType Directory -Force -Path (Join-Path $app 'MacOS'),(Join-Path $app 'Resources') | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $app 'MacOS/archiveflow')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'crates/desktop/icons/icon.icns') -Destination (Join-Path $app 'Resources/icon.icns')
    @'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>archiveflow</string>
<key>CFBundleIdentifier</key><string>com.archiveflow.app</string>
<key>CFBundleName</key><string>ArchiveFlow</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>icon.icns</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
'@ | Set-Content -LiteralPath (Join-Path $app 'Info.plist') -Encoding utf8NoBOM
    & chmod +x (Join-Path $app 'MacOS/archiveflow')
    if ($LASTEXITCODE -ne 0) { throw 'Failed to mark application executable.' }
    & ditto -c -k --sequesterRsrc --keepParent (Join-Path $outputDir 'ArchiveFlow.app') (Join-Path $outputDir 'ArchiveFlow-macos-arm64.zip')
    if ($LASTEXITCODE -ne 0) { throw 'Failed to package macOS application.' }
}
