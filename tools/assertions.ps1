# 关键路径断言（ratchet 基线）
#
# 目的：把"改漏了 13 个文件之一""改名时某语言缺键""行数反弹"这类机械错误，
#      变成 CI 里会红的东西，而不是靠人肉 grep。
#
# 用法：
#   pwsh -NoProfile -File tools/assertions.ps1                    # 校验（CI 用）
#   pwsh -NoProfile -File tools/assertions.ps1 -UpdateBaseline    # 收紧基线（改完并确认后手动执行）
#
# 规则（ratchet / 只允许变好）：
#   * 结构指标超过基线 → FAIL
#   * 结构指标低于基线 → PASS，并提示可以收紧基线
#   * i18n：以 zh-CN 为基准；某语言"当前缺失键"里出现基线之外的新键 → FAIL
#   * i18n：基线里记的缺口已补齐 → PASS，提示收紧
#
# 已知既存缺口（截至 2026-11-04 的 master）：
#   es-ES / ja-JP / pt-BR / ru-RU 的 frontend i18n 各缺 14 个 `mappings.fps.setting.interaction*` 键。
#   这些已写入基线，不算失败；但**不允许再新增**缺键。
#
# 本脚本只读仓库文件；-UpdateBaseline 是唯一写操作（只写基线 json）。

[CmdletBinding()]
param(
    [switch]$UpdateBaseline
)

$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$BaselinePath = Join-Path $PSScriptRoot 'assertions-baseline.json'
$FrontendI18n = Join-Path $Root 'frontend/src/i18n'
$BackendLocales = Join-Path $Root 'assets/locales'

$failures = New-Object System.Collections.Generic.List[string]
$notices = New-Object System.Collections.Generic.List[string]

function Write-Section([string]$text) {
    Write-Host ""
    Write-Host "== $text ==" -ForegroundColor Cyan
}

# ---------- JSON 键路径 ----------

function Get-JsonKeyPaths {
    param($Node, [string]$Prefix = '', [System.Collections.Generic.List[string]]$Acc)
    if ($null -eq $Node) { return }
    if ($Node -is [System.Management.Automation.PSCustomObject]) {
        foreach ($prop in $Node.PSObject.Properties) {
            $key = if ($Prefix) { "$Prefix.$($prop.Name)" } else { $prop.Name }
            if ($prop.Value -is [System.Management.Automation.PSCustomObject]) {
                Get-JsonKeyPaths -Node $prop.Value -Prefix $key -Acc $Acc
            }
            else {
                $Acc.Add($key)
            }
        }
    }
    elseif ($Node -is [System.Collections.IEnumerable] -and $Node -isnot [string]) {
        $Acc.Add($Prefix)
    }
}

function Get-KeySet([string]$Path) {
    $set = New-Object 'System.Collections.Generic.HashSet[string]'
    $json = Get-Content -Path $Path -Raw -Encoding utf8 | ConvertFrom-Json
    $acc = New-Object System.Collections.Generic.List[string]
    Get-JsonKeyPaths -Node $json -Acc $acc
    foreach ($k in $acc) { [void]$set.Add($k) }
    return , $set
}

# ---------- i18n 扫描 ----------

$i18nCurrent = [ordered]@{}

function Test-I18nGroup {
    param(
        [string]$GroupName,
        [string]$Directory,
        [string]$Pattern,
        [string]$Reference
    )

    Write-Section "i18n / $GroupName"

    $files = @(Get-ChildItem -Path $Directory -Filter $Pattern -Recurse -File | ForEach-Object {
            [pscustomobject]@{
                FullName = $_.FullName
                RelPath  = $_.FullName.Substring($Directory.Length + 1).Replace('\', '/')
            }
        })

    if ($files.Count -eq 0) {
        $failures.Add("[$GroupName] 未找到任何语言文件（$Directory/$Pattern）")
        return
    }

    $refFile = $files | Where-Object { $_.RelPath -eq $Reference } | Select-Object -First 1
    if (-not $refFile) {
        $failures.Add("[$GroupName] 缺少基准语言文件 $Reference（现有: $(($files | ForEach-Object { $_.RelPath }) -join ', ')）")
        return
    }

    $refKeys = Get-KeySet $refFile.FullName
    Write-Host ("基准 {0}: {1} 个键" -f $refFile.RelPath, $refKeys.Count)

    foreach ($file in $files) {
        $keys = Get-KeySet $file.FullName
        $missing = @($refKeys | Where-Object { -not $keys.Contains($_) })
        $extra = @($keys | Where-Object { -not $refKeys.Contains($_) })
        $i18nCurrent["$GroupName/$($file.RelPath)"] = @($missing)

        $status = if ($missing.Count -eq 0) { '完整' } else { "缺 $($missing.Count) 键" }
        Write-Host ("  {0,-30} {1} 键（{2}）" -f $file.RelPath, $keys.Count, $status)

        if ($extra.Count -gt 0) {
            $sampleExtra = ($extra | Select-Object -First 5) -join ', '
            $notices.Add("[$GroupName] $($file.RelPath) 比基准多 $($extra.Count) 个键: $sampleExtra")
        }
    }
}

# 国际化检查已按用户要求关闭（只维护中文，不做 i18n 校验）。
# Test-I18nGroup -GroupName 'frontend' -Directory $FrontendI18n -Pattern 'translation.json' -Reference 'zh-CN/translation.json'
# Test-I18nGroup -GroupName 'backend' -Directory $BackendLocales -Pattern '*.json' -Reference 'zh-CN.json'

# ---------- 结构指标 ----------

Write-Section '结构指标'

$buttonFiles = @(Get-ChildItem -Path (Join-Path $Root 'frontend/src/components/mappings') -Filter 'Button*.tsx' -File)
$buttonLines = ($buttonFiles | ForEach-Object { (Get-Content $_.FullName).Count } | Measure-Object -Sum).Sum

$dragMatches = @(Get-ChildItem -Path (Join-Path $Root 'frontend/src') -Recurse -Include *.ts,*.tsx -File |
        Select-String -Pattern 'mappingButtonDragFactory\(' -AllMatches)
$dragCalls = ($dragMatches | ForEach-Object { $_.Matches.Count } | Measure-Object -Sum).Sum
if ($null -eq $dragCalls) { $dragCalls = 0 }

function Get-LineCount([string]$RelativePath) {
    $p = Join-Path $Root $RelativePath
    if (-not (Test-Path $p)) { return 0 }
    return (Get-Content $p).Count
}

# 发射器收口指标（docs/21 S6.6 / docs/27 A2）：
# 统计 `ControlMsgHelper::send_touch(` 的调用点数量，只允许下降（ratchet）。
# A1（S6.1a）完成后应显著减少；回升说明有人绕过了 IntentSender。
$sendTouchCalls = @(
    Get-ChildItem -Path (Join-Path $Root 'src') -Recurse -Include *.rs -File |
        Select-String -Pattern 'ControlMsgHelper::send_touch\(' -AllMatches
).Count

# ---------- 依赖方向（S1，docs/33）----------
# 只检查 `crate::<domain>` 形式的引用；禁止反向依赖（web/mask 内部细节不被底层引用）。
$dependencyViolations = 0
$depRules = @(
    @{ Path = 'src/scrcpy'; Forbidden = @('crate::mask', 'crate::web') },
    @{ Path = 'src/config.rs'; Forbidden = @('crate::mask', 'crate::web', 'crate::scrcpy') },
    @{ Path = 'src/access_rules.rs'; Forbidden = @('crate::mask', 'crate::web', 'crate::scrcpy') },
    @{ Path = 'src/input_effects.rs'; Forbidden = @('crate::mask', 'crate::web', 'crate::scrcpy') },
    @{ Path = 'src/mask'; Forbidden = @('crate::web') },
    @{ Path = 'src/utils'; Forbidden = @('crate::mask', 'crate::web') }
)
foreach ($rule in $depRules) {
    $target = Join-Path $Root $rule.Path
    if (-not (Test-Path $target)) { continue }
    $files = if ((Get-Item $target).PSIsContainer) {
        Get-ChildItem $target -Recurse -Include *.rs -File
    } else {
        @(Get-Item $target)
    }
    foreach ($file in $files) {
        $text = Get-Content $file.FullName -Raw -Encoding utf8
        foreach ($forbidden in $rule.Forbidden) {
            $pattern = [regex]::Escape($forbidden) + '[^A-Za-z0-9_]'
            $count = ([regex]::Matches($text, $pattern)).Count
            if ($count -gt 0) {
                $dependencyViolations += $count
                Write-Host ("  依赖违规：{0} 引用 {1}（{2} 处）" -f $file.FullName.Replace($Root, ''), $forbidden, $count) -ForegroundColor Yellow
            }
        }
    }
}

$structure = [ordered]@{
    mappingButtonDragFactoryCalls = [int]$dragCalls
    buttonTsxTotalLines           = [int]$buttonLines
    buttonTsxFiles                = [int]$buttonFiles.Count
    devicesTsxLines               = [int](Get-LineCount 'frontend/src/components/Devices.tsx')
    mappingsTsxLines              = [int](Get-LineCount 'frontend/src/components/mappings/Mappings.tsx')
    settingsTsxLines              = [int](Get-LineCount 'frontend/src/components/Settings.tsx')
    sendTouchCallSites            = [int]$sendTouchCalls
    dependencyDirectionViolations = [int]$dependencyViolations
}

# ---------- 基线 ----------

$baseline = $null
if (Test-Path $BaselinePath) {
    $baseline = Get-Content -Path $BaselinePath -Raw -Encoding utf8 | ConvertFrom-Json
}

if ($UpdateBaseline) {
    $payload = [ordered]@{ structure = $structure; i18nMissing = $i18nCurrent }
    $payload | ConvertTo-Json -Depth 6 | Set-Content -Path $BaselinePath -Encoding utf8
    Write-Host ""
    Write-Host "已写入基线: $BaselinePath"
    $structure.GetEnumerator() | ForEach-Object { Write-Host ("  {0} = {1}" -f $_.Key, $_.Value) }
    $i18nCurrent.GetEnumerator() | ForEach-Object {
        Write-Host ("  i18nMissing[{0}] = {1} 键" -f $_.Key, $_.Value.Count)
    }
    exit 0
}

if (-not $baseline) {
    Write-Host "基线文件不存在：按当前值初始化（本次不失败）"
    $payload = [ordered]@{ structure = $structure; i18nMissing = $i18nCurrent }
    $payload | ConvertTo-Json -Depth 6 | Set-Content -Path $BaselinePath -Encoding utf8
    $baseline = $payload
}

# ---------- 比较：结构指标 ----------

Write-Host ""
foreach ($key in $structure.Keys) {
    $current = $structure[$key]
    $base = $baseline.structure.$key
    if ($null -eq $base) {
        $notices.Add("基线里缺少结构指标 $key，建议执行 -UpdateBaseline 补齐")
        Write-Host ("  {0,-30} 当前 {1,6}  （基线无此项）" -f $key, $current)
        continue
    }
    if ($current -gt $base) {
        # 只有"重复度"指标是硬门槛；文件行数会因正常功能新增而增长，只提示不阻塞
        $hardMetrics = @('mappingButtonDragFactoryCalls', 'buttonTsxTotalLines', 'buttonTsxFiles', 'dependencyDirectionViolations')
        if ($hardMetrics -contains $key) {
            $failures.Add("结构指标回归：$key = $current > 基线 $base")
            Write-Host ("  {0,-30} 当前 {1,6}  基线 {2,6}  ← 回归" -f $key, $current, $base) -ForegroundColor Red
        }
        else {
            $notices.Add("$key 增加（$base → $current），若属正常功能新增可执行 -UpdateBaseline")
            Write-Host ("  {0,-30} 当前 {1,6}  基线 {2,6}  ← 增加（仅提示）" -f $key, $current, $base) -ForegroundColor Yellow
        }
    }
    elseif ($current -lt $base) {
        $notices.Add("$key 已改善（$base → $current），可以执行 -UpdateBaseline 收紧")
        Write-Host ("  {0,-30} 当前 {1,6}  基线 {2,6}  ← 改善" -f $key, $current, $base) -ForegroundColor Green
    }
    else {
        Write-Host ("  {0,-30} 当前 {1,6}  基线 {2,6}" -f $key, $current, $base)
    }
}

# ---------- 比较：i18n 缺失集合 ----------
# ---------- i18n（已暂停：只维护中文，不跑 i18n 检查）----------

Write-Section 'i18n 检查（已暂停）'
Write-Host "  已暂停（用户要求，只维护 zh-CN）"

# ---------- 汇总 ----------

Write-Section '汇总'

foreach ($n in $notices) { Write-Host "提示: $n" -ForegroundColor Yellow }

$summaryPath = $env:GITHUB_STEP_SUMMARY
if ($summaryPath) {
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add('### 关键路径断言')
    $lines.Add('')
    $lines.Add('| 结构指标 | 当前 | 基线 |')
    $lines.Add('| --- | --- | --- |')
    foreach ($key in $structure.Keys) {
        $lines.Add(("| {0} | {1} | {2} |" -f $key, $structure[$key], $baseline.structure.$key))
    }
    $lines.Add('')
    $lines.Add('| i18n 文件 | 缺失（当前 / 基线） |')
    $lines.Add('| --- | --- |')
    foreach ($fileKey in $i18nCurrent.Keys) {
        $cur = @($i18nCurrent[$fileKey] | Where-Object { $_ }).Count
        $baseCount = 0
        if ($baselineMissing -and $baselineMissing.PSObject.Properties.Name -contains $fileKey) {
            $baseCount = @($baselineMissing.$fileKey | Where-Object { $_ }).Count
        }
        $lines.Add(("| {0} | {1} / {2} |" -f $fileKey, $cur, $baseCount))
    }
    $lines.Add('')
    if ($failures.Count -gt 0) {
        $lines.Add('**失败项**')
        foreach ($f in $failures) { $lines.Add("- $f") }
    }
    else {
        $lines.Add('全部通过 ✅')
    }
    $lines | Out-File -FilePath $summaryPath -Append -Encoding utf8
}

if ($failures.Count -gt 0) {
    Write-Host ""
    foreach ($f in $failures) { Write-Host "FAIL: $f" -ForegroundColor Red }
    exit 1
}

Write-Host "全部通过" -ForegroundColor Green
exit 0
