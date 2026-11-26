# Install the verified LLVM toolchain used by WebRTC bindgen on Windows.
$ErrorActionPreference = 'Stop'

$version = '22.1.8'
$installer = Join-Path $env:RUNNER_TEMP "LLVM-$version-win64.exe"
$installDir = Join-Path $env:RUNNER_TEMP "tuic-llvm-$version"
$url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/LLVM-$version-win64.exe"
& curl.exe --fail --location --silent --show-error --output $installer $url
if ($LASTEXITCODE -ne 0) { throw "LLVM installer download failed: $LASTEXITCODE" }
$expected = '16e5709785fef73c854646241c4a92c5cd574318d1b33c63330dd7721903e55c'
$actual = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
if ($actual -ne $expected) {
  $bytes = (Get-Item -LiteralPath $installer).Length
  throw "LLVM installer SHA-256 mismatch ($bytes bytes): expected $expected, got $actual"
}
# NSIS requires the unquoted destination argument to be last.
$install = Start-Process -FilePath $installer -ArgumentList "/S /D=$installDir" -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "LLVM installer failed: $($install.ExitCode)" }
$bin = Join-Path $installDir 'bin'
if (-not (Test-Path -LiteralPath (Join-Path $bin 'libclang.dll') -PathType Leaf)) {
  throw 'LLVM installer did not provide libclang.dll'
}
"LIBCLANG_PATH=$bin" >> $env:GITHUB_ENV
Remove-Item -LiteralPath $installer
