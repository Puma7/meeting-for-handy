"""Regression guards for actual-ASR verdicts; unit inputs are synthetic, no downloads."""
import copy
import hashlib
import importlib.util
import io
import json
import math
import os
from pathlib import Path
import struct
import tarfile
import tempfile
import sys
import types
import unittest
from unittest import mock
import wave


def load(name, file):
    """Import the actual verifier so regressions exercise its public contracts."""
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


asr = load("portable_asr", "check-portable-asr.py")
audio = load("linux_audio_result", "check-linux-audio-result.py")


class ReferenceVerdicts(unittest.TestCase):
    def setUp(self):
        """Create distinct owned PCM fixtures and a complete native-result baseline."""
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest = json.loads(asr.MANIFEST.read_text(encoding="utf-8"))
        self.result = {flag: True for flag in ("success", "synthetic", "intentionalOwnedAsrKill",
                       "confirmedAudioHashesPreserved", "confirmedTextJournalPrefixPreserved",
                       "bothTranscriptsComplete", "reopenedTextMatches")}
        self.result.update(realtime=False, openedAudioDevices=False, backend="CPU", elapsedSeconds=5,
                           systemRestarts=1, parts=[])
        for source, value in (("system", 100), ("microphone", -200)):
            data = struct.pack("<h", value) * (16000 * 4)
            with wave.open(str(self.root / (source + ".wav")), "wb") as output:
                output.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
                output.writeframes(data)
            self.result["parts"].append({"source": source, "status": "complete", "audioClosed": True,
                "confirmedSampleCount": len(data) // 2, "processedSamples": len(data) // 2,
                "confirmedSha256": hashlib.sha256(data).hexdigest(),
                "text": self.manifest["speech"][source]["text"]})

    def verify(self):
        """Apply the production content verdict to the current mutated fixture."""
        return asr.verify_result(self.result, self.root, self.manifest)

    def test_exact_speech_and_declared_numeric_alternatives_pass(self):
        """Accept exact reference words and only the declared numeric equivalent."""
        self.assertEqual(self.verify()["system"]["wordErrors"], 0)
        self.result["parts"][1]["text"] = self.result["parts"][1]["text"].replace("forty two", "42")
        self.assertEqual(self.verify()["microphone"]["wordErrors"], 0)

    def test_nonempty_gibberish_and_swapped_tracks_fail_even_when_native_probe_succeeded(self):
        """Reject wrong content even when native mechanical success flags are true."""
        original = copy.deepcopy(self.result)
        self.result["parts"][0]["text"] = "nonempty unrelated words"
        with self.assertRaisesRegex(ValueError, "word error"):
            self.verify()
        self.result = original
        left, right = self.result["parts"]
        left["text"], right["text"] = right["text"], left["text"]
        with self.assertRaises(ValueError):
            self.verify()

    def test_critical_fact_change_fails_even_below_word_error_limit(self):
        """Require source-specific facts even when overall edit distance is small."""
        self.result["parts"][0]["text"] = self.result["parts"][0]["text"].replace("Friday", "Monday")
        with self.assertRaisesRegex(ValueError, "track-specific"):
            self.verify()

    def test_truncation_changed_pcm_and_missing_restart_fail(self):
        """Reject partial recognition, changed audio, open files and absent restart proof."""
        for field, value in (("confirmedSampleCount", 0), ("processedSamples", 1),
                             ("confirmedSha256", "0" * 64), ("audioClosed", False)):
            with self.subTest(field=field):
                old = self.result["parts"][0][field]
                self.result["parts"][0][field] = value
                with self.assertRaises(ValueError):
                    self.verify()
                self.result["parts"][0][field] = old
        self.result["systemRestarts"] = True
        with self.assertRaises(ValueError):
            self.verify()

    def test_download_is_bounded_hash_checked_and_never_overwrites(self):
        """Reject oversized or substituted downloads while preserving prior evidence."""
        path = self.root / "download"
        asset = {"bytes": 3, "sha256": hashlib.sha256(b"abc").hexdigest()}
        with mock.patch.object(asr.urllib.request, "urlopen", return_value=io.BytesIO(b"abcd")):
            with self.assertRaisesRegex(ValueError, "exceeds"):
                asr.download("https://example.test/pinned", path, asset)
        self.assertTrue(path.exists())
        with self.assertRaises(FileExistsError):
            asr.download("https://example.test/pinned", path, asset)
        path2 = self.root / "wrong"
        with mock.patch.object(asr.urllib.request, "urlopen", return_value=io.BytesIO(b"abd")):
            with self.assertRaisesRegex(ValueError, "digest"):
                asr.download("https://example.test/pinned", path2, asset)

    def test_short_download_and_timeout_preserve_evidence_and_fail(self):
        """Keep partial download evidence and never turn EOF or timeout into success."""
        asset = {"bytes": 3, "sha256": hashlib.sha256(b"abc").hexdigest()}
        with mock.patch.object(asr.urllib.request, "urlopen", return_value=io.BytesIO(b"ab")):
            with self.assertRaisesRegex(ValueError, "length"):
                asr.download("https://example.test/pinned", self.root / "short", asset)
        self.assertEqual((self.root / "short").read_bytes(), b"ab")
        with mock.patch.object(asr.urllib.request, "urlopen", side_effect=TimeoutError("synthetic timeout")):
            with self.assertRaises(TimeoutError):
                asr.download("https://example.test/pinned", self.root / "timeout", asset)
        self.assertTrue((self.root / "timeout").exists())

    def test_ci_provisioning_isolates_home_hf_and_loaders_and_copies_checked_executable(self):
        """Exercise fresh CI provisioning with tiny pinned synthetic assets, not inference."""
        manifest = copy.deepcopy(self.manifest)
        bundle = io.BytesIO()
        with tarfile.open(fileobj=bundle, mode="w:gz") as archive:
            executable = tarfile.TarInfo("Handy.app/Contents/MacOS/handy")
            executable.size, executable.mode = 4, 0o755
            archive.addfile(executable, io.BytesIO(b"real"))
        payloads = {manifest["handy"]["osx-arm64"]["file"]: bundle.getvalue(),
                    manifest["model"]["file"]: b"model", "LICENSE": b"license", "NOTICE": b"notice"}
        for asset, name in [(manifest["handy"]["osx-arm64"], manifest["handy"]["osx-arm64"]["file"]),
                            (manifest["model"], manifest["model"]["file"])] + list(
                                (value, key) for key, value in manifest["model"]["notices"].items()):
            asset.update(bytes=len(payloads[name]), sha256=hashlib.sha256(payloads[name]).hexdigest())
        definition = self.root / "assets.json"
        definition.write_text(json.dumps(manifest))
        app, sdk, output = self.root / "HandyMeeting", self.root / "sdk", self.root / "ci-output"
        app.write_bytes(b"exact checked executable")
        sdk.mkdir()
        (sdk / "libtranscribe.dylib").write_bytes(b"reviewed sdk")
        result = copy.deepcopy(self.result)
        result["version"] = "test"
        calls = []

        def response(url, timeout):
            """Return the selected tiny pinned asset without network access."""
            self.assertEqual(timeout, 60)
            return io.BytesIO(payloads[url.rsplit("/", 1)[1]])

        def generate(root, rid, definition, env):
            """Reuse owned PCM fixtures instead of invoking a host speech engine."""
            for source in asr.SOURCES:
                (root / (source + ".wav")).write_bytes((self.root / (source + ".wav")).read_bytes())

        def recognize(command, env, log, timeout):
            """Simulate only native process output while assertions verify orchestration."""
            calls.append(command)
            self.assertEqual(timeout, 300)
            self.assertNotIn("LD_LIBRARY_PATH", env)
            self.assertNotIn("DYLD_LIBRARY_PATH", env)
            self.assertEqual(env["GGML_METAL_DEVICES"], "0")
            self.assertEqual(env["HOME"], str(output / "home"))
            self.assertEqual(env["HF_HOME"], str(output / "home/huggingface"))
            self.assertEqual(env["XDG_DATA_HOME"], str(output / "home/data"))
            self.assertEqual(Path(command[0]).read_bytes(), app.read_bytes())
            self.assertEqual((Path(command[3]) / "libtranscribe.dylib").read_bytes(), b"reviewed sdk")
            refs = Path(command[2]).parents[2] / "refs"
            self.assertEqual((refs / manifest["model"]["revision"]).read_text(), manifest["model"]["revision"])
            self.assertEqual(Path(command[-1]).read_bytes(), b"real")
            Path(command[6]).mkdir()
            (Path(command[6]) / "result.json").write_text(json.dumps(result))

        def query(command, **kwargs):
            """Simulate the real CLI boundary without exposing untrusted diagnostic text."""
            self.assertEqual(command[1:], ["--list-models", "--json"])
            self.assertEqual(kwargs["timeout"], 45)
            kwargs["stdout"].write(json.dumps([{"id": manifest["model"]["repository"] + "/" + manifest["model"]["file"],
                "is_downloaded": True, "supports_streaming": True}]).encode())
            return types.SimpleNamespace(returncode=0)

        argv = ["check-portable-asr.py", "osx-arm64", str(app), str(sdk), str(output)]
        with mock.patch.object(asr, "MANIFEST", definition), mock.patch.object(sys, "argv", argv), \
             mock.patch.object(asr.urllib.request, "urlopen", side_effect=response), \
             mock.patch.object(asr, "generate_speech", side_effect=generate), \
             mock.patch.object(asr, "run", side_effect=recognize), \
             mock.patch.object(asr.subprocess, "run", side_effect=query), \
             mock.patch.dict(os.environ, {"LD_LIBRARY_PATH": "caller-sdk", "DYLD_LIBRARY_PATH": "caller-sdk"}):
            asr.main()
            self.assertEqual(os.environ["LD_LIBRARY_PATH"], "caller-sdk")
        self.assertEqual(len(calls), 1)
        receipt = json.loads((output / "checks.json").read_text())
        self.assertNotIn(str(self.root), json.dumps(receipt))
        self.assertNotIn("parts", receipt)
        self.assertTrue(receipt["success"])
        self.assertTrue(receipt["handyReleaseArchiveVerified"])
        self.assertTrue(receipt["handyGpuEnumerationDisabledForCpuCheck"])
        self.assertEqual(receipt["handyVersion"], "0.9.8")
        self.assertEqual(app.read_bytes(), b"exact checked executable")

    def test_catalog_failure_reports_only_static_loader_categories(self):
        """Classify missing libraries without publishing raw paths or log fragments."""
        def query(command, **kwargs):
            """Simulate the real CLI boundary without exposing untrusted diagnostic text."""
            kwargs["stderr"].write(b"private-path/private-session: error while loading shared libraries: arbitrary.so")
            return types.SimpleNamespace(returncode=127)
        with mock.patch.object(asr.subprocess, "run", side_effect=query), \
             mock.patch("builtins.print") as printed:
            with self.assertRaisesRegex(ValueError, "catalog preflight failed"):
                asr.check_handy_catalog(self.root / "handy", self.root, self.manifest, {})
        summary = json.loads(printed.call_args.args[0])["handyPreflight"]
        self.assertTrue(summary["missingSharedLibrary"])
        self.assertNotIn("private", json.dumps(summary))
        self.assertNotIn("arbitrary", json.dumps(summary))

    def test_catalog_failure_exposes_only_known_missing_runtime_libraries(self):
        """Keep a declared runtime dependency actionable without exposing private paths."""
        def query(command, **kwargs):
            """Model a loader failure including an untrusted path and unknown library."""
            kwargs["stderr"].write(b"/private/session: error while loading shared libraries: libopenblas.so.0: cannot open shared object file\nunknown-private.so")
            return types.SimpleNamespace(returncode=127)
        with mock.patch.object(asr.subprocess, "run", side_effect=query), \
             mock.patch("builtins.print") as printed:
            with self.assertRaisesRegex(ValueError, "catalog preflight failed"):
                asr.check_handy_catalog(self.root / "handy", self.root, self.manifest, {})
        summary = json.loads(printed.call_args.args[0])["handyPreflight"]
        self.assertEqual(summary["missingKnownLibraries"], ["libopenblas.so.0"])
        self.assertNotIn("private", json.dumps(summary))

    def test_process_error_never_echoes_raw_transcript_or_path(self):
        """Expose a known static error while keeping raw process output private."""
        def failed(command, **kwargs):
            """Return a failed process whose private output must never be echoed."""
            kwargs["stdout"].write(b"private transcript /home/person/session/secret\nHandy could not be queried.")
            return types.SimpleNamespace(returncode=1)
        with mock.patch.object(asr.subprocess, "run", side_effect=failed):
            with self.assertRaisesRegex(ValueError, "^Handy could not be queried\\.$"):
                asr.run(["owned-test-process"], {}, self.root / "process.log")

    def test_timeout_and_launch_errors_do_not_publish_command_arguments(self):
        """Keep executable paths and speech arguments out of timeout/launch tracebacks."""
        failures = [asr.subprocess.TimeoutExpired(["private-speech", "/private/path"], 1),
                    OSError("private-path")]
        for index, failure in enumerate(failures):
            with mock.patch.object(asr.subprocess, "run", side_effect=failure):
                with self.assertRaises(ValueError) as caught:
                    asr.run(["private-speech"], {}, self.root / (str(index) + ".log"))
                self.assertNotIn("private-path", str(caught.exception))
                self.assertNotIn("private-speech", str(caught.exception))
                self.assertTrue(caught.exception.__suppress_context__)
                root = self.root / ("catalog" + str(index))
                root.mkdir()
                with self.assertRaises(ValueError) as caught:
                    asr.check_handy_catalog(self.root / "private-path", root, self.manifest, {})
                self.assertNotIn("private-path", str(caught.exception))
                self.assertTrue(caught.exception.__suppress_context__)

    def test_failed_probe_summary_excludes_raw_errors_transcripts_and_session_ids(self):
        """Publish only whitelisted checkpoint facts, never arbitrary result strings."""
        result = copy.deepcopy(self.result)
        result["id"] = "private-session-id"
        result["parts"][0].update(text="private transcript", error="worker failed /private/model/file")
        (self.root / "probe").mkdir()
        (self.root / "probe/result.json").write_text(json.dumps(result))
        summary = asr.failed_probe_summary(self.root)
        self.assertTrue(summary["resultAvailable"])
        self.assertIn("model-runtime", summary["tracks"][0]["errorCategories"])
        self.assertNotIn("private", json.dumps(summary))

    def test_local_executable_is_hashed_without_claiming_a_pinned_release_version(self):
        """Identify a local executable without borrowing an unverified CI version."""
        definition = self.root / "local-assets.json"
        manifest = copy.deepcopy(self.manifest)
        model, handy, app, sdk = (self.root / name for name in ("model", "handy", "app", "sdk"))
        model.write_bytes(b"model")
        handy.write_bytes(b"locally selected Handy, version not queried")
        app.write_bytes(b"checked application")
        sdk.mkdir()
        manifest["model"].update(bytes=5, sha256=hashlib.sha256(b"model").hexdigest())
        definition.write_text(json.dumps(manifest))
        output = self.root / "local-run"
        result = copy.deepcopy(self.result)
        result["version"] = "test"

        def recognize(command, env, log, timeout):
            """Simulate only native process output while assertions verify orchestration."""
            Path(command[6]).mkdir()
            (Path(command[6]) / "result.json").write_text(json.dumps(result))

        argv = ["check-portable-asr.py", "win-x64", str(app), str(sdk), str(output),
                "--local-handy", str(handy), "--local-model", str(model), "--fixtures", str(self.root)]
        with mock.patch.object(asr, "MANIFEST", definition), mock.patch.object(sys, "argv", argv), \
             mock.patch.object(asr, "run", side_effect=recognize), \
             mock.patch.object(asr.urllib.request, "urlopen", side_effect=AssertionError("Local mode must not download")):
            asr.main()
        receipt = json.loads((output / "checks.json").read_text())
        self.assertIsNone(receipt["handyVersion"])
        self.assertFalse(receipt["handyReleaseArchiveVerified"])
        self.assertEqual(receipt["handyExecutableSha256"], hashlib.sha256(handy.read_bytes()).hexdigest())

    def test_linux_uses_ubuntu22_deb_without_installation_or_appimage_execution(self):
        """Exercise private extraction rather than the newer-GLIBC AppImage launcher."""
        calls = []

        def downloaded(url, path, asset):
            """Create only owned placeholder assets at the production cache locations."""
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture")

        def extract(command, env, log, timeout=60):
            """Model dpkg extraction while rejecting installation or script execution."""
            calls.append(command)
            self.assertEqual(command[:2], ["dpkg-deb", "--extract"])
            self.assertTrue(str(command[2]).endswith("Handy_0.9.8_amd64.deb"))
            executable = Path(command[3]) / "usr/bin/handy"
            executable.parent.mkdir(parents=True)
            executable.write_bytes(b"extracted executable")

        with mock.patch.object(asr, "download", side_effect=downloaded), \
             mock.patch.object(asr, "run", side_effect=extract), \
             mock.patch.object(asr.subprocess, "run", side_effect=AssertionError("Do not execute AppImage")):
            handy, weights = asr.stage_assets(self.root, "linux-x64", self.manifest, {})
        self.assertEqual(len(calls), 1)
        self.assertEqual(handy.read_bytes(), b"extracted executable")
        self.assertTrue(weights.is_file())
        self.assertEqual(self.manifest["handy"]["linux-x64"]["sha256"],
                         "4efe1fdcc2b68e95612e866810ccb23e06bbdf004a4e857de0862b0c6927caa8")

    def test_bundle_rejects_traversal_and_escaping_links(self):
        """Reject archive entries that could escape the new owned bundle directory."""
        for index, (name, link) in enumerate((("../outside", None), ("Handy.app/link", "../../outside"))):
            archive = self.root / (str(index) + ".tar.gz")
            with tarfile.open(archive, "w:gz") as bundle:
                entry = tarfile.TarInfo(name)
                if link:
                    entry.type, entry.linkname = tarfile.SYMTYPE, link
                bundle.addfile(entry, io.BytesIO(b""))
            with self.assertRaises(ValueError):
                asr.extract_app(archive, self.root / ("bundle" + str(index)))

    @unittest.skipIf(os.name == "nt", "Real framework links/modes require a Unix target host")
    def test_valid_mac_framework_link_and_executable_mode_are_retained(self):
        """Preserve legitimate internal framework links and Unix executable permissions."""
        archive = self.root / "valid.tar.gz"
        with tarfile.open(archive, "w:gz") as bundle:
            entry = tarfile.TarInfo("Handy.app/Contents/Frameworks/Test.framework/Versions/A/Test")
            entry.size, entry.mode = 4, 0o755
            bundle.addfile(entry, io.BytesIO(b"test"))
            link = tarfile.TarInfo("Handy.app/Contents/Frameworks/Test.framework/Test")
            link.type, link.linkname = tarfile.SYMTYPE, "Versions/A/Test"
            bundle.addfile(link)
        asr.extract_app(archive, self.root / "bundle")
        target = self.root / "bundle/Handy.app/Contents/Frameworks/Test.framework/Test"
        self.assertTrue(target.is_symlink())
        self.assertEqual(target.read_bytes(), b"test")
        self.assertTrue(os.access(target, os.X_OK))

    def test_linux_unexpected_capture_failure_cannot_pass_on_duration_and_signal(self):
        """Fail unexpected capture warnings even when duration appears sufficient."""
        (self.root / "checkpoints").mkdir()
        session = {"id": "a" * 32, "status": "recorded", "captureIncomplete": True,
                   "warnings": ["system: capture process exited unexpectedly"], "parts": []}
        result = {"systemMs": 8500, "microphoneMs": 8500, "captureIncomplete": True,
                  "trackDifferenceWithinLimit": True, "warnings": session["warnings"]}
        (self.root / "result.json").write_text(json.dumps(result))
        (self.root / "checkpoints/checkpoint-000000.json").write_text(json.dumps(session))
        with self.assertRaisesRegex(ValueError, "Unexpected capture warning"):
            audio.verify(self.root)

    def linux_fixture(self, counts=(136000, 136000), warnings=None):
        """Create closed two-tone PCM and matching hashes for the actual Linux verifier."""
        warnings = [audio.TIMING] if warnings is None else warnings
        session = {"id": "b" * 32, "status": "recorded",
                   "captureIncomplete": any(w.endswith(audio.START) for w in warnings),
                   "warnings": warnings, "parts": []}
        folder = self.root / "sessions" / session["id"] / "audio"
        folder.mkdir(parents=True)
        for source, hz, count in zip(asr.SOURCES, (440, 1000), counts):
            data = b"".join(struct.pack("<h", int(8000 * math.sin(2 * math.pi * hz * i / 16000)))
                            for i in range(count))
            name = source + "-000000000000.wav"
            with wave.open(str(folder / name), "wb") as output:
                output.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
                output.writeframes(data)
            session["parts"].append({"source": source, "file": name, "audioClosed": True,
                "sampleCount": count, "confirmedSampleCount": count,
                "confirmedSha256": hashlib.sha256(data).hexdigest()})
        result = {"systemMs": counts[0] * 1000 // 16000, "microphoneMs": counts[1] * 1000 // 16000,
                  "captureIncomplete": session["captureIncomplete"], "warnings": warnings,
                  "trackDifferenceWithinLimit": abs(counts[0] - counts[1]) <= 8000}
        (self.root / "checkpoints").mkdir()
        (self.root / "checkpoints/checkpoint-000000.json").write_text(json.dumps(session))
        (self.root / "result.json").write_text(json.dumps(result))
        return folder

    def test_full_linux_pcm_tones_pass_and_start_gaps_remain_incomplete(self):
        """Accept transport proof while preserving the incomplete startup-gap verdict."""
        self.linux_fixture(warnings=[audio.TIMING, "system" + audio.START])
        receipt = audio.verify(self.root)
        self.assertEqual(receipt["verdict"], "passed-with-startup-gaps")
        self.assertFalse(receipt["completeCapture"])
        self.assertFalse(receipt["physicalHardware"])

    def test_linux_clean_capture_passes_but_changed_confirmed_pcm_fails(self):
        """Prove full distinct tones pass and a changed confirmed byte fails."""
        folder = self.linux_fixture()
        self.assertTrue(audio.verify(self.root)["completeCapture"])
        file = folder / "system-000000000000.wav"
        data = bytearray(file.read_bytes())
        data[-1] ^= 1
        file.write_bytes(data)
        with self.assertRaisesRegex(ValueError, "Confirmed audio"):
            audio.verify(self.root)

    def test_linux_half_length_cannot_pass_old_four_second_minimum(self):
        """Reject a half-length capture that passed the former duration minimum."""
        self.linux_fixture(counts=(64000, 64000))
        with self.assertRaisesRegex(ValueError, "truncated"):
            audio.verify(self.root)

    def test_linux_more_than_500_ms_track_drift_fails(self):
        """Preserve the preregistered500ms maximum difference between saved tracks."""
        self.linux_fixture(counts=(136000, 144016))
        with self.assertRaisesRegex(ValueError, "500 ms"):
            audio.verify(self.root)

    def test_linux_swapped_or_silent_track_fails_even_with_matching_hashes(self):
        """Detect wrong signal sources independently of self-consistent PCM hashes."""
        folder = self.linux_fixture()
        checkpoint = self.root / "checkpoints/checkpoint-000000.json"
        session = json.loads(checkpoint.read_text())
        part = session["parts"][0]
        system_file = folder / part["file"]
        for data in ((folder / session["parts"][1]["file"]).read_bytes(),
                     system_file.read_bytes()[:44] + b"\0" * (136000 * 2)):
            system_file.write_bytes(data)
            with wave.open(str(system_file), "rb") as saved:
                part["confirmedSha256"] = hashlib.sha256(saved.readframes(saved.getnframes())).hexdigest()
            checkpoint.write_text(json.dumps(session))
            with self.assertRaisesRegex(ValueError, "Silent or mixed-up"):
                audio.verify(self.root)

    def rewrite_linux_signal(self, source, transform):
        """Change only synthetic PCM and its matching confirmation, preserving duration."""
        checkpoint = self.root / "checkpoints/checkpoint-000000.json"
        session = json.loads(checkpoint.read_text())
        part = next(p for p in session["parts"] if p["source"] == source)
        file = self.root / "sessions" / session["id"] / "audio" / part["file"]
        with wave.open(str(file), "rb") as saved:
            data = transform(saved.readframes(saved.getnframes()))
        with wave.open(str(file), "wb") as saved:
            saved.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
            saved.writeframes(data)
        part["confirmedSha256"] = hashlib.sha256(data).hexdigest()
        checkpoint.write_text(json.dumps(session))

    def test_linux_late_player_exit_cannot_pass_middle_only_tone_check(self):
        """A full-length/hash-consistent track still fails when its last two seconds are silent."""
        self.linux_fixture()
        self.rewrite_linux_signal("system", lambda data: data[:-64000] + bytes(64000))
        with self.assertRaisesRegex(ValueError, "Silent or mixed-up"):
            audio.verify(self.root)

    def test_linux_early_silence_without_its_own_startup_warning_fails(self):
        """Another track's startup warning cannot excuse missing microphone signal."""
        self.linux_fixture(warnings=[audio.TIMING, "system" + audio.START])
        self.rewrite_linux_signal("microphone", lambda data: bytes(64000) + data[64000:])
        with self.assertRaisesRegex(ValueError, "Silent or mixed-up"):
            audio.verify(self.root)

    def test_linux_signal_failure_reports_only_source_and_numeric_window_evidence(self):
        """Identify a failing source/window without exposing PCM, paths or session identities."""
        self.linux_fixture(counts=(144000, 144000))
        self.rewrite_linux_signal("microphone", lambda data: bytes(3200) + data[3200:])
        with self.assertRaisesRegex(ValueError, "Silent or mixed-up") as raised:
            audio.verify(self.root)
        message = str(raised.exception)
        detail = json.loads(message.partition(": ")[2])
        self.assertEqual(detail["source"], "microphone")
        self.assertEqual(detail["startSample"], 0)
        self.assertEqual(detail["endSample"], 4000)
        self.assertEqual(detail["trackSamples"], 144000)
        self.assertEqual(detail["expectedHz"], 1000)
        self.assertEqual(detail["toleranceHz"], 60)
        self.assertEqual(detail["minimumRmsExclusive"], 1000)
        self.assertEqual(detail["sampleRate"], 16000)
        self.assertFalse(detail["declaredStartup"])
        self.assertEqual(detail["leadingZeroSamples"], 1601)
        self.assertGreater(detail["rms"], 1000)
        self.assertLess(detail["hz"], 940)
        self.assertNotIn(str(self.root), message)
        self.assertNotIn("b" * 32, message)
        self.assertTrue(all(isinstance(value, (int, float, bool))
                            for key, value in detail.items() if key != "source"))

    def test_linux_signal_failure_identifies_a_late_system_window(self):
        """Retain the strict final-window check and expose where its signal actually failed."""
        self.linux_fixture(counts=(144000, 144000))
        self.rewrite_linux_signal("system", lambda data: data[:-16000] + bytes(16000))
        with self.assertRaisesRegex(ValueError, "Silent or mixed-up") as raised:
            audio.verify(self.root)
        detail = json.loads(str(raised.exception).partition(": ")[2])
        self.assertEqual(detail["source"], "system")
        self.assertGreater(detail["startSample"], 7 * 16000)
        self.assertLessEqual(detail["endSample"], 144000)
        self.assertEqual(detail["expectedHz"], 440)
        self.assertEqual(detail["toleranceHz"], 40)

    def test_linux_startup_diagnostics_separate_native_padding_and_helper_zeros(self):
        """Surface only exact source counters without excusing undeclared missing signal."""
        self.linux_fixture(counts=(144000, 144000))
        self.rewrite_linux_signal("microphone", lambda data: bytes(910) + data[910:])
        path = self.root / "result.json"
        result = json.loads(path.read_text())
        result["portableStartup"] = [{"source": "microphone", "writtenBeforePadding": 0,
            "estimatedPaddingSamples": 451, "firstPacketSamples": 1600,
            "firstPacketLeadingZeroSamples": 4, "ignored": "private fixture content"}]
        path.write_text(json.dumps(result))
        with self.assertRaisesRegex(ValueError, "Silent or mixed-up") as raised:
            audio.verify(self.root)
        detail = json.loads(str(raised.exception).partition(": ")[2])
        self.assertEqual(detail["nativeEstimatedPaddingSamples"], 451)
        self.assertEqual(detail["nativeFirstPacketLeadingZeroSamples"], 4)
        self.assertEqual(detail["nativeFirstPacketSamples"], 1600)
        self.assertFalse(detail["declaredStartup"])
        self.assertNotIn("private fixture", str(raised.exception))
        self.assertTrue(all(type(value) in (int, float, bool)
                            for key, value in detail.items() if key != "source"))

    def test_linux_startup_diagnostics_do_not_accept_untrusted_counter_text(self):
        """Malformed counters fail with fixed text instead of reflecting arbitrary local values."""
        result = {"portableStartup": [{"source": "system", "writtenBeforePadding": 0,
            "estimatedPaddingSamples": "private fixture path", "firstPacketSamples": 1600,
            "firstPacketLeadingZeroSamples": 0}]}
        with self.assertRaisesRegex(ValueError, "Invalid portable startup counter") as raised:
            audio.startup_counters(result, "system")
        self.assertNotIn("private fixture", str(raised.exception))

    def test_linux_short_native_padding_is_only_accepted_as_declared_incomplete_audio(self):
        """A truthful short native-start warning keeps all tone limits and the incomplete verdict."""
        self.linux_fixture(counts=(144922, 144922),
                           warnings=[audio.TIMING, "microphone" + audio.START])
        self.rewrite_linux_signal("microphone", lambda data: bytes(1044) + data[:-1044])
        path = self.root / "result.json"
        result = json.loads(path.read_text())
        result["portableStartup"] = [{"source": "microphone", "writtenBeforePadding": 0,
            "estimatedPaddingSamples": 522, "firstPacketSamples": 200,
            "firstPacketLeadingZeroSamples": 1}]
        path.write_text(json.dumps(result))
        receipt = audio.verify(self.root)
        self.assertFalse(receipt["completeCapture"])
        self.assertEqual(receipt["verdict"], "passed-with-startup-gaps")
        self.assertEqual(receipt["tracks"]["microphone"]["nativeEstimatedPaddingSamples"], 522)
        self.assertGreaterEqual(receipt["tracks"]["microphone"]["qualifiedToneSamples"], 128000)
        # Merely supplying native counters never substitutes for the warning/flag.
        checkpoint = self.root / "checkpoints/checkpoint-000000.json"
        session = json.loads(checkpoint.read_text())
        result["warnings"] = session["warnings"] = [audio.TIMING]
        result["captureIncomplete"] = session["captureIncomplete"] = False
        path.write_text(json.dumps(result))
        checkpoint.write_text(json.dumps(session))
        with self.assertRaisesRegex(ValueError, "Silent or mixed-up"):
            audio.verify(self.root)

    def test_linux_startup_diagnostics_are_source_specific_and_optional(self):
        """Keep older results compatible and never use one track's counters for another."""
        entry = {"source": "system", "writtenBeforePadding": 0,
            "estimatedPaddingSamples": 455, "firstPacketSamples": 1600,
            "firstPacketLeadingZeroSamples": 0}
        self.assertEqual(audio.startup_counters({}, "system"), {})
        self.assertEqual(audio.startup_counters({"portableStartup": [entry]}, "microphone"), {})
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            audio.startup_counters({"portableStartup": [entry, entry]}, "system")
        entry["firstPacketLeadingZeroSamples"] = 1601
        with self.assertRaisesRegex(ValueError, "Inconsistent"):
            audio.startup_counters({"portableStartup": [entry]}, "system")

    def test_linux_repeated_short_silences_cannot_replace_eight_seconds_of_tone(self):
        """Window averages must not turn 7.82 seconds of tone into an eight-second proof."""
        self.linux_fixture()
        def interrupt(data):
            payload = bytearray(data)
            for sample in range(0, len(data) // 2, 2000):
                payload[sample * 2:(sample + 160) * 2] = bytes(320)
            return bytes(payload)
        self.rewrite_linux_signal("system", interrupt)
        with self.assertRaisesRegex(ValueError, "less than eight seconds of qualified tone"):
            audio.verify(self.root)

    def test_linux_phase_zero_crossings_are_not_missing_tone(self):
        """Keep a complete exact-eight-second sine valid despite its isolated phase zeros."""
        self.linux_fixture(counts=(128000, 128000))
        receipt = audio.verify(self.root)
        for track in receipt["tracks"].values():
            self.assertEqual(track["qualifiedToneSamples"], 128000)
            self.assertEqual(track["excludedInteriorZeroSamples"], 0)

    def test_linux_declared_startup_silence_is_separate_from_eight_seconds_of_tone(self):
        """A declared half-second prefix leaves eight real seconds for window verification."""
        self.linux_fixture(warnings=[audio.TIMING, "system" + audio.START])
        self.rewrite_linux_signal("system", lambda data: bytes(16000) + data[16000:])
        receipt = audio.verify(self.root)
        self.assertFalse(receipt["completeCapture"])
        self.assertEqual(receipt["tracks"]["system"]["startupSilenceMs"], 500)
        self.assertGreater(receipt["tracks"]["system"]["toneWindows"], 20)

    def test_linux_declared_startup_cannot_replace_required_tone_duration(self):
        """A warning never lowers the eight-second signal duration requirement."""
        self.linux_fixture(warnings=[audio.TIMING, "system" + audio.START])
        self.rewrite_linux_signal("system", lambda data: bytes(64000) + data[64000:])
        with self.assertRaisesRegex(ValueError, "startup"):
            audio.verify(self.root)

    def test_linux_nine_second_fixture_keeps_eight_seconds_of_tone(self):
        """A nine-second receipt may contain a declared one-second prefix, never less tone."""
        self.linux_fixture(counts=(144000, 144000), warnings=[audio.TIMING, "system" + audio.START])
        self.rewrite_linux_signal("system", lambda data: bytes(32000) + data[32000:])
        receipt = audio.verify(self.root)
        self.assertFalse(receipt["completeCapture"])
        self.assertEqual(receipt["tracks"]["system"]["startupSilenceMs"], 1000)
        self.rewrite_linux_signal("system", lambda data: bytes(40000) + data[40000:])
        with self.assertRaisesRegex(ValueError, "startup"):
            audio.verify(self.root)


if __name__ == "__main__":
    unittest.main()
