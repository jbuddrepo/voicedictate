[CmdletBinding()]
param([ValidateSet('Debug','Release')] [string]$Configuration = 'Release')

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$tauriRoot = Join-Path $projectRoot 'src-tauri'
$workerRoot = Join-Path $tauriRoot 'worker'
$cargoProfile = if ($Configuration -eq 'Release') { 'release' } else { 'debug' }
$appVersion = (Get-Content -Raw -LiteralPath (Join-Path $projectRoot 'package.json') | ConvertFrom-Json).version

Push-Location $projectRoot
try {
    if (-not (Test-Path -LiteralPath 'src-tauri\resources\models\ggml-tiny.bin')) {
        & "$PSScriptRoot\fetch-models.ps1"
    }
    pnpm install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw 'pnpm install failed.' }
    pnpm tauri icon assets\icon.svg
    if ($LASTEXITCODE -ne 0) { throw 'Tauri icon generation failed.' }

    $targetTriple = (rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
    $binaryDirectory = Join-Path $tauriRoot 'binaries'
    New-Item -ItemType Directory -Force -Path $binaryDirectory | Out-Null

    # Separate target directories ensure the two CPU instruction variants never mix.
    # whisper-rs generates target-correct Windows bindings through LLVM/libclang.
    $llvmBin = Join-Path $env:ProgramFiles 'LLVM\bin'
    $localClang = Join-Path $projectRoot 'work\libclang\runtimes\win-x64\native'
    if (Test-Path -LiteralPath (Join-Path $llvmBin 'libclang.dll')) { $env:LIBCLANG_PATH = $llvmBin }
    elseif (Test-Path -LiteralPath (Join-Path $localClang 'libclang.dll')) { $env:LIBCLANG_PATH = $localClang }
    else { throw 'libclang.dll was not found. Install LLVM or extract the libclang Windows runtime under work\libclang.' }
    Push-Location $workerRoot
    try {
        $env:CARGO_TARGET_DIR = Join-Path $workerRoot 'target-baseline'
        $env:CMAKE_PROJECT_INCLUDE = Join-Path $projectRoot 'scripts\cmake\baseline.cmake'
        $env:CMAKE_MSVC_RUNTIME_LIBRARY = 'MultiThreaded'
        $env:RUSTFLAGS = '-C target-feature=+crt-static,+avx,-avx2,-fma'
        if ($Configuration -eq 'Release') { cargo build --release } else { cargo build }
        if ($LASTEXITCODE -ne 0) { throw 'The baseline Whisper worker build failed.' }
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$cargoProfile\voicedictate-worker.exe") -Destination (Join-Path $binaryDirectory "voicedictate-worker-$targetTriple.exe") -Force

        $env:CARGO_TARGET_DIR = Join-Path $workerRoot 'target-avx2'
        $env:CMAKE_PROJECT_INCLUDE = Join-Path $projectRoot 'scripts\cmake\avx2.cmake'
        # Keep Rust build scripts host-safe; the whisper.cpp C++ core carries
        # the AVX2/FMA optimization through /arch:AVX2.
        $env:RUSTFLAGS = '-C target-feature=+crt-static'
        if ($Configuration -eq 'Release') { cargo build --release } else { cargo build }
        if ($LASTEXITCODE -ne 0) { throw 'The AVX2 Whisper worker build failed.' }
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$cargoProfile\voicedictate-worker.exe") -Destination (Join-Path $binaryDirectory "voicedictate-worker-avx2-$targetTriple.exe") -Force
    } finally {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        Remove-Item Env:CMAKE_PROJECT_INCLUDE -ErrorAction SilentlyContinue
        Remove-Item Env:CMAKE_MSVC_RUNTIME_LIBRARY -ErrorAction SilentlyContinue
        Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
        Pop-Location
    }

    if ($Configuration -eq 'Release') {
        # Automatic updater signing expects the key contents even when the
        # developer configured a key file path.
        if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
            $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw -LiteralPath $env:TAURI_SIGNING_PRIVATE_KEY_PATH
        }
        if ($env:TAURI_SIGNING_PRIVATE_KEY -or $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
            pnpm tauri:build --config updater.conf.json
        } else {
            pnpm tauri:build
        }
        if ($LASTEXITCODE -ne 0) { throw 'The Tauri application build failed.' }

        $artifactDirectory = Join-Path $projectRoot 'artifacts'
        $portableDirectory = Join-Path $artifactDirectory "VoiceDictate-Portable-$appVersion"
        New-Item -ItemType Directory -Force -Path (Join-Path $portableDirectory 'resources\models') | Out-Null
        $installer = Get-ChildItem -LiteralPath (Join-Path $tauriRoot 'target\release\bundle\nsis') -Filter "*$appVersion*setup.exe" | Select-Object -First 1
        if (-not $installer) { throw 'The NSIS installer was not produced.' }
        Copy-Item -LiteralPath $installer.FullName -Destination (Join-Path $artifactDirectory "VoiceDictate-Setup-$appVersion.exe") -Force
        $updaterSignature = "$($installer.FullName).sig"
        if (Test-Path -LiteralPath $updaterSignature) {
            Copy-Item -LiteralPath $updaterSignature -Destination (Join-Path $artifactDirectory "VoiceDictate-Setup-$appVersion.exe.sig") -Force
        }
        Copy-Item -LiteralPath (Join-Path $tauriRoot 'target\release\voicedictate.exe') -Destination (Join-Path $portableDirectory 'VoiceDictate.exe') -Force
        Copy-Item -LiteralPath (Join-Path $binaryDirectory "voicedictate-worker-$targetTriple.exe") -Destination (Join-Path $portableDirectory 'voicedictate-worker.exe') -Force
        Copy-Item -LiteralPath (Join-Path $binaryDirectory "voicedictate-worker-avx2-$targetTriple.exe") -Destination (Join-Path $portableDirectory 'voicedictate-worker-avx2.exe') -Force
        Copy-Item -LiteralPath (Join-Path $tauriRoot 'resources\models\ggml-tiny.bin') -Destination (Join-Path $portableDirectory 'resources\models\ggml-tiny.bin') -Force
        Copy-Item -LiteralPath (Join-Path $projectRoot 'PRIVACY.md') -Destination $portableDirectory -Force
        Copy-Item -LiteralPath (Join-Path $projectRoot 'REMOTE-TRANSCRIPTION.md') -Destination $portableDirectory -Force
        Copy-Item -LiteralPath (Join-Path $projectRoot 'BUILDING.md') -Destination $portableDirectory -Force
        Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD-PARTY-NOTICES.md') -Destination $portableDirectory -Force
        Copy-Item -LiteralPath (Join-Path $projectRoot 'PORTABLE-README.txt') -Destination $portableDirectory -Force
        Compress-Archive -Path "$portableDirectory\*" -DestinationPath (Join-Path $artifactDirectory "VoiceDictate-Portable-$appVersion.zip") -Force

        Get-ChildItem -LiteralPath $artifactDirectory -File | Where-Object Name -ne 'SHA256SUMS.txt' | ForEach-Object {
            $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
            "$hash  $($_.Name)"
        } | Set-Content -Encoding ascii (Join-Path $artifactDirectory 'SHA256SUMS.txt')
    } else {
        pnpm build
        if ($LASTEXITCODE -ne 0) { throw 'The frontend build failed.' }
        cargo test --manifest-path src-tauri\Cargo.toml
        if ($LASTEXITCODE -ne 0) { throw 'The desktop test suite failed.' }
    }
} finally { Pop-Location }
