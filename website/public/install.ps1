$ErrorActionPreference = "Stop"

$repo = "sstraus/tuicommander"
$apiUrl = "https://api.github.com/repos/$repo/releases/latest"

Write-Host "Fetching latest TUICommander release..." -ForegroundColor Cyan

$release = Invoke-RestMethod -Uri $apiUrl
$asset = $release.assets | Where-Object { $_.name -like '*x64-setup.exe' } | Select-Object -First 1

if (-not $asset) {
    Write-Host "Error: Windows installer not found in latest release" -ForegroundColor Red
    exit 1
}

$version = $release.tag_name
$outFile = Join-Path $env:TEMP "TUICommander-setup.exe"

Write-Host "Downloading TUICommander $version..." -ForegroundColor Cyan
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $outFile

Write-Host "Running installer..." -ForegroundColor Cyan
Start-Process -FilePath $outFile -Wait

Write-Host "TUICommander $version installed successfully!" -ForegroundColor Green
