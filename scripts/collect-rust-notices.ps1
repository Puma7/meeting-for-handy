param(
    [string]$Target = 'x86_64-pc-windows-msvc',
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if ($Target -notmatch '^[A-Za-z0-9_-]+$') { throw 'Invalid target triple.' }
$rustOutput = Join-Path $root ('licenses/rust/' + $Target)
$frontendOutput = Join-Path $root 'licenses/frontend'
New-Item -ItemType Directory -Path $rustOutput, $frontendOutput -Force | Out-Null

# These packages omit a standalone license from their published crate. The Git
# current-source revisions come from that exact crate's .cargo_vcs_info.json;
# separately retained historical notices are explicitly labeled below. No text
# is synthesized from SPDX identifiers. selectors references Mozilla's MPL.
$fallback = @{
    'tauri-plugin@2.6.3' = @(
        @{ Original = 'LICENSE_MIT'; Url = 'https://raw.githubusercontent.com/tauri-apps/tauri/6f6ab1207bb3923c2721fbc67d2fdb1c8deb0c7a/LICENSE_MIT'; Note = 'Original MIT option from this crate archive''s exact .cargo_vcs_info.json revision; the build-helper crate omits a standalone notice.' }
    )
    'alloc-stdlib@0.2.2' = @(
        @{ Original = 'LICENSE'; Url = 'https://raw.githubusercontent.com/dropbox/rust-alloc-no-stdlib/6032b6a9b20e03737135c55a0270ccffcc1438ef/LICENSE' }
    )
    'clipboard-win@5.4.3' = @(
        @{ Original = 'LICENSE'; Url = 'https://raw.githubusercontent.com/DoumanAsh/clipboard-win/ed27008ae5d1f10cfbb266de039f0fb35fd05a56/LICENSE' }
    )
    'dlopen2@0.8.2' = @(
        @{ Original = 'LICENSE'; Url = 'https://raw.githubusercontent.com/OpenByteDev/dlopen2/cc80e4a0a90d499b677fdf7743699b4b3a43a989/LICENSE' }
    )
    'dlopen2_derive@0.4.3' = @(
        @{ Original = 'LICENSE'; Url = 'https://raw.githubusercontent.com/OpenByteDev/dlopen2/cc80e4a0a90d499b677fdf7743699b4b3a43a989/LICENSE' }
    )
    'libappindicator-sys@0.9.0' = @(
        @{ Original = 'LICENSE-MIT'; Url = 'https://raw.githubusercontent.com/tauri-apps/libappindicator-rs/eafd1e3682a1247f595410266091e9684021cb6f/LICENSE-MIT' },
        @{ Original = 'LICENSE-APACHE'; Url = 'https://raw.githubusercontent.com/tauri-apps/libappindicator-rs/eafd1e3682a1247f595410266091e9684021cb6f/LICENSE-APACHE' }
    )
    'selectors@0.36.1' = @(
        @{ Original = 'MPL-2.0.txt'; Url = 'https://www.mozilla.org/media/MPL/2.0/index.txt' }
    )
}
foreach ($name in @('unic-char-property', 'unic-char-range', 'unic-common', 'unic-ucd-ident', 'unic-ucd-version')) {
    $revision = if ($name -eq 'unic-ucd-ident') { '8a6ce83063d90b91ae2ce59eddb803edd393fca9' } else { '5878605364af97a3358368a6eaef02104af2e016' }
    $fallback[$name + '@0.9.0'] = @(
        @{ Original = 'LICENSE-MIT'; Url = "https://raw.githubusercontent.com/open-i18n/rust-unic/$revision/LICENSE-MIT" },
        @{ Original = 'LICENSE-APACHE'; Url = "https://raw.githubusercontent.com/open-i18n/rust-unic/$revision/LICENSE-APACHE" },
        @{ Original = 'COPYRIGHT.md'; Url = "https://raw.githubusercontent.com/open-i18n/rust-unic/$revision/COPYRIGHT.md" }
    )
}
# The current objc2 LICENSE.md explicitly permits MIT for all these crates.
# Preserve that exact-revision explanation, including its Apple SDK boundary,
# alongside the original MIT notice retained from before upstream replaced it
# in cfb199226661ec6e4939fcd7cd7531b3c5453076. No copyright text is synthesized.
$objcRevisions = @{
    'block2@0.6.2' = 'b4167b582b2f75f9a1be75495c41b765344fd03c'
    'dispatch2@0.3.0' = 'e68410e22a2d29d5f37a0b4c1b7e8e194808eade'
    'objc2@0.6.4' = '8852b424193ca41602281b3d7540d7c8ed51e49a'
    'objc2-encode@4.1.0' = '8d214f5477365ffcbcbb7de058c86ed9a518efb7'
    'objc2-exception-helper@0.1.1' = '8d214f5477365ffcbcbb7de058c86ed9a518efb7'
}
foreach ($name in @('app-kit', 'core-foundation', 'core-graphics', 'foundation', 'io-surface', 'web-kit')) {
    $objcRevisions["objc2-$name@0.3.2"] = '7b1abfd750a2cacaea71d6a56ecfb83cb7de560b'
}
foreach ($key in $objcRevisions.Keys) {
    $revision = $objcRevisions[$key]
    $fallback[$key] = @(
        @{ Original = 'LICENSE.md'; Url = "https://raw.githubusercontent.com/madsmtm/objc2/$revision/LICENSE.md" },
        @{ Original = 'LICENSE.txt'; Url = 'https://raw.githubusercontent.com/madsmtm/objc2/757cd841d0293341dbefd9b99d7548dec244ffde/LICENSE.txt'; Note = 'Historical upstream MIT notice with original copyright; retained alongside this package revision''s LICENSE.md. The current notice expressly permits the MIT option. Replaced upstream in cfb199226661ec6e4939fcd7cd7531b3c5453076; not claimed to be supplied by this crate archive.' }
    )
}
foreach ($version in @('webview2-com@0.38.2', 'webview2-com-macros@0.8.1', 'webview2-com-sys@0.38.2')) {
    $fallback[$version] = @(
        @{ Original = 'LICENSE'; Url = 'https://raw.githubusercontent.com/wravery/webview2-rs/b74dc5e2b394044bea5191052868ce7a106c202c/LICENSE' }
    )
}
$remoteHashes = @{
    'https://raw.githubusercontent.com/tauri-apps/tauri/6f6ab1207bb3923c2721fbc67d2fdb1c8deb0c7a/LICENSE_MIT' = '9dd42ea92cff2ede5cd477cbfcce051b2d0115c0ac7f368ee88cb545055dff1d'
    'https://raw.githubusercontent.com/OpenByteDev/dlopen2/cc80e4a0a90d499b677fdf7743699b4b3a43a989/LICENSE' = '39fa265207450e77c62e90c5594a06c085b655d8374c7ced4bf7894b6bd95dd2'
    'https://raw.githubusercontent.com/dropbox/rust-alloc-no-stdlib/6032b6a9b20e03737135c55a0270ccffcc1438ef/LICENSE' = 'c0c56f26d9c051cac4d200c34c84e7ae9aaa853e01a982a1df08b09931e518ae'
    'https://raw.githubusercontent.com/DoumanAsh/clipboard-win/ed27008ae5d1f10cfbb266de039f0fb35fd05a56/LICENSE' = 'c9bff75738922193e67fa726fa225535870d2aa1059f91452c411736284ad566'
    'https://www.mozilla.org/media/MPL/2.0/index.txt' = '3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04'
    'https://raw.githubusercontent.com/open-i18n/rust-unic/5878605364af97a3358368a6eaef02104af2e016/LICENSE-MIT' = '23f18e03dc49df91622fe2a76176497404e46ced8a715d9d2b67a7446571cca3'
    'https://raw.githubusercontent.com/open-i18n/rust-unic/5878605364af97a3358368a6eaef02104af2e016/LICENSE-APACHE' = 'a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2'
    'https://raw.githubusercontent.com/open-i18n/rust-unic/5878605364af97a3358368a6eaef02104af2e016/COPYRIGHT.md' = 'f5c342c49f3ac804f3e8e7bb62a8040a44c50d47bb36902b1abd13f66a1adf8b'
    'https://raw.githubusercontent.com/wravery/webview2-rs/b74dc5e2b394044bea5191052868ce7a106c202c/LICENSE' = '0dcf41516e608bbcb6cdc5229feb7b86fe4a643b85e7df251133c93408fdac73'
    'https://raw.githubusercontent.com/tauri-apps/libappindicator-rs/eafd1e3682a1247f595410266091e9684021cb6f/LICENSE-MIT' = 'eb227437252b2a7a9c1fc342c93ade1f3d7ce38cc6dd754f613db07d53ceff0b'
    'https://raw.githubusercontent.com/tauri-apps/libappindicator-rs/eafd1e3682a1247f595410266091e9684021cb6f/LICENSE-APACHE' = 'a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2'
    'https://raw.githubusercontent.com/open-i18n/rust-unic/8a6ce83063d90b91ae2ce59eddb803edd393fca9/LICENSE-MIT' = '23f18e03dc49df91622fe2a76176497404e46ced8a715d9d2b67a7446571cca3'
    'https://raw.githubusercontent.com/open-i18n/rust-unic/8a6ce83063d90b91ae2ce59eddb803edd393fca9/LICENSE-APACHE' = 'a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2'
    'https://raw.githubusercontent.com/open-i18n/rust-unic/8a6ce83063d90b91ae2ce59eddb803edd393fca9/COPYRIGHT.md' = 'f5c342c49f3ac804f3e8e7bb62a8040a44c50d47bb36902b1abd13f66a1adf8b'
    'https://raw.githubusercontent.com/madsmtm/objc2/757cd841d0293341dbefd9b99d7548dec244ffde/LICENSE.txt' = 'e353f37b12aefbb9f9b29490e837cfee05d9bda70804b3562839a3285c1df1e5'
}
foreach ($revision in ($objcRevisions.Values | Sort-Object -Unique)) {
    $remoteHashes["https://raw.githubusercontent.com/madsmtm/objc2/$revision/LICENSE.md"] = '7f976f7e9cb2d87df7230606feb932c3f21ac0e664045a775b600046ff850c54'
}
$remoteCache = @{}
function Hash-Bytes([byte[]]$Bytes) { return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant() }
function Save-License([string]$Output, [string]$Name, [string]$Version, [string]$Original, [byte[]]$Bytes) {
    if ($Bytes.Length -eq 0 -or $Bytes.Length -gt 2000000) { throw ('Invalid license size: ' + $Name) }
    $filename = (($Name + '-' + $Version + '-' + $Original) -replace '[^A-Za-z0-9._-]', '_')
    if (-not $filename.EndsWith('.txt', [StringComparison]::OrdinalIgnoreCase)) { $filename += '.txt' }
    $path = Join-Path $Output $filename
    $hash = Hash-Bytes $Bytes
    if (Test-Path -LiteralPath $path) {
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash) {
            throw ('Existing license differs; preserved for review: ' + $filename)
        }
    } else {
        $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        try { $stream.Write($Bytes); $stream.Flush($true) } finally { $stream.Dispose() }
    }
    return [ordered]@{ OriginalFilename = $Original; File = $filename; SHA256 = $hash }
}
function Remote-License([string]$Url, [string]$Existing) {
    if ($remoteCache.ContainsKey($Url)) { return ,$remoteCache[$Url] }
    if ($Offline) {
        if (-not (Test-Path -LiteralPath $Existing)) { throw 'A referenced upstream license is missing; rerun without -Offline.' }
        $bytes = [IO.File]::ReadAllBytes($Existing)
    } else {
        $uri = [Uri]$Url
        if ($uri.Scheme -ne 'https' -or $uri.Host -notin @('raw.githubusercontent.com', 'www.mozilla.org')) { throw 'Unapproved license host.' }
        $response = Invoke-WebRequest -Uri $uri -TimeoutSec 45 -MaximumRedirection 0
        $memory = [IO.MemoryStream]::new()
        try { $response.RawContentStream.CopyTo($memory); $bytes = $memory.ToArray() } finally { $memory.Dispose() }
    }
    if (-not $remoteHashes.ContainsKey($Url) -or (Hash-Bytes $bytes) -ne $remoteHashes[$Url]) { throw 'Upstream license checksum differs from the reviewed text; preserve existing notices and review.' }
    $remoteCache[$Url] = $bytes
    return ,$bytes
}
function Package-Repository($Value) {
    $url = if ($Value -is [string]) { $Value } elseif ($null -ne $Value) { [string]$Value.url } else { '' }
    $url = $url -replace '^git\+', '' -replace '^git://github.com/', 'https://github.com/'
    if ($url -match '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { $url = 'https://github.com/' + $url }
    if ($url -and ([Uri]$url).UserInfo) { throw 'Repository URL unexpectedly contains credentials.' }
    return $url
}
function Write-Manifest([string]$Path, $Value) {
    $text = $Value | ConvertTo-Json -Depth 12
    if ($text -match '(?i)[A-Z]:\\|/Users/|/home/|github_pat_|ghp_[A-Za-z0-9]{20,}|sk-[A-Za-z0-9]{24,}') { throw 'Private location or possible credential rejected from generated manifest.' }
    [IO.File]::WriteAllText($Path, $text + "`n", [Text.UTF8Encoding]::new($false))
}

Push-Location $root
try {
    $arguments = @('metadata', '--locked', '--filter-platform', $Target, '--format-version', '1')
    if ($Offline) { $arguments += '--offline' }
    $metadataText = & cargo @arguments | Out-String
    if ($LASTEXITCODE -ne 0) { throw 'Locked Cargo metadata failed; no dependency update was attempted.' }
    $metadata = $metadataText | ConvertFrom-Json
    $active = @{}
    foreach ($node in $metadata.resolve.nodes) { $active[$node.id] = $true }
    $packages = @($metadata.packages | Where-Object { $_.source -and $active.ContainsKey($_.id) } | Sort-Object name, version)
    $rustManifest = foreach ($package in $packages) {
        $folder = Split-Path $package.manifest_path -Parent
        $provided = @(Get-ChildItem -LiteralPath $folder -File -Force | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)([._-].*)?$' | Sort-Object Name)
        if ($package.license_file) {
            $declared = [IO.Path]::GetFullPath((Join-Path $folder $package.license_file))
            $prefix = [IO.Path]::GetFullPath($folder).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
            if (-not $declared.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Declared license lies outside its package; manual review required.' }
            if (-not (Test-Path -LiteralPath $declared -PathType Leaf)) { throw ('Declared license missing: ' + $package.name) }
            $provided += Get-Item -LiteralPath $declared
            $provided = @($provided | Sort-Object FullName -Unique)
        }
        $licenseFiles = @()
        $checksumPath = Join-Path $folder '.cargo-checksum.json'
        $checksums = if (Test-Path -LiteralPath $checksumPath) { Get-Content -LiteralPath $checksumPath -Raw | ConvertFrom-Json -AsHashtable } else { $null }
        foreach ($file in $provided) {
            $relative = [IO.Path]::GetRelativePath($folder, $file.FullName).Replace('\', '/')
            $bytes = [IO.File]::ReadAllBytes($file.FullName)
            # The gtk-rs Git tree links each crate's notices to the repository root.
            # On Windows these links may check out as short text files.
            if ($package.source -eq 'git+https://github.com/gtk-rs/gtk-rs-core?rev=ea720152f28e293ef4362ee844ee5cc499f32d2a#ea720152f28e293ef4362ee844ee5cc499f32d2a') {
                $noticeHashes = @{
                    LICENSE = '8cf56d10131ce201cf69ab74b111d3ebac1acca3833d7efb39ae357224b70edb'
                    COPYRIGHT = 'dae402989de65164815b7e2b6bc2b9576285434c3785934c8b6ece0fa055960d'
                }
                $gtkRoot = & git -C $folder rev-parse --show-toplevel
                if ($LASTEXITCODE -ne 0 -or !$noticeHashes.ContainsKey($relative)) { throw 'Unrecognized gtk-rs license source.' }
                $bytes = [IO.File]::ReadAllBytes((Join-Path $gtkRoot $relative))
                if ((Hash-Bytes $bytes) -ne $noticeHashes[$relative]) { throw 'The gtk-rs license differs from the pinned notice.' }
            }
            if ($checksums -and $checksums.files.ContainsKey($relative) -and (Hash-Bytes $bytes) -ne $checksums.files[$relative]) { throw ('Crate license checksum mismatch: ' + $package.name) }
            $entry = Save-License $rustOutput $package.name $package.version $relative $bytes
            $entry.Source = 'provided-crate-file'
            $licenseFiles += $entry
        }
        $key = $package.name + '@' + $package.version
        if ($provided.Count -eq 0) {
            if (-not $fallback.ContainsKey($key)) { throw ('No supplied or reviewed upstream license: ' + $key) }
            foreach ($remote in $fallback[$key]) {
                $filename = (($package.name + '-' + $package.version + '-' + $remote.Original) -replace '[^A-Za-z0-9._-]', '_')
                if (-not $filename.EndsWith('.txt', [StringComparison]::OrdinalIgnoreCase)) { $filename += '.txt' }
                $bytes = Remote-License $remote.Url (Join-Path $rustOutput $filename)
                $entry = Save-License $rustOutput $package.name $package.version $remote.Original $bytes
                $entry.Source = 'upstream-license'; $entry.SourceURL = $remote.Url
                if ($remote.Note) { $entry.SourceNote = $remote.Note }
                $licenseFiles += $entry
            }
        }
        if ($package.name -eq 'selectors') {
            $source = [IO.File]::ReadAllText((Join-Path $folder 'lib.rs'))
            $notice = [regex]::Match($source, '^/\*[\s\S]*?\*/').Value
            if (-not $notice.Contains('https://mozilla.org/MPL/2.0/')) { throw 'Expected original selectors license notice missing.' }
            $entry = Save-License $rustOutput $package.name $package.version 'lib.rs-license-notice' ([Text.Encoding]::UTF8.GetBytes($notice))
            $entry.Source = 'provided-source-notice'; $licenseFiles += $entry
        }
        [ordered]@{
            Name = $package.name; Version = $package.version; SPDX = $package.license
            Repository = Package-Repository $package.repository
            Source = $package.source
            SourceArchive = if ($package.source.StartsWith('registry+')) { 'https://crates.io/api/v1/crates/' + $package.name + '/' + $package.version + '/download' } else { $package.source }
            LicenseFiles = $licenseFiles
        }
    }
    $cargoHash = (Get-FileHash -LiteralPath Cargo.lock -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Manifest (Join-Path $rustOutput 'manifest.json') ([ordered]@{ Target = $Target; CargoLockSHA256 = $cargoHash; Scope = 'Active Cargo resolve nodes, including build dependencies, filtered to this target; excludes workspace packages.'; PackageCount = $packages.Count; Packages = @($rustManifest) })

    $frontendRoot = Join-Path $root 'apps/desktop'
    $lockPath = Join-Path $frontendRoot 'package-lock.json'
    $lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json -AsHashtable
    $production = @($lock.packages.GetEnumerator() | Where-Object { $_.Key -and -not $_.Value.dev -and -not $_.Value.devOptional } | Sort-Object Key)
    $frontendManifest = foreach ($package in $production) {
        $folder = Join-Path $frontendRoot $package.Key
        $meta = Get-Content -LiteralPath (Join-Path $folder 'package.json') -Raw | ConvertFrom-Json
        if ($meta.version -ne $package.Value.version) { throw 'Installed frontend package differs from the lock; run npm ci first.' }
        $provided = @(Get-ChildItem -LiteralPath $folder -File | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)([._-].*)?$' | Sort-Object Name)
        if (-not $provided.Count) { throw ('Missing frontend license: ' + $meta.name) }
        $licenseFiles = @(foreach ($file in $provided) { Save-License $frontendOutput $meta.name $meta.version $file.Name ([IO.File]::ReadAllBytes($file.FullName)) })
        [ordered]@{ Name = $meta.name; Version = $meta.version; SPDX = $meta.license; Repository = Package-Repository $meta.repository; LicenseFiles = $licenseFiles }
    }
    $handyNotice = Join-Path $frontendRoot 'HANDY_STYLE_NOTICE.md'
    $handyFile = Save-License $frontendOutput 'Handy-palette' '73c095857bc4eadff097f64767c40ad129cc372d' 'HANDY_STYLE_NOTICE.md' ([IO.File]::ReadAllBytes($handyNotice))
    $frontendManifest += [ordered]@{ Name = 'Handy shared frontend palette'; Version = '73c095857bc4eadff097f64767c40ad129cc372d'; SPDX = 'MIT'; Repository = 'https://github.com/cjpais/Handy'; LicenseFiles = @($handyFile) }
    Write-Manifest (Join-Path $frontendOutput 'manifest.json') ([ordered]@{ PackageLockSHA256 = (Get-FileHash -LiteralPath $lockPath -Algorithm SHA256).Hash.ToLowerInvariant(); Scope = 'Locked production JavaScript packages and reused Handy palette; development tools are excluded from the shipped bundle notices.'; PackageCount = $production.Count; Packages = @($frontendManifest) })
    $applicationVersion = @($metadata.packages | Where-Object name -EQ 'handy-meeting-desktop')[0].version
    if (-not $applicationVersion) { throw 'Application version missing from locked metadata.' }
    $addition = @"
Meeting for Handy $applicationVersion - additional third-party notices

Rust dependency records and verbatim license texts: licenses/rust/$Target/.
Frontend production dependency records and verbatim license texts: licenses/frontend/.
The manifests identify exact package versions, declared licenses, original filenames,
source repositories and SHA-256 values. The Rust list includes active build tools as
well as application dependencies; it is not a claim that every crate is linked at runtime.

transcribe-cpp and transcribe-cpp-sys 0.4.1: MIT.
wasapi 0.26.0: MIT. hound 3.5.1: Apache-2.0.
Tauri crates retain their respective supplied Apache-2.0/MIT notices.
Handy's shared frontend palette: MIT, Copyright (c) 2025 CJ Pais.
The unchanged native transcribe.cpp runtime retains its own license directory.
No model weights are included. Model licenses are separate.

selectors 0.36.1 declares MPL-2.0. Its original source notice and Mozilla's
canonical license text are included. Original source is available at:
https://crates.io/crates/selectors/0.36.1
https://github.com/servo/stylo/tree/635e1a19d02960588a00e189bd4bd5bdb150ec3d/selectors
This project does not relicense third-party code under its own MIT license.
"@
    [IO.File]::WriteAllText((Join-Path $rustOutput 'ADDITIONAL_NOTICES.txt'), $addition + "`n", [Text.UTF8Encoding]::new($false))
    [ordered]@{ Target = $Target; RustPackages = $packages.Count; FrontendPackages = $production.Count; HandyPaletteNotice = $true; CargoLockSHA256 = $cargoHash; DependencyUpdates = $false } | ConvertTo-Json
} finally { Pop-Location }
