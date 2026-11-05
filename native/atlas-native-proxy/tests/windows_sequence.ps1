$ErrorActionPreference='Stop'
if($args.Count -ne 2){throw 'Usage: windows_sequence.ps1 PROXY_EXE JOB_TEST_EXE'}
$proxy=$args[0];$test=$args[1]
$dir=Join-Path $env:TEMP ('atlas-sequence-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $dir | Out-Null
Set-Content -LiteralPath (Join-Path $dir 'index.html') -Value 'fixture'
try {
 $env:ATLAS_TEST_PROXY=$proxy;$env:ATLAS_TEST_UI=$dir
 & $test 'sequence_tests::' --ignored --test-threads=1 --nocapture
 if($LASTEXITCODE -ne 0){throw "sequence test failed: $LASTEXITCODE"}
} finally {Remove-Item Env:ATLAS_TEST_PROXY -ErrorAction SilentlyContinue;Remove-Item Env:ATLAS_TEST_UI -ErrorAction SilentlyContinue;Remove-Item -LiteralPath $dir -Recurse -Force}
