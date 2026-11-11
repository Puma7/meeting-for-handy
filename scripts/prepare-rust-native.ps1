$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$archive = Join-Path $root '.build/native-download/transcribe-0.4.1-windows.tar.gz'
$archiveHash = '272EB97795048CCB557C85BB9DD44C9E530E1C928443114C39816A8A5F76269D'
if (-not (Test-Path -LiteralPath $archive)) {
    New-Item -ItemType Directory -Path (Split-Path $archive -Parent) -Force | Out-Null
    $download = $archive + '.' + [guid]::NewGuid().ToString('N') + '.partial'
    try {
        Invoke-WebRequest -Uri 'https://github.com/handy-computer/transcribe.cpp/releases/download/v0.4.1/transcribe-native-0.4.1-windows-x86_64-cpu-vulkan.tar.gz' -OutFile $download -MaximumRetryCount 2 -RetryIntervalSec 5
        if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ne $archiveHash) { throw 'The downloaded native archive checksum does not match.' }
        [IO.File]::Move($download, $archive)
    } finally {
        if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download -Force }
    }
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $archiveHash) { throw 'The native archive checksum does not match. Existing programs remain untouched.' }
$entries = & tar -tzf $archive
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect native archive.' }
foreach ($entry in $entries) { if (-not $entry.StartsWith('transcribe-native-windows-x86_64-cpu-vulkan/') -or $entry -match '(^|/)\.\.(/|$)' -or $entry.Contains('\')) { throw 'Unexpected native archive path.' } }
$source = Join-Path $root ('.build/rust-native-extract-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $source | Out-Null
& tar -xzf $archive -C $source --strip-components=1
if ($LASTEXITCODE -ne 0) { throw 'Cannot extract verified native archive.' }
$sdk = Join-Path $root '.build/rust-native-0.4.1'
$libDirectory = Join-Path $sdk 'lib'
$binDirectory = Join-Path $sdk 'bin'
New-Item -ItemType Directory -Path $libDirectory,$binDirectory,(Join-Path $sdk 'include') -Force | Out-Null
foreach ($file in Get-ChildItem -LiteralPath $source -File -Filter '*.dll') {
    $destination=Join-Path $binDirectory $file.Name
    if (Test-Path -LiteralPath $destination) {
        if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash) { throw 'Existing Rust SDK differs from the verified archive. Preserve it and use a separate SDK directory.' }
    } else { Copy-Item -LiteralPath $file.FullName -Destination $destination }
}
$licenseSource=Join-Path $source 'licenses'
foreach ($license in Get-ChildItem -LiteralPath $licenseSource -File -Recurse) {
    $destination=Join-Path (Join-Path $sdk 'licenses') ([IO.Path]::GetRelativePath($licenseSource,$license.FullName))
    New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
    if (Test-Path -LiteralPath $destination) {
        if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $license.FullName -Algorithm SHA256).Hash) { throw 'Existing SDK license differs from the verified archive; preserve it for review.' }
    } else { Copy-Item -LiteralPath $license.FullName -Destination $destination }
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio C++ Build Tools are required.' }
$visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $visualStudio) { throw 'Visual Studio C++ Build Tools are required.' }
$tools = Get-ChildItem -LiteralPath (Join-Path $visualStudio 'VC/Tools/MSVC') -Directory | Sort-Object Name -Descending | Select-Object -First 1
$toolsBin = Join-Path $tools.FullName 'bin/Hostx64/x64'
$exports = & (Join-Path $toolsBin 'dumpbin.exe') /nologo /exports (Join-Path $binDirectory 'transcribe.dll')
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect the pinned official transcribe DLL.' }
$names = @($exports | ForEach-Object { if ($_ -match '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]+\s+(transcribe_[A-Za-z0-9_]+)\s*$') { $Matches[1] } })
if ($names.Count -lt 30 -or 'transcribe_version' -notin $names -or 'transcribe_stream_feed' -notin $names) { throw 'Official native export list is incomplete.' }
$definition = Join-Path $libDirectory 'transcribe.def'
[IO.File]::WriteAllText($definition,("LIBRARY transcribe.dll`nEXPORTS`n"+($names -join "`n")+"`n"),[Text.UTF8Encoding]::new($false))
& (Join-Path $toolsBin 'lib.exe') /nologo "/def:$definition" /machine:x64 "/out:$(Join-Path $libDirectory 'transcribe.lib')"
if ($LASTEXITCODE -ne 0) { throw 'Cannot create native import library.' }
@{shared=$true;lib_dir='lib';include_dir='include';libraries=@('transcribe');system_libs=@();library_paths=@();frameworks=@();link_flags=@();module_dir='bin'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $libDirectory 'transcribe-link.json') -Encoding utf8NoBOM
$verifiedFiles=@(Get-ChildItem -LiteralPath $source -File -Filter '*.dll' | Sort-Object Name | ForEach-Object { @{Name=$_.Name;SHA256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()} })
@{Version='0.4.1';Source='Official hash-pinned transcribe.cpp Windows release';ArchiveSha256=$archiveHash;Exports=$names.Count;DllSha256=(Get-FileHash (Join-Path $binDirectory 'transcribe.dll') -Algorithm SHA256).Hash;Files=$verifiedFiles} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $sdk 'SDK_PROVENANCE.json') -Encoding utf8NoBOM
Write-Output $sdk
