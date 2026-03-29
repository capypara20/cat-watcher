# ランダムな間隔で特定のディレクトリにファイルを作成するスクリプト

param(
    [Parameter(Mandatory)][string]$OutputDir,
    [Parameter(Mandatory)][int]$FileCount,
    [Parameter(Mandatory)][int]$MinInterval,   # 最小間隔（秒）
    [Parameter(Mandatory)][int]$MaxInterval    # 最大間隔（秒）
)

if ($MinInterval -gt $MaxInterval) {
    Write-Host @"
エラー: MinInterval($MinInterval) が MaxInterval($MaxInterval) より大きいです。

使い方:
  .\create_random_files.ps1 -OutputDir <出力先> -FileCount <ファイル数> -MinInterval <最小秒> -MaxInterval <最大秒>

例:
  .\create_random_files.ps1 -OutputDir ".\output" -FileCount 10 -MinInterval 1 -MaxInterval 5
"@
    exit 1
}

# 出力ディレクトリが無ければ作成
if (-not (Test-Path $OutputDir)) {
    New-Item -Path $OutputDir -ItemType Directory | Out-Null
    Write-Host "ディレクトリ作成: $OutputDir"
}

for ($i = 1; $i -le $FileCount; $i++) {
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss_fff"
    $fileName = "file_${timestamp}_${i}.txt"
    $filePath = Join-Path $OutputDir $fileName

    $content = "作成日時: $(Get-Date)`nファイル番号: $i"
    Set-Content -Path $filePath -Value $content -Encoding UTF8

    Write-Host "[$i/$FileCount] 作成: $fileName"

    if ($i -lt $FileCount) {
        $wait = Get-Random -Minimum $MinInterval -Maximum ($MaxInterval + 1)
        Write-Host "  -> ${wait}秒待機..."
        Start-Sleep -Seconds $wait
    }
}

Write-Host "`n完了: ${FileCount}個のファイルを作成しました"
