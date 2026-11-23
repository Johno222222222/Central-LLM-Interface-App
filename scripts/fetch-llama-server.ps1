# Downloads the pinned CUDA 13.5 llama-server build into the Tauri resources
# folder. CUDA 13.5 is the Windows build that can run on an RTX 5070.
# CUDA 12.4 builds in the same release do not support that GPU.
$ErrorActionPreference = "Stop"
$Tag = if ($env:LLAMA_CPP_TAG) { $env:LLAMA_CPP_TAG } else { "b12651" }
$Root = Split-Path -Parent $PSScriptRoot
$Dest = Join-Path $Root "src-tauri\resources\llama"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$Server = Join-Path $Dest "llama-server.exe"
if ((Test-Path $Server) -and -not $env:LLAMA_CPP_FORCE) {
    Write-Host "llama-server.exe already present"
    exit 0
}

$Base = "https://github.com/ggml-org/llama.cpp/releases/download/$Tag"
$Files = @(
    "llama-$Tag-bin-win-cuda-13.5-x64.zip",
    "cudart-llama-bin-win-cuda-13.5-x64.zip"
)
$Staging = Join-Path $env:TEMP "cllm-llama-$Tag"
if (Test-Path $Staging) { Remove-Item -Recurse -Force $Staging }
New-Item -ItemType Directory -Force -Path $Staging | Out-Null

foreach ($Name in $Files) {
    $Zip = Join-Path $Staging $Name
    Write-Host "Downloading $Name"
    Invoke-WebRequest -Uri "$Base/$Name" -OutFile $Zip
    Expand-Archive -Path $Zip -DestinationPath $Staging -Force
}

Get-ChildItem $Staging -Recurse -File | Where-Object { $_.Extension -in ".exe", ".dll" } | ForEach-Object {
    Copy-Item $_.FullName -Destination (Join-Path $Dest $_.Name) -Force
}

if (-not (Test-Path $Server)) {
    throw "llama-server.exe was not in the $Tag cuda-13.5 archive."
}
Write-Host "Bundled llama-server from $Tag (cuda-13.5)."
