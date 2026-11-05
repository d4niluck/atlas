$ErrorActionPreference='Stop'
if($args.Count -ne 1){throw 'Usage: windows_deferred.ps1 PROXY_EXE'}
$exe=$args[0]
$ui=Join-Path $env:TEMP ('atlas-proxy-first-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $ui | Out-Null
Set-Content -LiteralPath (Join-Path $ui 'index.html') -Value 'fixture'
try {
 foreach($mode in @('shutdown-before-backend','port-then-shutdown')) {
  $si=[Diagnostics.ProcessStartInfo]::new();$si.FileName=$exe;$si.Arguments='"'+$ui+'" 0 0 --backend-stdin';$si.UseShellExecute=$false;$si.RedirectStandardInput=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true;$si.CreateNoWindow=$true
  $p=[Diagnostics.Process]::Start($si)
  try {
   $p.StandardInput.WriteLine('bootstrap-only-1234567890abcdefgh');$p.StandardInput.WriteLine('backend-token-1234567890abcdefghij');$p.StandardInput.Flush()
   $read=$p.StandardOutput.ReadLineAsync();if(-not $read.Wait(5000)){throw 'proxy did not report port'};$port=0;if(-not [int]::TryParse($read.Result,[ref]$port) -or $port -lt 1){throw 'invalid port'}
   if($mode -eq 'port-then-shutdown'){$p.StandardInput.WriteLine('12345');$p.StandardInput.Flush()}
   $p.StandardInput.WriteLine('shutdown');$p.StandardInput.Flush();$read=$p.StandardOutput.ReadLineAsync();if(-not $read.Wait(5000) -or $read.Result -ne 'OK'){throw 'shutdown not acknowledged'};if(-not $p.WaitForExit(5000) -or $p.ExitCode -ne 0){throw 'proxy failed to exit cleanly'}
   Write-Host ("PASS "+$mode)
  } finally {if(-not $p.HasExited){$p.Kill();$p.WaitForExit()};$p.Dispose()}
 }
} finally {Remove-Item -LiteralPath $ui -Recurse -Force}
$ui=Join-Path $env:TEMP ('atlas-proxy-drain-'+[Guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $ui | Out-Null;Set-Content (Join-Path $ui 'index.html') 'fixture'
$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$listener.Start();$backend=([Net.IPEndPoint]$listener.LocalEndpoint).Port
$si=[Diagnostics.ProcessStartInfo]::new();$si.FileName=$exe;$si.Arguments='"'+$ui+'" 0 0 --backend-stdin';$si.UseShellExecute=$false;$si.RedirectStandardInput=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true;$si.CreateNoWindow=$true;$p=[Diagnostics.Process]::Start($si)
try {
 $p.StandardInput.WriteLine('bootstrap-only-1234567890abcdefgh');$p.StandardInput.WriteLine('backend-token-1234567890abcdefghij');$p.StandardInput.Flush();$port=$p.StandardOutput.ReadLine();$p.StandardInput.WriteLine($backend);$p.StandardInput.Flush()
 $req=[Net.HttpWebRequest]::Create("http://127.0.0.1:$port/bootstrap/bootstrap-only-1234567890abcdefgh");$req.AllowAutoRedirect=$false;$res=$req.GetResponse();$cookie=$res.Headers['Set-Cookie'].Split(';')[0];$res.Close()
 $req=[Net.HttpWebRequest]::Create("http://127.0.0.1:$port/api/test");$req.Headers.Add('Cookie',$cookie);$pending=$req.GetResponseAsync();$accept=$listener.AcceptTcpClientAsync();if(-not $accept.Wait(5000)){throw 'backend request did not arrive'};$connection=$accept.Result;$stream=$connection.GetStream()
 $p.StandardInput.WriteLine('shutdown');$p.StandardInput.Flush();$reply=$p.StandardOutput.ReadLineAsync();if($reply.Wait(500)){throw 'shutdown acknowledged while request active'}
 $bytes=[Text.Encoding]::ASCII.GetBytes("HTTP/1.1 200 OK`r`nContent-Length: 2`r`nConnection: close`r`n`r`nok");$stream.Write($bytes,0,$bytes.Length);$stream.Flush();$connection.Close();if(-not $pending.Wait(5000)){throw 'pending request not completed'};$res=$pending.Result;$res.Close();if(-not $reply.Wait(5000) -or $reply.Result -ne 'OK'){throw 'drain not acknowledged'};if(-not $p.WaitForExit(5000) -or $p.ExitCode -ne 0){throw 'proxy did not exit'};Write-Host 'PASS active-request-drain'
} finally {if(-not $p.HasExited){$p.Kill();$p.WaitForExit()};$p.Dispose();$listener.Stop();Remove-Item -LiteralPath $ui -Recurse -Force}
