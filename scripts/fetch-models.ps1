[CmdletBinding()]
param([switch]$IncludeBase)

$ErrorActionPreference = 'Stop'
$modelDirectory = Join-Path $PSScriptRoot '..\src-tauri\resources\models'
New-Item -ItemType Directory -Force -Path $modelDirectory | Out-Null

$models = @(
    @{ Name = 'ggml-tiny.bin'; Url = 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny-q5_1.bin'; Sha256 = '818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7' }
)
if ($IncludeBase) {
    $models += @{ Name = 'ggml-base.bin'; Url = 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin'; Sha256 = '60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe' }
}

foreach ($model in $models) {
    $destination = Join-Path $modelDirectory $model.Name
    $needsDownload = -not (Test-Path -LiteralPath $destination)
    if (-not $needsDownload) {
        $existingHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash.ToLowerInvariant()
        $needsDownload = $existingHash -ne $model.Sha256
    }
    if ($needsDownload) {
        Write-Host "Downloading $($model.Name)..."
        Invoke-WebRequest -Uri $model.Url -OutFile $destination
    }
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash.ToLowerInvariant()
    if ($model.Sha256 -and $hash -ne $model.Sha256) {
        throw "Checksum validation failed for $($model.Name)"
    }
    Write-Host "$($model.Name) SHA256 $hash"
}
