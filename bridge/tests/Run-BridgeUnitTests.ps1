#Requires -Version 7
<#
.SYNOPSIS
  Multi-entry runner for bridge stub-classpath tests.

.DESCRIPTION
  Source roots:
    bridge/stubs/     shared stubs
    bridge/src/       production (all *.java; BridgeBuildInfo generated into build/gen)
    bridge/tests/     stub-classpath tests

  Entries:
    FlatJsonNestedTest / LineChannelLimitTest / TextContrastProjectIsolationTest / BridgeUnitFixtures
      - headless-compatible protocol and editor tests (default all)
    WindowIsolationTest
      - window lifecycle test; always compiled on stub classpath
      - NOT in default all (CI may be headless)
      - run explicitly with -WindowTests and/or -Entry WindowIsolationTest
      - forces -Djava.awt.headless=false; never silently skips
    NativePainterApiTest
      - NEVER compiled/run here (real D:/Rebased/lib jars only)

.PARAMETER Entry
  all | FlatJsonNestedTest | LineChannelLimitTest | TextContrastProjectIsolationTest | WindowIsolationTest

.PARAMETER WindowTests
  Include WindowIsolationTest. Required for window tests under default Entry=all.
  When window tests are selected, java is launched with -Djava.awt.headless=false.
  If the JVM is still headless, the runner fails instead of skipping.
#>
param(
  [ValidateSet('all','FlatJsonNestedTest','LineChannelLimitTest','TextContrastProjectIsolationTest','WindowIsolationTest','IslandBackgroundGateTest','IslandBordersTest','WindowSurfaceGraphicsTest')]
  [string]$Entry = 'all',
  [switch]$WindowTests,
  [string]$JdkHome = $env:JAVA_HOME
)

$ErrorActionPreference = 'Stop'
if (-not $JdkHome) { throw 'Set JAVA_HOME or pass -JdkHome pointing to JDK 21+.' }
$BridgeRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Out = Join-Path $BridgeRoot 'build/unit-test-classes'
$Gen = Join-Path $BridgeRoot 'build/gen/studio/background/rebased/bridge'
$Javac = Join-Path $JdkHome 'bin/javac.exe'
$Java = Join-Path $JdkHome 'bin/java.exe'
$WindowEntry = 'WindowIsolationTest'
$WindowSource = Join-Path $BridgeRoot "tests/studio/background/rebased/bridge/$WindowEntry.java"

if (-not (Test-Path $Javac)) { throw "javac not found: $Javac" }

if (Test-Path $Out) { Remove-Item $Out -Recurse -Force }
New-Item -ItemType Directory -Path $Out | Out-Null
New-Item -ItemType Directory -Path $Gen -Force | Out-Null

$buildInfo = Join-Path $Gen 'BridgeBuildInfo.java'
@(
  'package studio.background.rebased.bridge;',
  '',
  'final class BridgeBuildInfo {',
  '  static final String VERSION = "test";',
  '',
  '  private BridgeBuildInfo() {',
  '  }',
  '}',
  ''
) | Set-Content -Path $buildInfo -Encoding utf8NoBOM

$prod = @(Get-ChildItem (Join-Path $BridgeRoot 'src') -Recurse -Filter '*.java' | ForEach-Object FullName)
$prod += $buildInfo

# Always compile WindowIsolationTest on the stub classpath; never NativePainterApiTest.
$unitTests = @(Get-ChildItem (Join-Path $BridgeRoot 'tests/studio/background/rebased/bridge') -Filter '*.java' |
  Where-Object { $_.Name -ne 'NativePainterApiTest.java' } |
  ForEach-Object FullName)

$stubFiles = @(Get-ChildItem (Join-Path $BridgeRoot 'stubs') -Recurse -Filter '*.java' | ForEach-Object FullName)
$sources = @($stubFiles + $prod + $unitTests)

Write-Host "Compiling $($sources.Count) sources -> $Out"
& $Javac --release 21 -encoding UTF-8 -proc:none -Xlint:all -Xlint:-options -d $Out @sources
if ($LASTEXITCODE -ne 0) { throw "javac failed: $LASTEXITCODE" }

$wantWindow = $WindowTests -or ($Entry -eq $WindowEntry)
if ($wantWindow -and -not (Test-Path $WindowSource)) {
  throw "$WindowEntry was requested but source is missing: $WindowSource"
}

if ($Entry -eq 'all') {
  $entries = @('FlatJsonNestedTest','LineChannelLimitTest','TextContrastProjectIsolationTest','IslandBackgroundGateTest','IslandBordersTest','WindowSurfaceGraphicsTest')
  if ($wantWindow) { $entries += $WindowEntry }
} elseif ($Entry -eq $WindowEntry) {
  $entries = @($WindowEntry)
} else {
  $entries = @($Entry)
}

foreach ($name in $entries) {
  $fqcn = "studio.background.rebased.bridge.$name"
  Write-Host "==> java $fqcn"
  if ($name -eq $WindowEntry) {
    & $Java '-Djava.awt.headless=false' -cp $Out $fqcn
  } else {
    & $Java -cp $Out $fqcn
  }
  if ($LASTEXITCODE -ne 0) { throw "$name failed: $LASTEXITCODE" }
}

Write-Host "Bridge unit tests passed: $($entries -join ', ')"
