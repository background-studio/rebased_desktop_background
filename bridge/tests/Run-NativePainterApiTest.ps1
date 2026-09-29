#Requires -Version 7
param(
  [Parameter(Mandatory = $true)][string]$RebasedHome,
  [string]$JdkHome = $env:JAVA_HOME
)
$ErrorActionPreference = 'Stop'
if (-not $JdkHome) { $JdkHome = Join-Path $RebasedHome 'jbr' }
$out = Join-Path ([System.IO.Path]::GetTempPath()) ('rebased-native-api-' + [guid]::NewGuid())
New-Item $out -ItemType Directory | Out-Null
try {
  $source = Join-Path $PSScriptRoot 'studio/background/rebased/bridge/NativePainterApiTest.java'
  $graphics = Join-Path $PSScriptRoot '../src/studio/background/rebased/bridge/WindowPainterGraphics.java'
  & (Join-Path $JdkHome 'bin/javac.exe') --release 21 -encoding UTF-8 -d $out $source $graphics
  if ($LASTEXITCODE -ne 0) { throw 'Native API test compilation failed' }
  $classpath = $out + [System.IO.Path]::PathSeparator + (Join-Path $RebasedHome 'lib/*')
  & (Join-Path $JdkHome 'bin/java.exe') -cp $classpath studio.background.rebased.bridge.NativePainterApiTest
  if ($LASTEXITCODE -ne 0) { throw 'Rebased window painter API verification failed' }
}
finally { Remove-Item $out -Recurse -Force }
