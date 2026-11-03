#requires -Version 7.4
param([string]$OutputDirectory, [switch]$SourceFingerprintOnly)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

# Filesystem and compression only; no tools, shell commands or models are run.
function FileHash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function ChildPath([string]$Base, [string]$Relative) {
    $target = [IO.Path]::GetFullPath((Join-Path $Base $Relative))
    $prefix = [IO.Path]::GetFullPath($Base).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $target.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Package path escapes its checked directory.' }
    return $target
}
function SourceFingerprint {
    $inputs = @{}
    foreach ($relative in @('Cargo.toml','Cargo.lock','.gitattributes','.cargo/config.toml','rust-toolchain.toml','verify.ps1','global.json','NuGet.config','LICENSE','THIRD_PARTY_NOTICES.txt','.github/workflows/rust.yml','.github/workflows/rust-portable.yml','.github/workflows/rust-release.yml')) {
        $file = Join-Path $root $relative
        if (Test-Path -LiteralPath $file -PathType Leaf) { $inputs[$relative] = Get-Item -LiteralPath $file }
    }
    foreach ($relative in @('crates','apps/desktop','src','scripts','platform','assets','licenses/rust','licenses/frontend','licenses/installer')) {
        $directory = Join-Path $root $relative
        if (-not (Test-Path -LiteralPath $directory)) { continue }
        foreach ($file in Get-ChildItem -LiteralPath $directory -File -Recurse -Force) {
            $name = [IO.Path]::GetRelativePath($root,$file.FullName).Replace('\','/')
            if ($name -match '(^|/)(node_modules|target|dist|bin|obj|\.build|\.vite|test-results|playwright-report)(/|$)' -or $file.Extension -eq '.tsbuildinfo' -or $name -match '^apps/desktop/src-tauri/(gen|generated-schemas)(/|$)') { continue }
            if ($file.Extension -in @('.rs','.toml','.lock','.json','.ts','.tsx','.jsx','.css','.html','.svg','.png','.ico','.md','.txt','.ps1','.mjs','.sh','.py','.swift','.yml','.yaml','.config','.manifest','.cs','.csproj','.xaml','.axaml','.props','.targets','.nsi')) { $inputs[$name] = $file }
        }
    }
    $names = [string[]]@($inputs.Keys); [Array]::Sort($names,[StringComparer]::Ordinal)
    $lines = foreach ($name in $names) { $file=$inputs[$name]; $name + "`t" + (FileHash $file.FullName) + "`t" + $file.Length }
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes(($lines -join "`n")))).ToLowerInvariant()
}
if ($SourceFingerprintOnly) { SourceFingerprint; return }
$receipt = Get-Content -LiteralPath (Join-Path $root '.build/rust-release-verification.json') -Raw | ConvertFrom-Json
$checks = Get-Content -LiteralPath (Join-Path $root '.build/rust-verification.json') -Raw | ConvertFrom-Json
if (-not $receipt.success -or -not $receipt.releaseBinaryChecksPassed -or -not $receipt.legacyChecksExecuted -or -not $checks.success -or $receipt.sourceFingerprintSHA256 -ne $checks.sourceFingerprintSHA256 -or $receipt.sourceFingerprintSHA256 -ne (SourceFingerprint)) { throw 'A checked release from the exact current source tree is required.' }
$version = $receipt.version
$config = Get-Content -LiteralPath (Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
if ($version -notmatch '^\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$' -or $version -ne $config.version -or $receipt.programDirectory -ne ('dist-rust-'+$version)) { throw 'Unexpected checked program version.' }
$dist = ChildPath $root $receipt.programDirectory
if ((FileHash (Join-Path $dist 'HandyMeeting.exe')) -ne $receipt.executableSHA256) { throw 'The program differs from its release-binary verification.' }
$stage = Join-Path $root ('.build/rust-package-'+[guid]::NewGuid().ToString('N'))
$app = Join-Path $stage 'HandyMeeting'
$program = Join-Path $app $receipt.programDirectory
New-Item -ItemType Directory -Path $program -Force | Out-Null
foreach ($entry in $receipt.programFiles) {
    if ([IO.Path]::IsPathRooted($entry.path) -or $entry.path.Contains('..') -or $entry.path.Contains(':') -or $entry.path -match '(^|[\\/])(Sessions|data|models)([\\/]|$)' -or [IO.Path]::GetExtension($entry.path) -in @('.wav','.pcm','.mp3','.gguf','.pfx','.p12','.key')) { throw 'Private or unsafe payload path rejected.' }
    $source = ChildPath $dist $entry.path
    if ((FileHash $source) -ne $entry.sha256) { throw ('Checked program file changed: '+$entry.path) }
    $target = ChildPath $program $entry.path
    New-Item -ItemType Directory -Path (Split-Path $target -Parent) -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination $target
    if ((FileHash $target) -ne $entry.sha256) { throw 'A payload file changed while being copied.' }
}
if ($receipt.launcherFile -ne ('Meeting for Handy ' + $version + ' starten.cmd')) { throw 'Unexpected checked launcher.' }
$launcher = Join-Path $root $receipt.launcherFile
if ((FileHash $launcher) -ne $receipt.launcherSHA256) { throw 'The checked Rust launcher changed.' }
Copy-Item -LiteralPath $launcher -Destination $app
$instructions = "Meeting for Handy $version - Rust/Tauri Windows x64`n`nExtract the full archive and open $($receipt.launcherFile). Keep the version folder together.`nInstall Handy with a compatible downloaded streaming model. Future native model formats can require a Meeting for Handy update.`nWindows WebView2 and the Microsoft Visual C++ runtime may be required. .NET is not required to run Meeting for Handy.`nUser data stays in local application data under HandyMeeting/RustBeta, outside the program folder. Existing C# sessions remain unchanged.`nNo models, recordings, settings or keys are included. This is an independent companion app, not an official Handy product. Real-device capture, device loss, full disk and a new two-hour Rust run are not yet validated. Test only with recordings that are independently backed up.`nEvidence and limits: https://github.com/Puma7/meeting-for-handy/blob/main/TEST_REPORT.md`n"
[IO.File]::WriteAllText((Join-Path $app 'START-HERE.txt'),$instructions,[Text.UTF8Encoding]::new($false))
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $root 'packages' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$zip = Join-Path $OutputDirectory ('HandyMeeting-'+$version+'-windows-x64.zip')
if (Test-Path -LiteralPath $zip) { throw 'Package already exists and was preserved. Select a new version or fresh output directory.' }
if ((SourceFingerprint) -ne $receipt.sourceFingerprintSHA256) { throw 'Source changed during package preparation. No ZIP was created.' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($stage,$zip)
$hash = FileHash $zip
[IO.File]::WriteAllText(($zip+'.sha256'),($hash+'  '+[IO.Path]::GetFileName($zip)+"`n"),[Text.UTF8Encoding]::new($false))
$result = [ordered]@{success=$true;version=$version;package=[IO.Path]::GetFileName($zip);bytes=(Get-Item -LiteralPath $zip).Length;sha256=$hash;executableSHA256=$receipt.executableSHA256;sourceFingerprintSHA256=$receipt.sourceFingerprintSHA256;onlyCheckedProgramFiles=$true;userRecordingsIncluded=$false;published=$false}
[IO.File]::WriteAllText(($zip+'.verification.json'),(($result|ConvertTo-Json)+"`n"),[Text.UTF8Encoding]::new($false))
$result|ConvertTo-Json
