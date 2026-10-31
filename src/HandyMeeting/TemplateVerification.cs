using System.Diagnostics;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace HandyMeeting;

public static class TemplateVerification
{
    private static readonly NotesConnection Local = new(TextProcessingMode.Local, "http://127.0.0.1:1234/v1/", "fixture-model", "test-key-must-not-be-saved");
    private static TranscriptSnapshot Fixture(bool large = false, bool negative = false)
    {
        var data = new SessionData { Id = "template-fixture", Title = "Vorlagentest", Status = "complete", StartedAt = new(2026, 10, 25, 14, 0, 0, TimeSpan.FromHours(1)) };
        for (var i = 0; i < (large ? 14 : 2); i++)
        {
            var text = negative ? "Wir überlegen, eventuell Freitag zu starten. Dies ist nur ein unverbindlicher Vorschlag, keine Entscheidung. Anna übernimmt heute keine Aufgabe. Niemand hat sich zu einer Aufgabe verpflichtet. We are discussing options only, with no commitments."
                : i % 2 == 0 ? "Wir beschließen, die Veröffentlichung auf Montag zu verschieben. Anna übernimmt den Testbericht bis Freitag." : "Wir besprechen die Audioqualität. Die getrennten Audiospuren bleiben lokal gespeichert.";
            if (large) text += " " + new string('a', 2200);
            data.Parts.Add(new() { Source = i % 2 == 0 ? "system" : "microphone", Text = text, Status = "complete", AudioClosed = true, StartSample = i * 480000L, SampleCount = 480000 });
        }
        return TranscriptSnapshot.Create(data);
    }
    private sealed class Handler : HttpMessageHandler
    {
        public int Calls;
        public int FailOn;
        public string? CrashRoot;
        public List<string> Payloads { get; } = [];
        protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken token)
        {
            Calls++; var body = await request.Content!.ReadAsStringAsync(token); Payloads.Add(body);
            if (Calls == FailOn) return new(HttpStatusCode.ServiceUnavailable) { Content = new StringContent(Local.ApiKey) };
            if (Calls == 2 && CrashRoot is not null) { JsonData.Save(Path.Combine(CrashRoot, "ready.json"), new { Ready = true }); await Task.Delay(Timeout.Infinite, token); }
            using var doc = JsonDocument.Parse(body);
            var instruction = doc.RootElement.GetProperty("messages")[0].GetProperty("content").GetString()!;
            using var input = JsonDocument.Parse(doc.RootElement.GetProperty("messages")[1].GetProperty("content").GetString()!);
            List<MeetingFact> items;
            if (input.RootElement.TryGetProperty("verifiedSummary", out var pair)) items = [JsonSerializer.Deserialize<MeetingFact>(pair[0], JsonData.Options)!];
            else
            {
                var source = input.RootElement.GetProperty("transcript")[0]; var text = source.GetProperty("text").GetString()!; var id = source.GetProperty("sourceId").GetString()!;
                var tasks = instruction.Contains("Extrahiere ausschließlich ausdrücklich zugesagte", StringComparison.Ordinal);
                items = [new() { Text = tasks ? "Den Testbericht erstellen." : "Besprochener Punkt " + id, Owner = tasks ? "Anna" : null, Due = tasks ? "Freitag" : null, Evidence = [new(id, tasks ? "Anna übernimmt den Testbericht bis Freitag." : text[..Math.Min(100, text.Length)])] }];
            }
            var content = JsonSerializer.Serialize(new { items });
            return new(HttpStatusCode.OK) { Content = new StringContent(JsonSerializer.Serialize(new { choices = new[] { new { message = new { content }, finish_reason = "stop" } } }), Encoding.UTF8, "application/json") };
        }
    }
    private static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    public static async Task ChecksAsync(string root)
    {
        var checks = new List<string>();
        void Check(bool ok, string name) { Require(ok, name); checks.Add(name); }
        async Task Reject(Func<Task> action, string name)
        {
            try { await action(); } catch (Exception e) when (e is InvalidDataException or InvalidOperationException or JsonException) { checks.Add(name); return; }
            throw new InvalidOperationException("Expected rejection: " + name);
        }
        var template = AnalysisTemplate.Default();
        template.Steps.Add(new() { Name = "Audioqualität", Prompt = "Beschreibe Aussagen zur Audioqualität auf Englisch." });
        var originalHash = template.Hash; var copy = template.Copy(); copy.Steps[0].Prompt += " Mehr Details.";
        Check(template.Hash == originalHash && copy.Hash != originalHash, "Template copies are independent and prompt changes alter the fingerprint");
        var library = new TemplateLibrary { Templates = [template], SelectedId = template.Id };
        JsonData.Save(Path.Combine(root, "library.json"), library); var read = JsonData.Read<TemplateLibrary>(Path.Combine(root, "library.json"))!; read.Validate();
        Check(read.Templates[0].Hash == template.Hash && read.SelectedId == template.Id, "Named templates and ordered steps survive a disk round trip");
        var many = template.Copy(); for (var i = 4; i < 20; i++) many.Steps.Add(new() { Name = "Schritt " + i, Prompt = "Belegte Themen." }); many.Validate();
        var handler = new Handler(); var snapshot = Fixture(); var session = Path.Combine(root, "twenty");
        var result = await TemplateNotesService.GenerateAsync(session, snapshot, many, Local, false, null, null, CancellationToken.None, handler);
        Check(result.AllSections.Count == 20 && handler.Calls == 20, "Twenty independently configured tabs all execute in template order");
        Check(handler.Payloads.Any(p => p.Contains("Audioqualit", StringComparison.Ordinal)) && handler.Payloads.All(p => !p.Contains(session.Replace("\\", "\\\\"), StringComparison.Ordinal) && !p.Contains("audio_url", StringComparison.Ordinal) && !p.Contains("tools\"", StringComparison.Ordinal)), "Custom prompts reach a text-only transport without session paths, audio or tools");
        Check(handler.Payloads.All(p => p.Contains("exakt kopiertes Originalzitat") && p.Contains("Audioquellen sind keine Personen")), "Immutable grounding and speaker limitations accompany every custom step");
        var loaded = MeetingNotesService.Load(session, result.Run.Id);
        Check(loaded.AllSections.Select(s => s.Id).SequenceEqual(many.Steps.Select(s => s.Id)), "Saved dynamic results retain all step identities and their order");
        var longTemplate = AnalysisTemplate.Default(); longTemplate.Steps = [longTemplate.Steps[0]];
        var longHandler = new Handler(); var longSnapshot = Fixture(large: true);
        var longResult = await TemplateNotesService.GenerateAsync(Path.Combine(root, "long-summary"), longSnapshot, longTemplate, Local, false, null, null, CancellationToken.None, longHandler);
        Check(longResult.AllSections[0].Items.Count <= 8 && longHandler.Calls > MeetingNotesService.Batches(longSnapshot).Count, "Long custom-template summaries condense bounded pairs with original quote validation");
        var plain = TemplateNotesService.PlainText(loaded);
        Check(many.Steps.All(s => plain.Contains(s.Name, StringComparison.Ordinal)) && plain.Contains("[S00001]") && plain.Contains("Anna übernimmt den Testbericht bis Freitag.") && !plain.Contains("\\."), "Copy-all text contains every tab, citations and literal quotes without Markdown escapes");
        var bytes = File.ReadAllBytes(Path.Combine(result.Folder, "sections.json"));
        var noCalls = new Handler(); await TemplateNotesService.GenerateAsync(session, snapshot, many, Local, false, result.Run.Id, null, CancellationToken.None, noCalls);
        Check(noCalls.Calls == 0 && bytes.SequenceEqual(File.ReadAllBytes(Path.Combine(result.Folder, "sections.json"))), "Reopening a completed run performs no network request or result rewrite");
        var modified = many.Copy(); modified.Steps[0].Prompt += " Changed prompt, same step count.";
        await Reject(() => TemplateNotesService.GenerateAsync(session, snapshot, modified, Local, false, result.Run.Id, null, CancellationToken.None, new Handler()), "Changed prompts cannot be mixed into saved work even with identical step IDs and counts");
        var reordered = many.Copy(); reordered.Steps.Reverse();
        await Reject(() => TemplateNotesService.GenerateAsync(session, snapshot, reordered, Local, false, result.Run.Id, null, CancellationToken.None, new Handler()), "Changed step order cannot silently alter a resumed result");
        await Reject(() => TemplateNotesService.GenerateAsync(session, snapshot, many, Local with { Model = "different" }, false, result.Run.Id, null, CancellationToken.None, new Handler()), "Changed models cannot be mixed into saved work");
        var noConsent = Path.Combine(root, "no-consent");
        await Reject(() => TemplateNotesService.GenerateAsync(noConsent, snapshot, many, new(TextProcessingMode.Cloud, "https://example.invalid/v1/", "cloud", ""), false, null, null, CancellationToken.None, new Handler()), "Cloud without per-analysis text consent is rejected before saving a run");
        Check(!Directory.Exists(noConsent), "Rejected cloud requests create no analysis files");
        var cloudHandler = new Handler();
        await TemplateNotesService.GenerateAsync(Path.Combine(root, "simulated-cloud"), snapshot, template, new(TextProcessingMode.Cloud, "https://example.invalid/v1/", "cloud", ""), true, null, null, CancellationToken.None, cloudHandler);
        Check(cloudHandler.Calls == 4, "Cloud text contract verified with a fake handler only; no real provider traffic");
        var work = Path.Combine(root, "resume");
        await Reject(() => TemplateNotesService.GenerateAsync(work, snapshot, template, Local, false, null, null, CancellationToken.None, new Handler { FailOn = 2 }), "Failed second step leaves a resumable run");
        var run = MeetingNotesService.Runs(work).Single(); var checkpoint = Path.Combine(work, "notes", run.Id, $"step-{template.Steps[0].Id}-00000.json"); var confirmed = File.ReadAllBytes(checkpoint);
        var resumedHandler = new Handler(); var resumed = await TemplateNotesService.GenerateAsync(work, snapshot, template, Local, false, run.Id, null, CancellationToken.None, resumedHandler);
        Check(resumedHandler.Calls == 3 && confirmed.SequenceEqual(File.ReadAllBytes(checkpoint)), "Resumption skips confirmed steps and retains their exact bytes");
        Check(!Directory.EnumerateFiles(work, "*.json", SearchOption.AllDirectories).Any(p => File.ReadAllText(p).Contains(Local.ApiKey, StringComparison.Ordinal)), "Checkpoints do not contain API keys or echoed service response bodies");
        var invalidQuote = "{\"items\":[{\"text\":\"Fake\",\"owner\":null,\"due\":null,\"evidence\":[{\"sourceId\":\"S00001\",\"quote\":\"This was not said.\"}]}]}";
        await Reject(() => { SectionValidation.Parse(invalidQuote, snapshot.Sources.ToDictionary(s => s.Id), AnalysisKind.Analysis); return Task.CompletedTask; }, "Custom analysis with invented evidence is rejected");
        await Reject(() => { SectionValidation.Parse("{\"items\":[],\"items\":[]}", snapshot.Sources.ToDictionary(s => s.Id), AnalysisKind.Analysis); return Task.CompletedTask; }, "Duplicate result properties are rejected");
        var hostile = template.Copy(); hostile.Steps[0].Name = "<script>bad</script> ![image](https://example.invalid)";
        var safe = await TemplateNotesService.GenerateAsync(Path.Combine(root, "safe"), snapshot, hostile, Local, false, null, null, CancellationToken.None, new Handler());
        Check(!File.ReadAllText(Path.Combine(safe.Folder, "notes.html")).Contains("<script>") && !File.ReadAllText(Path.Combine(safe.Folder, "notes.md")).Contains("![image]"), "Custom tab names are escaped in HTML and Markdown exports");
        var altered = JsonData.Read<NotesRun>(Path.Combine(resumed.Folder, "run.json"))!; altered.Template!.Steps[0].Prompt += " changed"; JsonData.Save(Path.Combine(resumed.Folder, "run.json"), altered);
        await Reject(() => { MeetingNotesService.Load(work, run.Id); return Task.CompletedTask; }, "Tampered historical prompts fail integrity validation");
        var data = new SessionData { Title = "Complete copy", Status = "recording", ModelId = "asr" };
        for (var i = 0; i < 24; i++) data.Parts.Add(new() { Source = i % 2 == 0 ? "system" : "microphone", StartSample = i * 480000L, SampleCount = 480000, Status = "complete", ModelId = "asr", Text = $"UNIQUE_SECTION_{i:D2}" });
        data.Parts[^1].Status = "transcribing";
        var store = new SessionStore(Path.Combine(root, "copy-fixture"), data); var transcript = store.CopyTranscriptSnapshot();
        Check(Enumerable.Range(0, 24).All(i => transcript.Contains($"UNIQUE_SECTION_{i:D2}")) && !Transcription.RenderLive(store).Contains("UNIQUE_SECTION_00"), "Whole-transcript copy includes both tracks, current text and all history beyond the sixteen-part live view");
        await LeaseWaitChecksAsync(Path.Combine(root, "lease-release"), checks);
        await CrashAsync(Path.Combine(root, "hard-crash")); checks.Add("Hard process termination preserves dynamic-step checkpoints and resumption");
        await RenderAsync(root, snapshot, result); checks.Add("German light and English dark interfaces render with all twenty tabs and exact source navigation");
        JsonData.Save(Path.Combine(root, "template-checks.json"), new { Passed = checks.Count, Checks = checks, CloudTrafficSent = false, SyntheticText = true });
    }
    public static async Task CrashHostAsync(string root)
    {
        var template = AnalysisTemplate.Default(); JsonData.Save(Path.Combine(root, "template.json"), template);
        await TemplateNotesService.GenerateAsync(root, Fixture(), template, Local, false, null, null, CancellationToken.None, new Handler { CrashRoot = root });
    }
    private static async Task CrashAsync(string root)
    {
        Directory.CreateDirectory(root);
        var info = new ProcessStartInfo(Environment.ProcessPath!) { UseShellExecute = false, CreateNoWindow = true }; info.ArgumentList.Add("--template-crash-host"); info.ArgumentList.Add(root);
        using var child = Process.Start(info)!; using var job = new WorkerJob(child); var clock = Stopwatch.StartNew();
        while (!File.Exists(Path.Combine(root, "ready.json"))) { Require(!child.HasExited && clock.Elapsed.TotalSeconds < 30, "Crash fixture did not checkpoint"); await Task.Delay(50); }
        child.Kill(true); await child.WaitForExitAsync();
        Require(child.HasExited, "Owned crash fixture process must exit before testing lease release");
        var leaseWait = Stopwatch.StartNew();
        await WaitForReleasedTestLeaseAsync(Path.Combine(root, "notes", "processing.lock"), TimeSpan.FromSeconds(5));
        JsonData.Save(Path.Combine(root, "lease-release.json"), new { OwnChildExited = child.HasExited, ChildId = child.Id, WaitMilliseconds = leaseWait.Elapsed.TotalMilliseconds, DeadlineMilliseconds = 5000, LockDeleted = false });
        var template = JsonData.Read<AnalysisTemplate>(Path.Combine(root, "template.json"))!; var run = MeetingNotesService.Runs(root).Single();
        var path = Path.Combine(root, "notes", run.Id, $"step-{template.Steps[0].Id}-00000.json"); var before = File.ReadAllBytes(path);
        var handler = new Handler(); var result = await TemplateNotesService.GenerateAsync(root, Fixture(), template, Local, false, run.Id, null, CancellationToken.None, handler);
        Require(result.Run.Status == "complete" && handler.Calls == 2 && before.SequenceEqual(File.ReadAllBytes(path)), "Crash lost a confirmed dynamic step");
    }
    private static bool IsLeaseContention(IOException error) => error.HResult == unchecked((int)0x80070020) || error.HResult == unchecked((int)0x80070021);
    // Test harness only: the owned child has already exited. Probe the same lease
    // semantics as production, without deleting the lock or retrying GenerateAsync.
    /// <summary>Await only a confirmed-exited test child's sharing violation under the existing bound, without deleting or modifying its lease.</summary>
    internal static async Task WaitForReleasedTestLeaseAsync(string path, TimeSpan deadline)
    {
        if (deadline <= TimeSpan.Zero) throw new ArgumentOutOfRangeException(nameof(deadline));
        var clock = Stopwatch.StartNew(); IOException? contention = null;
        while (true)
        {
            if (clock.Elapsed >= deadline) throw new TimeoutException("The test lease remained held past its bounded release deadline.", contention);
            try
            {
                using var probe = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None);
                return;
            }
            catch (IOException error) when (IsLeaseContention(error))
            {
                contention = error;
                var remaining = deadline - clock.Elapsed;
                if (remaining <= TimeSpan.Zero) throw new TimeoutException("The test lease remained held past its bounded release deadline.", error);
                await Task.Delay(remaining < TimeSpan.FromMilliseconds(25) ? remaining : TimeSpan.FromMilliseconds(25));
            }
        }
    }
    private static async Task LeaseWaitChecksAsync(string root, List<string> checks)
    {
        Directory.CreateDirectory(root);
        var releasedPath = Path.Combine(root, "released.lock");
        var original = Encoding.UTF8.GetBytes("owned synthetic lease; retain these bytes");
        Task wait;
        using (var held = new FileStream(releasedPath, FileMode.CreateNew, FileAccess.ReadWrite, FileShare.None))
        {
            held.Write(original); held.Flush(true);
            wait = WaitForReleasedTestLeaseAsync(releasedPath, TimeSpan.FromSeconds(2));
            await Task.Delay(75);
            Require(!wait.IsCompleted, "A currently held test lease must not be treated as released");
        }
        await wait;
        Require(original.SequenceEqual(File.ReadAllBytes(releasedPath)), "Waiting for release must not alter the original lease file");
        checks.Add("A temporarily held test lease is awaited until release without modifying its bytes");
        var activePath = Path.Combine(root, "active.lock");
        using (var held = new FileStream(activePath, FileMode.CreateNew, FileAccess.ReadWrite, FileShare.None))
        {
            held.Write(original); held.Flush(true);
            var timedOut = false;
            try { await WaitForReleasedTestLeaseAsync(activePath, TimeSpan.FromMilliseconds(150)); }
            catch (TimeoutException error) when (error.InnerException is IOException io && IsLeaseContention(io)) { timedOut = true; }
            Require(timedOut && !held.SafeFileHandle.IsClosed, "An actively held lease must cause a deadline failure and remain owned by its holder");
        }
        Require(original.SequenceEqual(File.ReadAllBytes(activePath)), "An active lease deadline must not delete or rewrite its file");
        checks.Add("An actively held test lease fails at its deadline and is never ignored or deleted");
        var missingFailed = false;
        try { await WaitForReleasedTestLeaseAsync(Path.Combine(root, "missing.lock"), TimeSpan.FromSeconds(2)); }
        catch (FileNotFoundException) { missingFailed = true; }
        Require(missingFailed, "Lease release waiting must not swallow missing-file or other unrelated I/O failures");
        checks.Add("Lease release waiting retries only sharing and locking violations, preserving unrelated I/O failures");
    }
    private static async Task RenderAsync(string root, TranscriptSnapshot snapshot, NotesResult result)
    {
        foreach (var dark in new[] { false, true })
        {
            Ui.Change(dark, dark); var window = new NotesWindow(root, snapshot); window.ShowResult(result);
            var content = (FrameworkElement)window.Content; content.Measure(new Size(1240, 900)); content.Arrange(new Rect(0, 0, 1240, 900)); content.UpdateLayout();
            var evidence = result.AllSections[0].Items[0].Evidence[0]; window.NavigateToEvidence(evidence); await Task.Yield(); content.UpdateLayout();
            Require(window.SelectedEvidenceForTest == evidence.Quote, "Source click must select exact quote");
            var tabs = (TabControl)window.FindName("ResultTabs"); Require(tabs.Items.Count == 20 && tabs.Template.FindName("AddStepButton", tabs) is Button, "Twenty tabs and a separate Add button must be available");
            Require((string)((Button)tabs.Template.FindName("EditStepButton", tabs)).Content == (dark ? "Edit instructions" : "Auftrag bearbeiten"), "Visible edit action must follow the interface language");
            Require((string)((Button)window.FindName("GenerateButton")).Content == (dark ? "Create new analysis" : "Neue Auswertung erstellen"), "Interface language did not apply");
            var bitmap = new RenderTargetBitmap(1240, 940, 96, 96, PixelFormats.Pbgra32); var background = new DrawingVisual();
            using (var dc = background.RenderOpen()) dc.DrawRectangle(window.Background, null, new Rect(0, 0, 1240, 940)); bitmap.Render(background); bitmap.Render(content);
            var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(bitmap)); using (var file = File.Create(Path.Combine(root, dark ? "templates-en-dark.png" : "templates-de-light.png"))) encoder.Save(file);
            ((ComboBox)window.FindName("ModeCombo")).SelectedIndex = 1;
            var model = (ComboBox)window.FindName("ModelInput"); model.ApplyTemplate();
            var editor = (TextBox)model.Template.FindName("PART_EditableTextBox", model); editor.Text = "first-model";
            var consent = (CheckBox)window.FindName("CloudConsent"); consent.IsChecked = true; editor.Text = "pasted-model";
            Require(consent.IsChecked == false, "Pasting a new model must clear prior cloud consent");
            window.Close();
            var main = new MainWindow(); var mainContent = (FrameworkElement)main.Content;
            mainContent.Measure(new Size(1120, 900)); mainContent.Arrange(new Rect(0, 0, 1120, 900)); mainContent.UpdateLayout();
            Require((string)((Button)main.FindName("CopyTranscriptButton")).Content == (dark ? "Copy transcript" : "Transkript kopieren"), "Transcript button language did not apply");
            Require(((FrameworkElement)main.FindName("CancelButton")).ToolTip is string tip && tip.Contains(dark ? "recording continues" : "Audioaufnahme geht weiter", StringComparison.Ordinal), "Pause tooltip must explain continued recording");
            var mainBitmap = new RenderTargetBitmap(1120, 940, 96, 96, PixelFormats.Pbgra32); var mainBackground = new DrawingVisual();
            using (var dc = mainBackground.RenderOpen()) dc.DrawRectangle(main.Background, null, new Rect(0, 0, 1120, 940)); mainBitmap.Render(mainBackground); mainBitmap.Render(mainContent);
            var mainEncoder = new PngBitmapEncoder(); mainEncoder.Frames.Add(BitmapFrame.Create(mainBitmap)); using (var file = File.Create(Path.Combine(root, dark ? "main-en-dark.png" : "main-de-light.png"))) mainEncoder.Save(file);
            // No recording is started in this headless rendering check.
            var closed = false; main.Closed += (_, _) => closed = true; main.Close(); Require(closed, "A window checking local setup must still close without an active recording");
        }
        Ui.Change(false, false);
    }
    public static async Task LocalAsync(string root, string model)
    {
        var connection = Local with { Model = model, ApiKey = "" }; var template = AnalysisTemplate.Default();
        template.Steps.Add(new() { Name = "English audio review", Prompt = "Write in English. Extract the statements about audio quality and retention of separate audio tracks. Do not invent speaker identities." });
        var watch = Stopwatch.StartNew(); var result = await TemplateNotesService.GenerateAsync(root, Fixture(), template, connection, false, null, null, CancellationToken.None);
        Require(result.AllSections.All(s => s.Items.Count > 0), "Real local model missed a reference section");
        Require(result.AllSections[1].Items.Count == 1 && result.AllSections[1].Items[0].Evidence.Any(e => e.Quote.Contains("Wir beschließen", StringComparison.Ordinal)), "Real local model confused decisions with tasks or existing states");
        Require(result.AllSections[2].Items.Any(f => f.Owner == "Anna" && f.Due == "Freitag"), "Real local model missed the explicitly assigned task");
        var negative = AnalysisTemplate.Default(); negative.Steps.RemoveAt(0);
        var declined = await TemplateNotesService.GenerateAsync(Path.Combine(root, "negative"), Fixture(negative: true), negative, connection, false, null, null, CancellationToken.None);
        Require(declined.AllSections.All(s => s.Items.Count == 0), "Real local model turned nonbinding suggestions into decisions or tasks");
        JsonData.Save(Path.Combine(root, "template-local.json"), new { Passed = true, Model = model, Seconds = watch.Elapsed.TotalSeconds, PositiveSections = result.AllSections.Select(s => new { s.Name, Count = s.Items.Count }), NegativePassed = true, result.Folder, NegativeFolder = declined.Folder, CloudTrafficSent = false, SyntheticText = true });
    }
}
