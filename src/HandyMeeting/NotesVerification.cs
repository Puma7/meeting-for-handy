using System.Diagnostics;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace HandyMeeting;

public static class NotesVerification
{
    private static readonly JsonSerializerOptions Camel = new() { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };
    private static readonly NotesConnection Local = new(TextProcessingMode.Local, "http://127.0.0.1:1234/v1/", "fixture-model", "never-write-this-test-key");
    private static void Require(bool condition, string name) { if (!condition) throw new InvalidOperationException(name); }
    private static TranscriptSnapshot Fixture(int count = 2, bool large = false)
    {
        var data = new SessionData { Id = "test-meeting", Title = "Produktbesprechung", Status = "complete", ModelId = "fixture-asr" };
        for (var i = 0; i < count; i++)
        {
            var text = i % 2 == 0 ? "Wir beschließen, die Veröffentlichung auf Montag zu verschieben. Anna übernimmt den Testbericht bis Freitag." : "Ich prüfe die Tonspuren und melde mich morgen. Die Quelldateien bleiben erhalten.";
            if (large) text = $"Abschnitt {i:D3}. " + text + " " + new string('a', 2200);
            data.Parts.Add(new() { File = $"{(i % 2 == 0 ? "system" : "microphone")}-{i * 480000L:D12}.wav", Source = i % 2 == 0 ? "system" : "microphone", StartSample = i * 480000L, SampleCount = 480000, AudioClosed = true, Status = "complete", ModelId = "fixture-asr", Text = text });
        }
        return TranscriptSnapshot.Create(data);
    }
    private sealed class ModelHandler : HttpMessageHandler
    {
        public int Calls { get; private set; }
        public int FailOn { get; init; }
        public string? BlockAtSecondCallRoot { get; init; }
        public List<string> Payloads { get; } = [];
        public List<Uri> Destinations { get; } = [];
        protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken token)
        {
            Calls++; Destinations.Add(request.RequestUri!);
            var body = request.Content is null ? "" : await request.Content.ReadAsStringAsync(token); Payloads.Add(body);
            if (FailOn == Calls) return new(HttpStatusCode.ServiceUnavailable) { Content = new StringContent("backend echoed a key: " + Local.ApiKey) };
            if (BlockAtSecondCallRoot is not null && Calls == 2)
            {
                JsonData.Save(Path.Combine(BlockAtSecondCallRoot, "notes-crash-ready.json"), new { ready = true });
                await Task.Delay(Timeout.Infinite, token);
            }
            using var input = JsonDocument.Parse(body);
            var messages = input.RootElement.GetProperty("messages");
            Require(messages.GetArrayLength() == 2 && messages[0].GetProperty("role").GetString() == "system" && messages[1].GetProperty("role").GetString() == "user", "Only system instructions and text data are sent");
            Require(!input.RootElement.TryGetProperty("tools", out _), "No tool execution is exposed to transcript content");
            using var data = JsonDocument.Parse(messages[1].GetProperty("content").GetString()!);
            var result = new MeetingNotes();
            if (data.RootElement.TryGetProperty("verifiedSummary", out var summaries))
            {
                var first = JsonSerializer.Deserialize<MeetingFact>(summaries[0], JsonData.Options)!;
                result.Summary.Add(new() { Text = "Verdichteter Überblick: " + first.Text, Evidence = first.Evidence });
            }
            else
            {
                foreach (var source in data.RootElement.GetProperty("transcript").EnumerateArray().Take(3))
                {
                    var id = source.GetProperty("sourceId").GetString()!; var text = source.GetProperty("text").GetString()!;
                    result.Summary.Add(new() { Text = "Besprochener Punkt " + id, Evidence = [new(id, text[..Math.Min(text.Length, 60)])] });
                    const string decision = "Wir beschließen, die Veröffentlichung auf Montag zu verschieben.";
                    const string todo = "Anna übernimmt den Testbericht bis Freitag.";
                    if (text.Contains(decision, StringComparison.Ordinal)) result.Decisions.Add(new() { Text = "Die Veröffentlichung wird auf Montag verschoben.", Evidence = [new(id, decision)] });
                    if (text.Contains(todo, StringComparison.Ordinal)) result.Todos.Add(new() { Text = "Den Testbericht übernehmen.", Owner = "Anna", Due = "Freitag", Evidence = [new(id, todo)] });
                }
            }
            var json = JsonSerializer.Serialize(result, Camel);
            return new(HttpStatusCode.OK) { Content = new StringContent(JsonSerializer.Serialize(new { choices = new[] { new { finish_reason = "stop", message = new { role = "assistant", content = json } } } }), Encoding.UTF8, "application/json") };
        }
    }
    public static async Task CrashHostAsync(string root)
    {
        var handler = new ModelHandler { BlockAtSecondCallRoot = root };
        await MeetingNotesService.GenerateAsync(root, Fixture(6, true), Local, false, null, null, CancellationToken.None, handler);
    }
    public static async Task ChecksAsync(string root)
    {
        var passed = new List<string>();
        void Check(bool condition, string name) { Require(condition, name); passed.Add(name); }
        void Reject(Action action, string name)
        {
            try { action(); } catch (Exception error) when (error is InvalidDataException or InvalidOperationException or JsonException) { passed.Add(name); return; }
            throw new InvalidOperationException("Nicht abgelehnt: " + name);
        }
        var snapshot = Fixture(); snapshot.Validate();
        var allowed = snapshot.Sources.ToDictionary(s => s.Id);
        var valid = new MeetingNotes { Todos = [new() { Text = "Testbericht erstellen.", Owner = "Anna", Due = "Freitag", Evidence = [new(snapshot.Sources[0].Id, "Anna übernimmt den Testbericht bis Freitag.")] }] };
        var validJson = JsonSerializer.Serialize(valid, Camel);
        Check(NotesValidation.Parse(validJson, allowed).Todos.Count == 1, "Exact source evidence, owner and literal due date are accepted");
        Reject(() => NotesValidation.Parse("{}", allowed), "Missing result sections are rejected");
        Reject(() => NotesValidation.Parse("{\"summary\":[],\"Summary\":[],\"decisions\":[],\"todos\":[]}", allowed), "Duplicate case-insensitive result fields are rejected");
        Reject(() => NotesValidation.Parse(validJson.Replace(snapshot.Sources[0].Id, "S99999"), allowed), "Unknown source identifiers are rejected");
        Reject(() => NotesValidation.Parse(validJson.Replace("Freitag.", "Sonntag."), allowed), "Fabricated source quotes are rejected");
        valid.Todos[0].Owner = "Erik";
        Reject(() => NotesValidation.Validate(valid, allowed), "Invented task owners are rejected");
        valid.Todos[0].Owner = "Anna"; valid.Todos[0].Due = "2027-01-01";
        Reject(() => NotesValidation.Validate(valid, allowed), "Invented absolute due dates are rejected");
        Reject(() => new NotesConnection(TextProcessingMode.Local, "https://external.example/v1", "model", "").BaseUri(), "Local mode cannot send text to an external host");
        Reject(() => new NotesConnection(TextProcessingMode.Local, "http://localhost.external.example/v1", "model", "").BaseUri(), "Lookalike localhost hostnames are rejected");
        Reject(() => new NotesConnection(TextProcessingMode.Local, "http://127.0.0.1/v1", "model:cloud", "").BaseUri(), "Known cloud-model tags are blocked in local mode");
        Reject(() => new NotesConnection(TextProcessingMode.Cloud, "http://external.example/v1", "model", "").BaseUri(), "Cloud processing requires HTTPS");
        Reject(() => new NotesConnection(TextProcessingMode.Cloud, "https://key@external.example/v1?secret=1", "model", "").BaseUri(), "Credentials and query parameters in provider URLs are rejected");
        Reject(() => { using var client = new TextModelClient(new(TextProcessingMode.Cloud, "https://external.example/v1", "model", "key"), false); }, "Cloud processing requires explicit consent");
        Check(new NotesConnection(TextProcessingMode.Local, "http://localhost:1234/v1", "model", "").BaseUri().Host == "127.0.0.1", "Localhost is pinned to literal loopback");
        var protectedKey = NotesSettings.Protect(Local.ApiKey);
        Check(!protectedKey.Contains(Local.ApiKey, StringComparison.Ordinal) && NotesSettings.Unprotect(protectedKey) == Local.ApiKey, "API keys use Windows user-bound encryption at rest");

        var unicode = new SessionData { Id = "unicode", Status = "complete", Parts = [new() { Status = "complete", AudioClosed = true, Text = new string('x', 2399) + "😀" + new string('y', 100) }] };
        var unicodeSnapshot = TranscriptSnapshot.Create(unicode);
        Check(string.Concat(unicodeSnapshot.Sources.Select(s => s.Text)) == unicode.Parts[0].Text && unicodeSnapshot.Sources.All(s => !char.IsHighSurrogate(s.Text[^1])), "Long sources preserve Unicode surrogate pairs and all original characters");
        Reject(() => TranscriptSnapshot.Create(new SessionData { Status = "recording" }), "Live or unfinished transcripts cannot be postprocessed");

        var sessionFolder = Path.Combine(root, "basic"); Directory.CreateDirectory(sessionFolder);
        File.WriteAllText(Path.Combine(sessionFolder, "audio-secret.wav"), "AUDIO_BYTES_MUST_NOT_BE_SENT");
        var handler = new ModelHandler();
        var result = await MeetingNotesService.GenerateAsync(sessionFolder, snapshot, Local, false, null, null, CancellationToken.None, handler);
        Check(result.Run.Status == "complete" && result.Notes.Decisions.Count == 1 && result.Notes.Todos.Count == 1, "Summary, decisions and tasks are persisted with original evidence");
        Check(handler.Payloads.All(p => !p.Contains("AUDIO_BYTES_MUST_NOT_BE_SENT") && !p.Contains(sessionFolder, StringComparison.Ordinal) && !p.Contains("audio-secret.wav")), "Requests contain text and source IDs without audio, audio filenames or local paths");
        Check(handler.Destinations.All(u => u.IsLoopback) && handler.Payloads.All(p => !p.Contains("input_audio") && !p.Contains("file_id")), "Local requests have only a loopback destination and no audio payload fields");
        var loaded = MeetingNotesService.Load(sessionFolder, result.Run.Id);
        Check(loaded.Original.Hash == snapshot.Hash && loaded.Notes.Todos[0].Evidence[0].Quote.Contains("Anna"), "Saved results reopen without a running AI service");
        Check(File.Exists(Path.Combine(result.Folder, "notes.html")) && File.Exists(Path.Combine(result.Folder, "source.md")), "Standalone HTML and Markdown exports include original-source navigation");
        Check(!File.ReadAllText(Path.Combine(result.Folder, "run.json")).Contains(Local.ApiKey), "Run metadata never persists API credentials");

        var cloudHandler = new ModelHandler();
        var cloud = await MeetingNotesService.GenerateAsync(Path.Combine(root, "cloud-fixture"), snapshot, new(TextProcessingMode.Cloud, "https://provider.example/v1", "fixture", "test-cloud-key"), true, null, null, CancellationToken.None, cloudHandler);
        Check(cloud.Run.Mode == TextProcessingMode.Cloud && cloudHandler.Destinations.All(u => u.Scheme == "https") && cloudHandler.Payloads.All(p => !p.Contains("AUDIO_BYTES_MUST_NOT_BE_SENT")), "Explicitly approved cloud path uses the same text-only request contract (simulated provider)");

        var resumeFolder = Path.Combine(root, "resume"); var longSnapshot = Fixture(6, true); var failing = new ModelHandler { FailOn = 2 };
        try { await MeetingNotesService.GenerateAsync(resumeFolder, longSnapshot, Local, false, null, null, CancellationToken.None, failing); throw new InvalidOperationException("Der simulierte Dienstfehler blieb unbemerkt."); }
        catch (InvalidOperationException error) when (error.Message.Contains("HTTP 503")) { }
        var paused = MeetingNotesService.Runs(resumeFolder).Single(); var firstCheckpoint = Path.Combine(resumeFolder, "notes", paused.Id, "map-00000.json");
        var before = SHA256.HashData(File.ReadAllBytes(firstCheckpoint));
        Check(paused.Status == "paused" && paused.CompletedBatches == 1 && !File.ReadAllText(Path.Combine(resumeFolder, "notes", paused.Id, "run.json")).Contains(Local.ApiKey), "Backend errors preserve the last validated checkpoint without logging provider error bodies or secrets");
        var resumedHandler = new ModelHandler();
        var resumed = await MeetingNotesService.GenerateAsync(resumeFolder, longSnapshot, Local, false, paused.Id, null, CancellationToken.None, resumedHandler);
        Check(resumed.Run.Status == "complete" && before.SequenceEqual(SHA256.HashData(File.ReadAllBytes(firstCheckpoint))) && resumedHandler.Calls == MeetingNotesService.Batches(longSnapshot).Count - 1, "Resume skips acknowledged requests and preserves their checkpoint bytes");
        var completePlan = File.ReadAllBytes(Path.Combine(resumed.Folder, "run.json")); var unused = new ModelHandler();
        _ = await MeetingNotesService.GenerateAsync(resumeFolder, longSnapshot, Local, false, paused.Id, null, CancellationToken.None, unused);
        Check(unused.Calls == 0 && completePlan.SequenceEqual(File.ReadAllBytes(Path.Combine(resumed.Folder, "run.json"))), "Reopening a completed run neither regenerates results nor rewrites metadata");

        var hierarchicalHandler = new ModelHandler(); var hierarchicalSnapshot = Fixture(14, true);
        var hierarchical = await MeetingNotesService.GenerateAsync(Path.Combine(root, "long-meeting"), hierarchicalSnapshot, Local, false, null, null, CancellationToken.None, hierarchicalHandler);
        Check(hierarchical.Notes.Summary.Count <= 8 && hierarchicalHandler.Calls > MeetingNotesService.Batches(hierarchicalSnapshot).Count, "Long meetings use bounded requests and a checkpointed hierarchical summary");
        Check(hierarchical.Notes.Decisions.Count == 1 && hierarchical.Notes.Decisions[0].Evidence.Count > 1, "Exact duplicates are merged while multiple original references remain available");

        var originalBefore = File.ReadAllBytes(Path.Combine(result.Folder, "notes.md"));
        using (var cancellation = new CancellationTokenSource())
        {
            cancellation.Cancel();
            try { await MeetingNotesService.GenerateAsync(sessionFolder, snapshot, Local, false, null, null, cancellation.Token, new ModelHandler()); throw new InvalidOperationException("Abbruch wurde ignoriert."); }
            catch (OperationCanceledException) { }
        }
        Check(originalBefore.SequenceEqual(File.ReadAllBytes(Path.Combine(result.Folder, "notes.md"))) && MeetingNotesService.Runs(sessionFolder).Count == 2, "Cancelled new runs retain previous complete versions");

        var malicious = Fixture(); malicious = malicious with { Title = "<script>alert(1)</script>" };
        malicious.Sources[0] = malicious.Sources[0] with { Text = "![remote](https://example.invalid/track) <script>alert(2)</script> Originaltext bleibt Dateninhalt." };
        malicious = malicious with { Hash = TranscriptSnapshot.CalculateHash(malicious.SessionId, malicious.Sources) };
        var escaped = await MeetingNotesService.GenerateAsync(Path.Combine(root, "escaping"), malicious, Local, false, null, null, CancellationToken.None, new ModelHandler());
        Check(!File.ReadAllText(Path.Combine(escaped.Folder, "notes.html")).Contains("<script>") && !File.ReadAllText(Path.Combine(escaped.Folder, "source.md")).Contains("![remote]"), "Generated exports escape transcript HTML and remote-image Markdown");
        var changed = JsonData.Read<TranscriptSnapshot>(Path.Combine(result.Folder, "source.json"))!;
        changed.Sources[0] = changed.Sources[0] with { Text = "Veränderter Quelltext" }; JsonData.Save(Path.Combine(result.Folder, "source.json"), changed);
        Reject(() => MeetingNotesService.Load(sessionFolder, result.Run.Id), "Changed original snapshots fail their integrity check");

        await CheckRedirectAsync(); passed.Add("Real loopback HTTP redirects are not followed to another destination");
        await CheckProcessCrashAsync(Path.Combine(root, "process-crash")); passed.Add("Hard process crash releases the notes lease and preserves validated progress for resumption");
        await RenderWindowAsync(root, snapshot, loaded); passed.Add("WPF results render and source clicks select the exact original quote");
        JsonData.Save(Path.Combine(root, "notes-checks.json"), new { Passed = passed.Count, Checks = passed, CloudTrafficSent = false, RealAudioUsed = false });
    }
    private static async Task CheckRedirectAsync()
    {
        using var target = new TcpListener(IPAddress.Loopback, 0); target.Start();
        using var origin = new TcpListener(IPAddress.Loopback, 0); origin.Start();
        var targetPort = ((IPEndPoint)target.LocalEndpoint).Port; var originPort = ((IPEndPoint)origin.LocalEndpoint).Port;
        var serving = Task.Run(async () =>
        {
            using var socket = await origin.AcceptTcpClientAsync(); await using var stream = socket.GetStream();
            using var reader = new StreamReader(stream, Encoding.ASCII, false, 1024, true);
            string? line; while (!string.IsNullOrEmpty(line = await reader.ReadLineAsync())) { }
            var bytes = Encoding.ASCII.GetBytes($"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{targetPort}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            await stream.WriteAsync(bytes);
        });
        using var client = new TextModelClient(new(TextProcessingMode.Local, $"http://127.0.0.1:{originPort}/v1/", "model", "test"), false);
        try { await client.ModelsAsync(CancellationToken.None); throw new InvalidOperationException("Weiterleitung wurde akzeptiert."); }
        catch (InvalidOperationException error) when (error.Message.Contains("HTTP 302")) { }
        await serving; Require(!target.Pending(), "Der lokale Client ist der Weiterleitung gefolgt.");
    }
    /// <summary>Resume once after the owned crash child and its exclusive fixture lease have ended; retain confirmed bytes.</summary>
    private static async Task CheckProcessCrashAsync(string root)
    {
        Directory.CreateDirectory(root);
        var info = new ProcessStartInfo(Environment.ProcessPath!) { UseShellExecute = false, CreateNoWindow = true };
        info.ArgumentList.Add("--notes-crash-host"); info.ArgumentList.Add(root);
        using var child = Process.Start(info) ?? throw new InvalidOperationException("Testprozess konnte nicht starten.");
        using var job = new WorkerJob(child);
        var clock = Stopwatch.StartNew();
        while (!File.Exists(Path.Combine(root, "notes-crash-ready.json")))
        { Require(!child.HasExited && clock.Elapsed.TotalSeconds < 30, "Testprozess erzeugte keinen bestätigten Arbeitsstand."); await Task.Delay(50); }
        child.Kill(true); await child.WaitForExitAsync();
        Require(child.HasExited, "Owned notes crash fixture must exit before testing lease release");
        var leaseWait = Stopwatch.StartNew();
        await TemplateVerification.WaitForReleasedTestLeaseAsync(Path.Combine(root, "notes", "processing.lock"), TimeSpan.FromSeconds(5));
        JsonData.Save(Path.Combine(root, "lease-release.json"), new { OwnChildExited = child.HasExited, ChildId = child.Id, WaitMilliseconds = leaseWait.Elapsed.TotalMilliseconds, DeadlineMilliseconds = 5000, LockDeleted = false });
        var run = MeetingNotesService.Runs(root).Single(); var path = Path.Combine(root, "notes", run.Id, "map-00000.json"); var prefix = File.ReadAllBytes(path);
        var resumed = await MeetingNotesService.GenerateAsync(root, Fixture(6, true), Local, false, run.Id, null, CancellationToken.None, new ModelHandler());
        Require(resumed.Run.Status == "complete" && prefix.SequenceEqual(File.ReadAllBytes(path)), "Nach Prozessabbruch fehlt ein bereits bestätigter Auswertungsabschnitt.");
    }
    private static async Task RenderWindowAsync(string root, TranscriptSnapshot snapshot, NotesResult result)
    {
        var uiFolder = Path.Combine(root, "ui-fixture");
        var preview = await MeetingNotesService.GenerateAsync(uiFolder, snapshot, Local, false, null, null, CancellationToken.None, new ModelHandler());
        var window = new NotesWindow(uiFolder, snapshot); window.ShowResult(preview);
        var content = (FrameworkElement)window.Content;
        content.Measure(new Size(1192, 904)); content.Arrange(new Rect(0, 0, 1192, 904)); content.UpdateLayout();
        var evidence = result.Notes.Todos[0].Evidence[0]; window.NavigateToEvidence(evidence);
        await Task.Yield(); content.UpdateLayout();
        Require(window.SelectedEvidenceForTest == evidence.Quote, "Quellensprung markiert nicht das Originalzitat.");
        var bitmap = new RenderTargetBitmap(1240, 940, 96, 96, PixelFormats.Pbgra32);
        var background = new DrawingVisual(); using (var context = background.RenderOpen()) context.DrawRectangle(window.Background, null, new Rect(0, 0, 1240, 940));
        bitmap.Render(background); bitmap.Render(content);
        var pixels = new byte[1240 * 940 * 4]; bitmap.CopyPixels(pixels, 1240 * 4, 0);
        var colors = new HashSet<int>(); for (var i = 0; i < pixels.Length; i += 16) colors.Add(BitConverter.ToInt32(pixels, i));
        Require(colors.Count > 50, "Die UI-Vorschau enthält keine gerenderten Bedienelemente.");
        var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(bitmap));
        using (var file = File.Create(Path.Combine(root, "notes-ui.png"))) encoder.Save(file);
        window.Close();
    }
    public static async Task LocalSmokeAsync(string root, string model)
    {
        Require(!string.IsNullOrWhiteSpace(model), "Für den lokalen Modelltest ist eine Modellkennung erforderlich.");
        var connection = new NotesConnection(TextProcessingMode.Local, "http://127.0.0.1:1234/v1/", model, "");
        var result = await MeetingNotesService.GenerateAsync(root, Fixture(), connection, false, null, null, CancellationToken.None);
        Require(result.Notes.Summary.Count > 0 && result.Notes.Decisions.Count > 0 && result.Notes.Todos.Count > 0, "Das lokale Modell hat nicht alle Referenzbereiche erkannt.");
        Require(result.Notes.Todos.Any(t => t.Owner == "Anna" && t.Due == "Freitag"), "Der belegte Referenzauftrag fehlt im lokalen Ergebnis.");
        var negative = new SessionData { Id = "negative-fixture", Title = "Unverbindliche Diskussion", Status = "complete", Parts = [new() { Status = "complete", AudioClosed = true, SampleCount = 480000, Text = "Wir sprechen über einen möglichen Start am Freitag. Der Vorschlag ist ausdrücklich nicht beschlossen. Anna hat keine Aufgabe übernommen. Es gibt heute keine verbindlichen Entscheidungen und keine zugewiesenen To-dos. We are discussing options only; nobody has committed to an action." }] };
        var declined = await MeetingNotesService.GenerateAsync(Path.Combine(root, "negative"), TranscriptSnapshot.Create(negative), connection, false, null, null, CancellationToken.None);
        Require(declined.Notes.Decisions.Count == 0 && declined.Notes.Todos.Count == 0, "Das lokale Modell hat einen unverbindlichen Vorschlag zu einer Entscheidung oder Aufgabe gemacht.");
        JsonData.Save(Path.Combine(root, "notes-local-smoke.json"), new { Passed = true, Model = model, Summary = result.Notes.Summary.Count, Decisions = result.Notes.Decisions.Count, Todos = result.Notes.Todos.Count, ResultFolder = result.Folder, NegativeCasePassed = true, NegativeResultFolder = declined.Folder, CloudTrafficSent = false });
    }
}
