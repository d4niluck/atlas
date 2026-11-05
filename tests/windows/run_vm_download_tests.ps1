param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('atlas-dl-contract-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tempRoot -ErrorAction Stop | Out-Null
try {
  foreach ($name in @('model_download','q8_model_download','vision_model_download','rag_model_download','rag_e4b_model_download','nemotron_model_download')) {
    foreach ($case in @('good','bad','existing','cancel','race')) {
      $caseRoot = Join-Path $tempRoot ($name + '-' + $case)
      New-Item -ItemType Directory -Path $caseRoot -ErrorAction Stop | Out-Null
      & $executablePath $caseRoot $name $case
      if ($LASTEXITCODE -ne 0) { throw "FAILED $name $case (exit $LASTEXITCODE)" }
      Write-Output "PASS $name $case"
    }
  }
} finally {
  Remove-Item -LiteralPath $tempRoot -Recurse -Force
}
