#!/usr/bin/env python3
"""Real Handy catalog + native CPU recognition of generated speech, never user audio.

CI downloads only the public assets pinned in asr-reference-assets.json into a new
private directory. Only checks.json is suitable for upload; all other files stay
local to the runner. --local-* reuses explicit installed assets without downloads.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import subprocess
import tarfile
import time
import urllib.request
import wave

MANIFEST = Path(__file__).with_name("asr-reference-assets.json")
SOURCES = ("system", "microphone")


def require(condition, message):
    """Reject a missing proof instead of producing a successful verdict."""
    if not condition:
        raise ValueError(message)


def digest(path):
    """Hash a regular input incrementally without retaining model bytes in RAM."""
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def verify_asset(path, asset):
    """Require both the reviewed byte length and SHA-256 before using an asset."""
    require(path.is_file() and not path.is_symlink(), "Asset must be a regular file")
    require(path.stat().st_size == asset["bytes"], "Pinned asset length differs")
    require(digest(path) == asset["sha256"], "Pinned asset digest differs")


def download(url, path, asset):
    """Download a pinned HTTPS asset exclusively, bounded by size and elapsed time."""
    require(url.startswith("https://"), "Asset download requires HTTPS")
    path.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation preserves failed/partial evidence, never trusts a cache.
    started = time.monotonic()
    count = 0
    with path.open("xb") as target, urllib.request.urlopen(url, timeout=60) as response:
        while True:
            require(time.monotonic() - started < 240, "Asset download deadline exceeded")
            block = response.read(1024 * 1024)
            if not block:
                break
            count += len(block)
            require(count <= asset["bytes"], "Asset exceeds pinned length")
            target.write(block)
    verify_asset(path, asset)


def extract_app(archive, root):
    """Allow app/framework links only when they stay inside this fresh bundle."""
    root.mkdir()
    with tarfile.open(archive) as bundle:
        members = bundle.getmembers()
        require(len(members) <= 20000, "Handy bundle has too many entries")
        require(sum(m.size for m in members) <= 512 * 1024 * 1024, "Handy bundle too large")
        seen = set()
        for member in members:
            name = PurePosixPath(member.name)
            require(not name.is_absolute() and ".." not in name.parts and "\\" not in member.name,
                    "Unsafe bundle entry")
            require(member.name not in seen, "Duplicate bundle entry")
            seen.add(member.name)
            target = root.joinpath(*name.parts)
            require(target.resolve().is_relative_to(root.resolve()), "Bundle entry escapes root")
            require(member.isdir() or member.isfile() or member.issym(), "Unsupported bundle entry")
            if member.issym():
                require(not Path(member.linkname).is_absolute()
                        and "\\" not in member.linkname
                        and (target.parent / member.linkname).resolve().is_relative_to(root.resolve()),
                        "Bundle link escapes root")
                target.parent.mkdir(parents=True, exist_ok=True)
                target.symlink_to(member.linkname)
            elif member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with bundle.extractfile(member) as source, target.open("xb") as output:
                    shutil.copyfileobj(source, output)
                target.chmod(member.mode & 0o755)


def run(command, env, log, timeout=60):
    """Retain process output privately; expose only known static failure categories."""
    with log.open("xb") as output:
        try:
            result = subprocess.run([str(c) for c in command], env=env, stdout=output,
                                    stderr=subprocess.STDOUT, check=False, timeout=timeout)
        except subprocess.TimeoutExpired:
            raise ValueError("Diagnostic process exceeded its time limit; output retained privately") from None
        except OSError:
            raise ValueError("Diagnostic process could not start; output retained privately") from None
    if result.returncode:
        with log.open("rb") as output:
            output.seek(max(0, log.stat().st_size - 64 * 1024))
            tail = output.read(64 * 1024).decode("utf-8", errors="replace")
        safe = next((message for message in (
            "Handy could not be queried.", "Handy's model-list format is not supported.",
            "Handy did not respond within the time limit.",
            "Synthetic model is not a currently installed streaming model advertised by Handy",
            "Synthetic fixture must be readable 16-kHz mono 16-bit PCM",
            "Synthetic native streaming/restart verification failed",
        ) if message in tail), "Process failed; raw output retained privately")
        raise ValueError(safe)


def check_handy_catalog(handy, root, manifest, env):
    """Query the real CLI and report readiness or static loader errors, never raw output."""
    stdout, stderr = root / "handy-catalog.json", root / "handy-catalog.log"
    with stdout.open("xb") as out, stderr.open("xb") as err:
        try:
            result = subprocess.run([str(handy), "--list-models", "--json"], cwd=handy.parent,
                                    env=env, stdout=out, stderr=err, timeout=45, check=False)
        except subprocess.TimeoutExpired:
            raise ValueError("Handy catalog exceeded its time limit; output retained privately") from None
        except OSError:
            raise ValueError("Handy catalog process could not start; output retained privately") from None
    require(stdout.stat().st_size <= 8 * 1024 * 1024 and stderr.stat().st_size <= 64 * 1024,
            "Handy preflight exceeded catalog output limits")
    errors = stderr.read_text(encoding="utf-8", errors="replace")
    summary = {"exitCode": result.returncode,
               "missingSharedLibrary": "error while loading shared libraries" in errors,
               "glibcVersionMismatch": "GLIBC_" in errors and "not found" in errors,
               "displayUnavailable": "cannot open display" in errors.lower()}
    missing = re.search(r"error while loading shared libraries: ([A-Za-z0-9_.+-]+):", errors)
    known_libraries = {"libopenblas.so.0", "libgtk-layer-shell.so.0", "libappindicator3.so.1",
                       "libtranscribe.so.0.2", "libonnxruntime.so.1", "libggml.so.0", "libggml-base.so.0"}
    summary["missingKnownLibraries"] = [missing[1]] if missing and missing[1] in known_libraries else []
    try:
        catalog = json.loads(stdout.read_text(encoding="utf-8"))
        require(isinstance(catalog, list), "Handy catalog must be an array")
        model_id = manifest["model"]["repository"] + "/" + manifest["model"]["file"]
        selected = [model for model in catalog if model.get("id") == model_id]
        summary.update(catalogValid=True, modelCount=len(catalog),
                       requiredModelInstalled=len(selected) == 1 and selected[0].get("is_downloaded") is True,
                       requiredModelStreaming=len(selected) == 1 and selected[0].get("supports_streaming") is True)
    except (ValueError, AttributeError, TypeError):
        summary["catalogValid"] = False
    print(json.dumps({"handyPreflight": summary}), flush=True)
    require(result.returncode == 0 and summary.get("requiredModelInstalled") is True
            and summary.get("requiredModelStreaming") is True, "Real Handy catalog preflight failed")


def stage_assets(root, rid, manifest, env):
    """Stage pinned real Handy and weights in the job-owned Hugging Face cache."""
    asset = manifest["handy"][rid]
    archive = root / asset["file"]
    download(f"https://github.com/cjpais/Handy/releases/download/v{manifest['handyVersion']}/{asset['file']}",
             archive, asset)
    if rid == "linux-x64":
        # Upstream builds the DEB on Ubuntu22.04; its AppImage uses Ubuntu24.04
        # and fails the real catalog preflight on this deliberately older host.
        # Extract privately; never install a package or execute maintainer scripts.
        bundle = root / "handy-deb"
        run(["dpkg-deb", "--extract", archive, bundle], env, root / "handy-extract.log")
        handy = bundle / "usr/bin/handy"
        require(handy.is_file() and not handy.is_symlink(), "Pinned Handy package needs a regular executable")
    else:
        extract_app(archive, root / "handy-bundle")
        binaries = list((root / "handy-bundle").glob("*.app/Contents/MacOS/*"))
        binaries = [p for p in binaries if p.name.lower() == "handy" and p.is_file()]
        require(len(binaries) == 1, "Expected exactly one Handy executable")
        handy = binaries[0]
    model = manifest["model"]
    cache = root / "home/huggingface/hub" / ("models--" + model["repository"].replace("/", "--"))
    weights = cache / "snapshots" / model["revision"] / model["file"]
    download(f"https://huggingface.co/{model['repository']}/resolve/{model['revision']}/{model['file']}",
             weights, model)
    refs = cache / "refs"
    refs.mkdir()
    for ref in (model["revision"], "main"):
        (refs / ref).write_text(model["revision"], encoding="utf-8")
    # The catalog's historical revision predates these notices. The pinned newer
    # revision carries the same Q8 bytes; retain its OpenMDW-1.1 license + origin.
    for name, notice in model["notices"].items():
        download(f"https://huggingface.co/{model['repository']}/resolve/{model['noticeRevision']}/{name}",
                 root / "model-notices" / name, notice)
    return handy, weights


def pcm(path):
    """Read a complete short mono PCM16 fixture and reject unsupported formats."""
    with wave.open(str(path), "rb") as audio:
        require((audio.getnchannels(), audio.getsampwidth(), audio.getcomptype()) == (1, 2, "NONE"),
                "Speech fixture must be mono PCM16")
        rate, count = audio.getframerate(), audio.getnframes()
        require(8000 <= rate <= 48000 and 3 * rate <= count <= 30 * rate,
                "Speech fixture must contain 3..30 seconds at a supported rate")
        data = audio.readframes(count)
        require(len(data) == count * 2, "Truncated speech fixture")
    return rate, data


def generate_speech(root, rid, manifest, env):
    """Generate known English speech locally and convert only fixtures to16kHz PCM."""
    for source in SOURCES:
        raw = root / (source + "-tts.wav")
        target = root / (source + ".wav")
        text = manifest["speech"][source]["text"]
        if rid == "linux-x64":
            run(["espeak-ng", "-v", "en-us", "-s", "145", "-w", raw, text], env,
                root / (source + "-tts.log"))
            rate, data = pcm(raw)
            values = struct.unpack(f"<{len(data) // 2}h", data)
            # Test-fixture conversion only; production capture uses its own path.
            output = []
            for i in range(len(values) * 16000 // rate):
                position = i * rate / 16000
                left = int(position)
                fraction = position - left
                right = min(left + 1, len(values) - 1)
                output.append(round(values[left] * (1 - fraction) + values[right] * fraction))
            with wave.open(str(target), "wb") as audio:
                audio.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
                audio.writeframes(struct.pack(f"<{len(output)}h", *output))
        else:
            aiff = root / (source + ".aiff")
            run(["say", "-v", "Samantha", "-r", "145", "-o", aiff, text], env,
                root / (source + "-tts.log"))
            run(["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", aiff, target], env,
                root / (source + "-convert.log"))


def words(text):
    """Normalize case/punctuation and the two preregistered numeric alternatives."""
    require(isinstance(text, str), "Transcript text is missing")
    # Declared numeric alternatives, no general deletion of numbers/content.
    text = re.sub(r"\b42\b", "forty two", text.lower())
    text = re.sub(r"\b9\b", "nine", text)
    return re.findall(r"[a-z0-9]+", text)


def word_errors(reference, actual):
    """Count word insertions, deletions and substitutions using edit distance."""
    previous = list(range(len(actual) + 1))
    for i, expected in enumerate(reference, 1):
        current = [i]
        for j, received in enumerate(actual, 1):
            current.append(min(current[-1] + 1, previous[j] + 1,
                               previous[j - 1] + (received != expected)))
        previous = current
    return previous[-1]


def verify_result(result, fixtures, manifest):
    """Require complete durable tracks, real restart proof and reference content."""
    for flag in ("success", "synthetic", "intentionalOwnedAsrKill", "confirmedAudioHashesPreserved",
                 "confirmedTextJournalPrefixPreserved", "bothTranscriptsComplete", "reopenedTextMatches"):
        require(result.get(flag) is True, "Missing real ASR/restart proof: " + flag)
    require(result.get("realtime") is False and result.get("openedAudioDevices") is False
            and result.get("backend") == "CPU", "Unexpected probe mode")
    require(type(result.get("systemRestarts")) is int and result["systemRestarts"] >= 1,
            "Owned worker did not restart")
    require(math.isfinite(result["elapsedSeconds"]) and 0 < result["elapsedSeconds"] < 240,
            "Recognition exceeded the preregistered deadline")
    parts = result.get("parts", [])
    require(len(parts) == 2 and sorted(p["source"] for p in parts) == sorted(SOURCES),
            "Expected one complete transcript per distinct track")
    receipts = {}
    for source in SOURCES:
        part = next(p for p in parts if p["source"] == source)
        rate, data = pcm(fixtures / (source + ".wav"))
        require(rate == 16000, "Recognition fixture must be 16 kHz")
        count = len(data) // 2
        require(part["status"] == "complete" and part["audioClosed"] is True
                and part["confirmedSampleCount"] == count and part["processedSamples"] == count
                and part["confirmedSha256"] == hashlib.sha256(data).hexdigest(),
                "Incomplete, truncated or changed audio/transcript")
        expected = words(manifest["speech"][source]["text"])
        actual = words(part.get("text"))
        errors = word_errors(expected, actual)
        require(errors / len(expected) <= manifest["maximumWordErrorRate"], "Transcript word error rate exceeds 15 percent: " + source)
        require(set(manifest["speech"][source]["anchors"]) <= set(actual), "Missing track-specific facts: " + source)
        receipts[source] = {"samples": count, "pcmSha256": hashlib.sha256(data).hexdigest(),
                            "referenceWords": len(expected), "wordErrors": errors,
                            "wordErrorRate": errors / len(expected)}
    require(receipts["system"]["pcmSha256"] != receipts["microphone"]["pcmSha256"], "Tracks must differ")
    return receipts


def failed_probe_summary(root):
    """Report failed checkpoint counters/categories without raw text, paths or IDs."""
    path = root / "probe/result.json"
    if not path.is_file() or path.stat().st_size > 8 * 1024 * 1024:
        return {"resultAvailable": False}
    try:
        result = json.loads(path.read_text(encoding="utf-8"))
        summary = {flag: result.get(flag) is True for flag in (
            "intentionalOwnedAsrKill", "confirmedAudioHashesPreserved",
            "confirmedTextJournalPrefixPreserved", "bothTranscriptsComplete", "reopenedTextMatches")}
        summary["resultAvailable"] = True
        tracks = []
        for source in SOURCES:
            for part in result.get("parts", []):
                if part.get("source") != source:
                    continue
                error = str(part.get("error", "")).lower()
                categories = [category for category, needles in (
                    ("deadline", ("deadline", "time limit")),
                    ("worker-transport", ("pipe", "channel", "protocol", "worker")),
                    ("model-runtime", ("model", "native", "runtime", "load")),
                    ("preservation", ("prefix", "hash", "journal")),
                ) if any(needle in error for needle in needles)]
                tracks.append({"source": source, "complete": part.get("status") == "complete",
                               "processedSamples": part.get("processedSamples")
                               if type(part.get("processedSamples")) is int else None,
                               "errorCategories": categories})
        summary["tracks"] = tracks
        return summary
    except (ValueError, AttributeError, TypeError):
        return {"resultAvailable": False}


def main():
    """Run isolated CI provisioning or explicit local reuse, then emit a safe verdict."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rid", choices=("linux-x64", "osx-arm64", "osx-x64", "win-x64"))
    parser.add_argument("executable", type=Path)
    parser.add_argument("backend", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--local-handy", type=Path)
    parser.add_argument("--local-model", type=Path)
    parser.add_argument("--fixtures", type=Path)
    args = parser.parse_args()
    local = (args.local_handy, args.local_model, args.fixtures)
    require(all(local) or not any(local), "Supply all three local inputs together")
    require(args.rid != "win-x64" or all(local), "Windows validation reuses explicit local assets only")
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    app, backend, root = args.executable.resolve(strict=True), args.backend.resolve(strict=True), args.output.absolute()
    root.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    if all(local):
        handy, model, fixtures = (p.resolve(strict=True) for p in local)
        verify_asset(model, manifest["model"])
    else:
        home = root / "home"
        for directory in (home, home / "config", home / "data", home / "cache"):
            directory.mkdir(exist_ok=True)
        env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / "config"), XDG_DATA_HOME=str(home / "data"),
                   XDG_CACHE_HOME=str(home / "cache"), HF_HOME=str(home / "huggingface"),
                   HF_HUB_CACHE=str(home / "huggingface/hub"))
        # Avoid injecting Meeting's SDK into the separate upstream Handy binary.
        for name in ("LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"):
            env.pop(name, None)
        if args.rid.startswith("osx-"):
            # Handy's catalog path also enumerates Metal devices. This CPU-only
            # check needs no GPU/shader initialization on virtual Mac runners.
            # ggml in Handy's pinned transcribe-cpp-sys0.2.4 supports zero devices.
            env["GGML_METAL_DEVICES"] = "0"
        handy, model = stage_assets(root, args.rid, manifest, env)
        fixtures = root
        generate_speech(root, args.rid, manifest, env)
        # Same checked executable bytes; rpath now resolves an owned SDK copy.
        program = root / "program"
        program.mkdir()
        copied = program / app.name
        shutil.copy2(app, copied)
        require(digest(copied) == digest(app), "Checked executable changed during staging")
        shutil.copytree(backend, program / "native", symlinks=False)
        app, backend = copied, program / "native"
        check_handy_catalog(handy, root, manifest, env)
    try:
        run([app, "--synthetic-smoke", model, backend, fixtures / "system.wav", fixtures / "microphone.wav",
             root / "probe", handy], env, root / "recognition.log", timeout=300)
    except ValueError:
        print(json.dumps({"failedProbe": failed_probe_summary(root)}), flush=True)
        raise
    result = json.loads((root / "probe/result.json").read_text(encoding="utf-8"))
    tracks = verify_result(result, fixtures, manifest)
    # Allowlisted scalar/derived proof, deliberately no raw session/catalog/logs.
    receipt = {"success": True, "rid": args.rid, "version": result["version"],
               "executableSha256": digest(app),
               "handyVersion": None if all(local) else manifest["handyVersion"],
               "handyReleaseArchiveVerified": not all(local), "handyExecutableSha256": digest(handy),
               "handyGpuEnumerationDisabledForCpuCheck": not all(local) and args.rid.startswith("osx-"),
               "modelSha256": manifest["model"]["sha256"], "modelRevision": manifest["model"]["revision"],
               "backend": "CPU", "syntheticSpeech": True, "realtime": False, "physicalDevices": False,
               "elapsedSeconds": result["elapsedSeconds"], "maximumWordErrorRate": manifest["maximumWordErrorRate"],
               "tracks": tracks, "ownedWorkerRestarted": True, "audioHashesAndJournalPreserved": True,
               "limits": "Short accelerated two-track speech/restart check. No live capture, GPU, latency, memory or two-hour claim."}
    with (root / "checks.json").open("x", encoding="utf-8") as output:
        json.dump(receipt, output, indent=2)
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
