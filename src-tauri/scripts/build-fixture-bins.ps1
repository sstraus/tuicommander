$ErrorActionPreference = 'Stop'
# CI must include workspace dev-dependency unification, not just the test profile.
if ($env:TUIC_FIXTURE_FEATURES) {
    $cargoArgs = @('test', '--workspace', '--no-run', '--features', $env:TUIC_FIXTURE_FEATURES)
} else {
    $cargoArgs = @('build', '--profile', 'test', '--no-default-features',
        '--bin', 'tuic-acp-fixture-agent', '--bin', 'tuic-mcp-fixture-server')
}
cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
