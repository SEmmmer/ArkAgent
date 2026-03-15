param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "release"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoArgs = @("build", "-p", "akbox-desktop")

if ($Profile -eq "release") {
    $cargoArgs += "--release"
}

Write-Host "构建桌面程序（$Profile）..."
& cargo @cargoArgs

if ($LASTEXITCODE -ne 0) {
    throw "cargo build 失败，退出码：$LASTEXITCODE"
}

$sourceExe = Join-Path $repoRoot "target\$Profile\akbox-desktop.exe"
if (-not (Test-Path $sourceExe)) {
    throw "未找到构建产物：$sourceExe"
}

$distDirectory = Join-Path $repoRoot "dist"
New-Item -ItemType Directory -Force -Path $distDirectory | Out-Null

$artifactName = if ($Profile -eq "release") {
    "方舟看号台.exe"
} else {
    "方舟看号台-调试.exe"
}

$artifactPath = Join-Path $distDirectory $artifactName
Copy-Item -Path $sourceExe -Destination $artifactPath -Force

Write-Host "已生成中文桌面产物：$artifactPath"
