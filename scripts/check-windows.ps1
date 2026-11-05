$ErrorActionPreference = "Stop"

. "$PSScriptRoot\ffmpeg-env.ps1"

$ProjectDir = Resolve-Path "$PSScriptRoot\.."
Push-Location $ProjectDir
# --locked：强制使用仓库里提交的 Cargo.lock。
# 否则 cargo 可能重新解析依赖（含 bevy_ineffable 这个 git 依赖），导致编译指纹变化、缓存失效。
cargo check --locked
$ExitCode = $LASTEXITCODE
Pop-Location

exit $ExitCode
