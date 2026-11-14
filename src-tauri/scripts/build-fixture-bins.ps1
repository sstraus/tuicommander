$ErrorActionPreference = 'Stop'
# CI must include workspace dev-dependency unification, not just the test profile.
if ($env:TUIC_FIXTURE_FEATURES) {
    $cargoArgs = @('test', '--workspace', '--no-run', '--features', $env:TUIC_FIXTURE_FEATURES)
} else {
    $cargoArgs = @('build', '--profile', 'test', '--no-default-features', '--features', 'test-fixtures',
        '--bin', 'tuic-acp-fixture-agent', '--bin', 'tuic-mcp-fixture-server')
}
# llvm-cov passes its target directory only to nextest. Its profile output
# identifies the same artifact root without guessing a target directory name.
if ($env:CARGO_LLVM_COV -eq '1') {
    if (-not $env:LLVM_PROFILE_FILE) { throw 'llvm-cov must provide its profile output path' }
    $cargoArgs += @('--target-dir', (Split-Path -Parent $env:LLVM_PROFILE_FILE))
}
cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
