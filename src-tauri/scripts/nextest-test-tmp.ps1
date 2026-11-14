# Nextest setup script (Windows): same roots as nextest-test-tmp.sh, in the
# order scripts/test-tmp-lib.sh documents.
$ErrorActionPreference = 'Stop'
$hostTmp = if ($env:TUIC_TEST_HOST_TMPDIR) { $env:TUIC_TEST_HOST_TMPDIR } else { [System.IO.Path]::GetTempPath() }
$hostTmp = $hostTmp.TrimEnd('\', '/')
if ($env:TUIC_TEST_TMP_ROOT) {
    $root = $env:TUIC_TEST_TMP_ROOT
} else {
    $base = if ($env:TUIC_TEST_TMP_BASE) { $env:TUIC_TEST_TMP_BASE.TrimEnd('\', '/') } else { Join-Path $hostTmp 'tuic-tests' }
    # FNV-1a 64 of the checkout path, as tuic-test-support's checkout_hash().
    $checkout = (Resolve-Path '..').ProviderPath
    $mod = [System.Numerics.BigInteger]::Pow(2, 64)
    $hash = [System.Numerics.BigInteger]::Parse('14695981039346656037')
    foreach ($byte in [System.Text.Encoding]::UTF8.GetBytes($checkout)) {
        $low = [int]($hash % 256)
        $hash = (($hash - $low + ($low -bxor $byte)) * 1099511628211) % $mod
    }
    $root = Join-Path $base ('tuic-co-' + ([uint64]$hash).ToString('x16'))
}
New-Item -ItemType Directory -Path $root -Force | Out-Null
$root = (Resolve-Path $root).ProviderPath
# Empty template dir: test-fixture `git init`/`git clone` skip git's hooks/*.sample
# copies (see nextest-test-tmp.sh).
$gitTemplate = Join-Path $root 'git-template-empty'
New-Item -ItemType Directory -Path $gitTemplate -Force | Out-Null
# Fail closed: git never discovers a repository above the root (see
# nextest-test-tmp.sh). Append the root's parent and the host temp dir (home of
# the socket roots) once each to any inherited value.
$gitCeiling = if ($env:GIT_CEILING_DIRECTORIES) { $env:GIT_CEILING_DIRECTORIES } else { '' }
foreach ($ceilingDir in @((Split-Path -Parent $root), $hostTmp)) {
    if ($ceilingDir -and -not (($gitCeiling -split ';') -contains $ceilingDir)) {
        $gitCeiling = if ($gitCeiling) { "$gitCeiling;$ceilingDir" } else { $ceilingDir }
    }
}
$lines = @(
    "TUIC_TEST_HOST_TMPDIR=$hostTmp"
    "TUIC_TEST_TMP_ROOT=$root"
    "TMPDIR=$root"
    "TMP=$root"
    "TEMP=$root"
    "GIT_TEMPLATE_DIR=$gitTemplate"
    "GIT_CEILING_DIRECTORIES=$gitCeiling"
)
# TUIC_TEST_FAKE_HOME=1: a throwaway HOME for the test processes, as
# nextest-test-tmp.sh does. On Windows only HOME-reading tools see it (Git for
# Windows' global config, a test's own HOME override); the `dirs` crate reads
# Known Folders, not the environment, so APPDATA/USERPROFILE are left alone.
if ($env:TUIC_TEST_FAKE_HOME -eq '1') {
    $fakeHome = Join-Path $root 'home'
    New-Item -ItemType Directory -Path $fakeHome -Force | Out-Null
    $lines += "HOME=$fakeHome"
}
[System.IO.File]::AppendAllText(
    $env:NEXTEST_ENV,
    ($lines -join "`n") + "`n",
    [System.Text.UTF8Encoding]::new($false)
)
