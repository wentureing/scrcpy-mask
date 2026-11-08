# 用本机已有依赖离线构建前端，并（可选）同步到人类测试目录。
#
# 原则：**只用已有 pnpm store，不下载**。store 缺包时直接失败并报告，不自动联网。
#
# 用法：
#   pwsh -NoProfile -File tools/build-frontend-local.ps1                 # 只离线安装 + 构建
#   pwsh -NoProfile -File tools/build-frontend-local.ps1 -Copy           # 构建后同步到测试目录
#   pwsh -NoProfile -File tools/build-frontend-local.ps1 -Copy -Target D:\other\assets\web

[CmdletBinding()]
param(
    # 本机已有 pnpm store（默认全局 store）
    [string]$StoreDir = "$env:LOCALAPPDATA\pnpm\store",
    # 人类测试目录的 assets/web
    [string]$Target = "C:\Users\86177\Desktop\scrcpy-mask改进计划\人类测试专用目录\assets\web",
    # 构建后是否同步
    [switch]$Copy
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$Frontend = Join-Path $Root "frontend"
$Dist = Join-Path $Frontend "dist"

if (-not (Test-Path $StoreDir)) {
    throw "pnpm store 不存在：$StoreDir"
}
if (-not (Test-Path $Frontend)) {
    throw "frontend 目录不存在：$Frontend"
}

$storeFiles = (Get-ChildItem $StoreDir -Recurse -File -ErrorAction SilentlyContinue | Measure-Object).Count
Write-Host "使用已有 store：$StoreDir（$storeFiles 个文件，离线模式，不下载）"

Write-Host "`n[1/3] 离线安装依赖..."
& pnpm --dir $Frontend install --offline --frozen-lockfile --store-dir $StoreDir
if ($LASTEXITCODE -ne 0) {
    throw "离线安装失败：store 里缺少依赖。请先确认缺失包，勿自动下载。"
}

Write-Host "`n[2/3] 构建前端..."
& pnpm --dir $Frontend build
if ($LASTEXITCODE -ne 0) {
    throw "前端构建失败。"
}

if (-not (Test-Path (Join-Path $Dist "index.html"))) {
    throw "构建产物缺少 index.html：$Dist"
}

if (-not $Copy) {
    Write-Host "`n完成（未同步）。产物：$Dist"
    Write-Host "如需同步到测试目录，追加 -Copy"
    exit 0
}

Write-Host "`n[3/3] 同步到测试目录：$Target"
if (Test-Path $Target) {
    $backup = "$Target.bak-$(Get-Date -Format yyyyMMdd-HHmmss)"
    Move-Item -Path $Target -Destination $backup
    Write-Host "旧目录已备份：$backup"
}
Copy-Item -Path $Dist -Destination $Target -Recurse -Force
Write-Host "已同步：$Target"
