// Artifact bookkeeping only. This module cannot run tools or contact a network.
import fs from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';

const ROOT = path.resolve(import.meta.dirname, '..');
const SOURCE_ROOTS = ['crates', 'apps/desktop', 'src', 'scripts', 'platform', 'assets', 'licenses/rust', 'licenses/frontend', 'licenses/installer'];
const SOURCE_FILES = ['Cargo.toml', 'Cargo.lock', '.gitattributes', '.cargo/config.toml', 'rust-toolchain.toml', 'verify.ps1', 'global.json', 'NuGet.config', 'LICENSE', 'THIRD_PARTY_NOTICES.txt', '.github/workflows/rust.yml', '.github/workflows/rust-portable.yml', '.github/workflows/rust-release.yml'];
const EXTENSIONS = new Set(['.rs', '.toml', '.lock', '.json', '.ts', '.tsx', '.jsx', '.css', '.html', '.svg', '.png', '.ico', '.md', '.txt', '.ps1', '.mjs', '.sh', '.py', '.swift', '.yml', '.yaml', '.config', '.manifest', '.cs', '.csproj', '.xaml', '.axaml', '.props', '.targets', '.nsi']);
const EXCLUDED = /(^|\/)(node_modules|target|dist|bin|obj|\.build|\.vite|test-results|playwright-report)(\/|$)/i;
const GENERATED = /^apps\/desktop\/src-tauri\/(gen|generated-schemas)(\/|$)/i;
const CHECKS = ['legacy', 'artifact-guards', 'distribution-guards', 'cargo-format', 'frontend-tests', 'frontend-build', 'rust-tests', 'rust-clippy'];
const CURRENT_CHECK = '.build/rust-verification.json';
const CURRENT_RELEASE = '.build/rust-release-verification.json';
const CURRENT_RELEASE_CHECK = '.build/rust-release-check-current.json';
const DLL_HASH = 'c00cbe50a845003f986d3ed0ee149c08f576d0f3cc54c5346033608185a000d5';
const ARCHIVE_HASH = '272eb97795048ccb557c85bb9dd44c9e530e1c928443114c39816a8a5f76269d';
const SDK = '.build/rust-native-0.4.1';
const EXE = '.build/rust-target/x86_64-pc-windows-msvc/release/HandyMeeting.exe';

function insist(condition, message) { if (!condition) throw new Error(message); }
function unix(value) { return value.split(path.sep).join('/'); }
function inside(root, relative) {
  const result = path.resolve(root, relative);
  insist(result.startsWith(path.resolve(root) + path.sep), 'Artifact path escapes its project directory.');
  return result;
}
async function exists(file) { try { await fs.lstat(file); return true; } catch (error) { if (error.code === 'ENOENT') return false; throw error; } }
async function regular(file) { const info = await fs.lstat(file); insist(!info.isSymbolicLink() && info.isFile(), 'Artifact must be a regular file.'); return info; }
async function sha(file) {
  await regular(file);
  const hash = crypto.createHash('sha256');
  for await (const chunk of createReadStream(file, { highWaterMark: 65536 })) hash.update(chunk);
  return hash.digest('hex');
}
async function json(file) { const info = await regular(file); insist(info.size <= 32 * 1024 * 1024, 'Artifact JSON exceeds its size limit.'); return JSON.parse(await fs.readFile(file, 'utf8')); }
async function writeJson(file, value, immutable = false) {
  await fs.mkdir(path.dirname(file), { recursive: true });
  const body = JSON.stringify(value, null, 2) + '\n';
  if (immutable) { const handle = await fs.open(file, 'wx'); try { await handle.writeFile(body); await handle.sync(); } finally { await handle.close(); } return; }
  const temporary = file + '.tmp-' + crypto.randomUUID();
  const handle = await fs.open(temporary, 'wx');
  try { await handle.writeFile(body); await handle.sync(); } finally { await handle.close(); }
  await fs.rename(temporary, file);
}
async function version(root = ROOT) {
  const config = await json(inside(root, 'apps/desktop/src-tauri/tauri.conf.json'));
  const frontend = await json(inside(root, 'apps/desktop/package.json'));
  const cargo = await fs.readFile(inside(root, 'Cargo.toml'), 'utf8');
  const match = cargo.match(/^version\s*=\s*"([^"]+)"/m);
  insist(/^\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$/.test(config.version) && match?.[1] === config.version && frontend.version === config.version, 'Cargo, frontend and Tauri versions must agree.');
  return config.version;
}
async function walk(directory) {
  if (!await exists(directory)) return [];
  const result = [];
  for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    insist(!entry.isSymbolicLink(), 'Linked files are not accepted in artifact inputs.');
    if (entry.isDirectory()) result.push(...await walk(file));
    else if (entry.isFile()) result.push(file);
  }
  return result;
}
async function sourceFingerprint(root = ROOT) {
  const names = new Map();
  for (const relative of SOURCE_FILES) if (await exists(inside(root, relative))) names.set(relative, inside(root, relative));
  async function scan(directory) {
    if (!await exists(directory)) return;
    for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      const relative = unix(path.relative(root, file));
      if (EXCLUDED.test(relative) || GENERATED.test(relative) || file.endsWith('.tsbuildinfo')) continue;
      insist(!entry.isSymbolicLink(), 'Linked source inputs require explicit review.');
      if (entry.isDirectory()) await scan(file);
      else if (entry.isFile() && EXTENSIONS.has(path.extname(file).toLowerCase())) names.set(relative, file);
    }
  }
  for (const relative of SOURCE_ROOTS) await scan(inside(root, relative));
  const files = [];
  for (const name of [...names.keys()].sort()) files.push({ path: name, sha256: await sha(names.get(name)), bytes: (await fs.stat(names.get(name))).size });
  const canonical = files.map(file => `${file.path}\t${file.sha256}\t${file.bytes}`).join('\n');
  return { sha256: crypto.createHash('sha256').update(canonical).digest('hex'), files };
}
function sameSource(receipt, source) { insist(receipt.sourceFingerprintSHA256 === source.sha256, 'Source changed since checks began. Start a new check run on stable source.'); }
function requireChecks(receipt) {
  for (const name of receipt.requiredChecks) insist(receipt.checks?.[name]?.reportedExitCode === 0, `Missing or failed separate tool check: ${name}`);
}
async function notices() {
  for (const [directory, lock, field] of [['licenses/rust/x86_64-pc-windows-msvc', 'Cargo.lock', 'CargoLockSHA256'], ['licenses/frontend', 'apps/desktop/package-lock.json', 'PackageLockSHA256']]) {
    const manifest = await json(inside(ROOT, directory + '/manifest.json'));
    insist(manifest[field] === await sha(inside(ROOT, lock)), 'License records do not match the locked dependencies.');
    for (const item of manifest.Packages) {
      insist(item.LicenseFiles?.length > 0, `License text missing: ${item.Name}`);
      for (const license of item.LicenseFiles) {
        insist(path.basename(license.File) === license.File, 'Unsafe license filename.');
        insist(await sha(inside(ROOT, directory + '/' + license.File)) === license.SHA256, 'A retained license text changed.');
      }
    }
  }
}
async function sdk() {
  const value = await json(inside(ROOT, SDK + '/SDK_PROVENANCE.json'));
  insist(value.Version === '0.4.1' && value.ArchiveSha256?.toLowerCase() === ARCHIVE_HASH, 'Prepared SDK provenance differs from the reviewed native release.');
  insist(await sha(inside(ROOT, SDK + '/bin/transcribe.dll')) === DLL_HASH, 'Prepared native DLL does not match the reviewed release.');
  insist(Array.isArray(value.Files) && value.Files.length > 0, 'Prepared SDK needs the complete reviewed DLL manifest.');
  const expected = new Map();
  for (const file of value.Files) {
    insist(typeof file.Name === 'string' && path.basename(file.Name) === file.Name && file.Name.toLowerCase().endsWith('.dll') && /^[a-f0-9]{64}$/i.test(file.SHA256), 'Unsafe SDK manifest entry.');
    insist(!expected.has(file.Name.toLowerCase()), 'Duplicated SDK manifest entry.');
    expected.set(file.Name.toLowerCase(), file.SHA256.toLowerCase());
  }
  const actual = (await fs.readdir(inside(ROOT, SDK + '/bin'), { withFileTypes: true })).filter(file => file.name.toLowerCase().endsWith('.dll'));
  insist(actual.length === expected.size, 'Additional or missing SDK DLLs require review.');
  for (const file of actual) insist(file.isFile() && await sha(inside(ROOT, SDK + '/bin/' + file.name)) === expected.get(file.name.toLowerCase()), 'A prepared SDK DLL changed.');
  return value;
}
async function beginCheck(skipLegacy) {
  const source = await sourceFingerprint();
  const run = '.build/rust-check-runs/' + new Date().toISOString().replace(/[:.]/g, '-') + '-' + crypto.randomUUID();
  const receipt = { success: false, version: await version(), startedAtUtc: new Date().toISOString(), runDirectory: run, sourceFingerprintSHA256: source.sha256, sourceFiles: source.files, requiredChecks: CHECKS.filter(name => !(skipLegacy && name === 'legacy')), checks: {}, legacyChecksExecuted: !skipLegacy, nativeModelsLoaded: false, audioDevicesOpened: false, cloudProviderCalled: false };
  await writeJson(inside(ROOT, run + '/begin.json'), receipt, true);
  await writeJson(inside(ROOT, CURRENT_CHECK), receipt);
  return { success: false, runDirectory: run, requiredChecks: receipt.requiredChecks, sourceFingerprintSHA256: source.sha256 };
}
async function recordCheck(name, rawExitCode) {
  const receipt = await json(inside(ROOT, CURRENT_CHECK));
  insist(!receipt.success, 'This verification run is already complete.');
  sameSource(receipt, await sourceFingerprint());
  insist(receipt.requiredChecks.includes(name), 'Only the fixed named tool checks may be recorded.');
  const exit = Number(rawExitCode);
  insist(typeof rawExitCode === 'string' && /^-?\d+$/.test(rawExitCode) && Number.isInteger(exit), 'An actual native exit code is required.');
  const event = { name, reportedExitCode: exit, confirmedAtUtc: new Date().toISOString(), sourceFingerprintSHA256: receipt.sourceFingerprintSHA256, execution: 'Separate standard tool invocation; this bookkeeping module executes no commands.' };
  const eventFile = receipt.runDirectory + '/' + name + '-' + crypto.randomUUID() + '.json';
  await writeJson(inside(ROOT, eventFile), event, true);
  receipt.checks[name] = { ...event, evidenceFile: eventFile };
  await writeJson(inside(ROOT, CURRENT_CHECK), receipt);
  insist(exit === 0, `Separate tool check failed: ${name} (${exit}).`);
  return { success: true, check: name, reportedExitCode: exit };
}
async function finishCheck() {
  const receipt = await json(inside(ROOT, CURRENT_CHECK));
  sameSource(receipt, await sourceFingerprint()); requireChecks(receipt); await notices(); await sdk();
  if (!receipt.success) { receipt.success = true; receipt.finishedAtUtc = new Date().toISOString(); await writeJson(inside(ROOT, receipt.runDirectory + '/verification.json'), receipt, true); await writeJson(inside(ROOT, CURRENT_CHECK), receipt); }
  return { success: true, sourceFingerprintSHA256: receipt.sourceFingerprintSHA256, legacyChecksExecuted: receipt.legacyChecksExecuted };
}
async function checkedSource() {
  const receipt = await json(inside(ROOT, CURRENT_CHECK));
  insist(receipt.success, 'Finish the separately executed tool checks first.');
  requireChecks(receipt); sameSource(receipt, await sourceFingerprint()); await notices(); await sdk();
  return receipt;
}
async function beginReleaseCheck() {
  const checked = await checkedSource();
  const output = '.build/rust-release-checks-' + crypto.randomUUID();
  const receipt = { version: await version(), sourceFingerprintSHA256: checked.sourceFingerprintSHA256, executable: EXE, executableSHA256: await sha(inside(ROOT, EXE)), startedAtUtc: new Date().toISOString(), outputDirectory: output };
  await writeJson(inside(ROOT, CURRENT_RELEASE_CHECK), receipt);
  return receipt;
}
function payloadName(name) {
  insist(!path.isAbsolute(name) && !name.includes('..') && !name.includes(':') && !/(^|[\\/])(Sessions|data|models)([\\/]|$)/i.test(name) && !/\.(wav|pcm|mp3|gguf|pfx|p12|key)$/i.test(name), 'Private or unsafe program payload path rejected.');
}
async function putSame(file, bytes) {
  await fs.mkdir(path.dirname(file), { recursive: true });
  if (await exists(file)) { insist(await sha(file) === crypto.createHash('sha256').update(bytes).digest('hex'), 'An existing version contains different bytes. Preserve it and increment the version.'); return; }
  const handle = await fs.open(file, 'wx'); try { await handle.writeFile(bytes); await handle.sync(); } finally { await handle.close(); }
}
/** Stage the checked Windows executable, libraries and notices in a new program folder.
 * Exact source and receipt checks precede copying; no executable or model is run here.
 */
async function stage() {
  const checked = await checkedSource();
  insist(checked.legacyChecksExecuted, 'Distributable candidates require the legacy checks too.');
  const pending = await json(inside(ROOT, CURRENT_RELEASE_CHECK));
  sameSource(pending, await sourceFingerprint());
  const executable = inside(ROOT, EXE);
  insist(pending.executable === EXE && await sha(executable) === pending.executableSHA256, 'Release executable changed after its check started.');
  const resultFile = inside(ROOT, pending.outputDirectory + '/release-checks.json');
  const result = await json(resultFile);
  insist(result.success && result.applicationVersion === pending.version && result.realCaptureOpened === false && result.installedMetadataQueried === false, 'The exact release executable needs successful model-free release checks.');
  insist((await fs.stat(resultFile)).mtimeMs >= Date.parse(pending.startedAtUtc), 'Release check evidence predates this executable check.');
  const currentVersion = await version();
  insist(currentVersion === pending.version, 'Release version changed.');
  const programDirectory = 'dist-rust-' + currentVersion;
  const inputs = new Map([['HandyMeeting.exe', executable], ['LICENSE', inside(ROOT, 'LICENSE')], ['THIRD_PARTY_NOTICES.txt', inside(ROOT, 'THIRD_PARTY_NOTICES.txt')], ['HANDY_STYLE_NOTICE.md', inside(ROOT, 'apps/desktop/HANDY_STYLE_NOTICE.md')]]);
  const native = await sdk();
  for (const file of native.Files) inputs.set(file.Name, inside(ROOT, SDK + '/bin/' + file.Name));
  for (const [source, destination] of [[SDK + '/licenses', 'native-licenses'], ['licenses/rust', 'licenses/rust'], ['licenses/frontend', 'licenses/frontend'], ['licenses/installer', 'licenses/installer']]) for (const file of await walk(inside(ROOT, source))) inputs.set(destination + '/' + unix(path.relative(inside(ROOT, source), file)), file);
  const files = [];
  for (const [name, file] of [...inputs].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) { payloadName(name); files.push({ path: name, sha256: await sha(file), bytes: (await fs.stat(file)).size }); }
  const manifest = { version: currentVersion, sourceFingerprintSHA256: checked.sourceFingerprintSHA256, executableSHA256: pending.executableSHA256, files };
  const manifestBytes = Buffer.from(JSON.stringify(manifest, null, 2) + '\n');
  const launcherFile = `Meeting for Handy ${currentVersion} starten.cmd`;
  const launcher = Buffer.from(`@echo off\r\nif not exist "%~dp0${programDirectory}\\HandyMeeting.exe" (\r\n  echo Bitte das vollstaendige Meeting-for-Handy-Paket entpacken.\r\n  pause\r\n  exit /b 1\r\n)\r\nstart "" "%~dp0${programDirectory}\\HandyMeeting.exe"\r\n`, 'ascii');
  // Check all conflicts before publishing a single new payload file.
  for (const file of files) { const target = inside(ROOT, programDirectory + '/' + file.path); if (await exists(target)) insist(await sha(target) === file.sha256, 'This version directory already contains different program bytes.'); }
  const manifestTarget = inside(ROOT, programDirectory + '/BUILD_MANIFEST.json');
  for (const [target, bytes] of [[manifestTarget, manifestBytes], [inside(ROOT, launcherFile), launcher]]) if (await exists(target)) insist(await sha(target) === crypto.createHash('sha256').update(bytes).digest('hex'), 'Existing manifest or Rust launcher differs; preserve it before choosing a new version.');
  for (const file of files) { const target = inside(ROOT, programDirectory + '/' + file.path); if (!await exists(target)) { await fs.mkdir(path.dirname(target), { recursive: true }); await fs.copyFile(inputs.get(file.path), target, 1); } insist(await sha(target) === file.sha256, 'Staged payload hash mismatch.'); }
  await putSame(manifestTarget, manifestBytes); await putSame(inside(ROOT, launcherFile), launcher);
  files.push({ path: 'BUILD_MANIFEST.json', sha256: await sha(manifestTarget), bytes: manifestBytes.length });
  const release = { success: true, version: currentVersion, sourceFingerprintSHA256: checked.sourceFingerprintSHA256, sourceFiles: checked.sourceFiles, executableSHA256: pending.executableSHA256, programDirectory, programFiles: files, launcherFile, launcherSHA256: await sha(inside(ROOT, launcherFile)), legacyChecksExecuted: true, releaseBinaryChecksPassed: true, releaseCheckEvidence: pending.outputDirectory + '/release-checks.json', preparedAtUtc: new Date().toISOString(), nativeModelsLoaded: false, audioDevicesOpened: false };
  sameSource(checked, await sourceFingerprint());
  await sdk();
  await writeJson(inside(ROOT, pending.outputDirectory + '/release-verification.json'), release, true);
  await writeJson(inside(ROOT, CURRENT_RELEASE), release);
  return { success: true, programDirectory, executableSHA256: release.executableSHA256, sourceFingerprintSHA256: release.sourceFingerprintSHA256 };
}
async function selfTest() {
  const directory = inside(ROOT, '.build/artifact-guards-' + crypto.randomUUID());
  await fs.mkdir(path.join(directory, 'crates/example/src'), { recursive: true });
  await fs.writeFile(path.join(directory, 'Cargo.lock'), 'synthetic-lock');
  await fs.writeFile(path.join(directory, 'crates/example/src/lib.rs'), 'pub fn fixture() {}\n');
  const workspace = path.join(directory, 'apps/desktop/src/workspace/Meeting.jsx');
  await fs.mkdir(path.dirname(workspace), { recursive: true });
  await fs.writeFile(workspace, 'export const title = <h1>Original</h1>;\n');
  const first = await sourceFingerprint(directory);
  const again = await sourceFingerprint(directory); insist(first.sha256 === again.sha256, 'Fingerprint is not deterministic.');
  await fs.writeFile(workspace, 'export const title = <h1>Changed</h1>;\n');
  insist((await sourceFingerprint(directory)).sha256 !== first.sha256, 'A changed workspace JSX file escaped the fingerprint.');
  await fs.writeFile(workspace, 'export const title = <h1>Original</h1>;\n');
  await fs.mkdir(path.join(directory, 'apps/desktop/src-tauri/gen/schemas'), { recursive: true });
  await fs.writeFile(path.join(directory, 'apps/desktop/src-tauri/gen/schemas/generated.json'), '{}');
  insist((await sourceFingerprint(directory)).sha256 === first.sha256, 'Generated schema changed the source fingerprint.');
  await fs.writeFile(path.join(directory, 'crates/example/src/new.rs'), 'pub fn additional() {}\n');
  insist((await sourceFingerprint(directory)).sha256 !== first.sha256, 'An added Rust source escaped the fingerprint.');
  let rejected = 0;
  for (const action of [() => requireChecks({ requiredChecks: ['rust-tests'], checks: {} }), () => requireChecks({ requiredChecks: ['rust-tests'], checks: { 'rust-tests': { reportedExitCode: 1 } } }), () => sameSource({ sourceFingerprintSHA256: 'different' }, first), () => payloadName('../secret.key'), () => payloadName('Sessions/private.wav')]) { try { action(); } catch { rejected++; } }
  insist(rejected === 5, 'Artifact verification guard accepted invalid evidence.');
  const retained = path.join(directory, 'retained.txt'); await putSame(retained, Buffer.from('original'));
  let overwriteRejected = false; try { await putSame(retained, Buffer.from('different')); } catch { overwriteRejected = true; }
  insist(overwriteRejected && await fs.readFile(retained, 'utf8') === 'original', 'An existing artifact was overwritten.');
  const result = { success: true, checks: 10, synthetic: true, commandsExecuted: false, networkUsed: false, outputDirectory: unix(path.relative(ROOT, directory)) };
  await writeJson(path.join(directory, 'result.json'), result, true); return result;
}

try {
  const [command, ...args] = process.argv.slice(2);
  let result;
  if (command === 'begin-check') result = await beginCheck(args.includes('--skip-legacy'));
  else if (command === 'record-check') result = await recordCheck(args[0], args[1]);
  else if (command === 'finish-check') result = await finishCheck();
  else if (command === 'require-check') { const value = await checkedSource(); result = { success: true, sourceFingerprintSHA256: value.sourceFingerprintSHA256 }; }
  else if (command === 'begin-release-check') result = await beginReleaseCheck();
  else if (command === 'stage') result = await stage();
  else if (command === 'source-fingerprint') result = await sourceFingerprint();
  else if (command === 'self-test') result = await selfTest();
  else throw new Error('Use begin-check, record-check NAME EXIT_CODE, finish-check, require-check, begin-release-check, stage, source-fingerprint or self-test.');
  console.log(JSON.stringify(result, null, 2));
} catch (error) { console.error(error.message); process.exitCode = 1; }
