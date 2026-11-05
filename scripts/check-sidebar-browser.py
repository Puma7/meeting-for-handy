"""Check actual Sidebar/RecordingStatus layout with installed Playwright 1.57.0.

Run from the repository: python scripts/check-sidebar-browser.py --browser msedge
Requires existing desktop npm dependencies, Node and an installed browser. This
script never installs dependencies/browsers, calls native IPC, or records audio.
It keeps a fresh small .build/sidebar-browser-* proof folder; existing files and
user/browser sessions are untouched. This is an explicit local check, not a
claim that the existing CI executes Playwright.
"""
import argparse
from datetime import datetime, timezone
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from importlib.metadata import version
import json
from pathlib import Path
import re
import subprocess
import threading
import uuid


def main():
    """Build only an isolated synthetic fixture, then test real browser layout."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", choices=["msedge", "chrome", "chromium"], default="msedge")
    args = parser.parse_args()
    installed = version("playwright")
    if installed != "1.57.0":
        raise RuntimeError("Use existing Playwright 1.57.0; no installation is performed.")
    from playwright.sync_api import sync_playwright, expect

    root = Path(__file__).resolve().parents[1]
    desktop = root / "apps/desktop"
    node_modules = desktop / "node_modules"
    vite = node_modules / "vite/bin/vite.js"
    if not vite.is_file():
        raise RuntimeError("The existing locked desktop dependencies are required.")
    parent = (root / ".build").resolve()
    parent.mkdir(exist_ok=True)
    identity = uuid.uuid4().hex
    output = parent / ("sidebar-browser-" + identity)
    assert output.resolve().parent == parent and not output.exists()
    output.mkdir()
    fixture = output / "fixture"
    fixture.mkdir()
    (fixture / "index.html").write_text(
        '<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" '
        'content="width=device-width,initial-scale=1"><link rel="icon" href="data:,">'
        '<title>Synthetic sidebar browser check</title></head><body><div id="root"></div>'
        '<script type="module" src="/main.jsx"></script></body></html>', encoding="utf8")
    source = desktop.as_posix()
    imports = "\n".join([
        'import React from "react";',
        'import {createRoot} from "react-dom/client";',
        f'import Sidebar from {json.dumps(source + "/src/views/Sidebar.tsx")};',
        f'import RecordingStatus from {json.dumps(source + "/src/views/RecordingStatus.tsx")};',
        f'import {{de,en}} from {json.dumps(source + "/src/i18n.ts")};',
        f'import {json.dumps(source + "/src/styles.css")};',
    ])
    body = r'''
const params = new URLSearchParams(location.search);
const locale = params.get("locale") === "en" ? "en" : "de";
const theme = params.get("theme") === "dark" ? "dark" : "light";
document.documentElement.dataset.theme = theme;
document.documentElement.lang = locale;
window.__sidebarBrowserFixture = {synthetic: true, loadCalls: 0};
const load = async () => {
  window.__sidebarBrowserFixture.loadCalls++;
  return {cpuPercent: 25, memoryUsedBytes: 2147483648, memoryTotalBytes: 8589934592};
};
const status = <RecordingStatus locale={locale} paused={false} durationMs={65000}
  parts={[{source:"microphone", startSample:0, sampleCount:480000, status:"complete",
    text:"This is synthetic transcript text for the browser layout check."}]}
  warningCount={1} microphoneSilent={false} microphoneMuted={false} backlogMs={35000}
  getSystemLoad={load} onOpen={() => {}} />;
createRoot(document.getElementById("root")).render(<div className="app-shell">
  <Sidebar app={{version:"synthetic",recording:true}} page="meeting" locale={locale}
    theme={theme} t={locale === "de" ? de : en} navigate={async () => {}}
    setTheme={() => {}} setLocale={() => {}} recordingStatus={status} />
  <main><h1>Synthetic meeting</h1><p>No native IPC, audio or model service.</p></main>
</div>);
'''
    (fixture / "main.jsx").write_text(imports + "\n" + body, encoding="utf8")
    config = f'''import {{createRequire}} from "node:module";
import {{pathToFileURL}} from "node:url";
const source={json.dumps(source)};
const require=createRequire(source+"/package.json");
const react=(await import(pathToFileURL(require.resolve("@vitejs/plugin-react")).href)).default;
export default {{root:{json.dumps(fixture.as_posix())},plugins:[react()],
  publicDir:source+"/public",resolve:{{alias:{{react:source+"/node_modules/react",
  "react-dom":source+"/node_modules/react-dom","lucide-react":source+"/node_modules/lucide-react"}}}},
  build:{{outDir:{json.dumps((output / "dist").as_posix())},emptyOutDir:false,target:"es2022"}}}};
'''
    config_path = output / "vite.config.mjs"
    config_path.write_text(config, encoding="utf8")
    files = ["apps/desktop/src/views/Sidebar.tsx", "apps/desktop/src/views/RecordingStatus.tsx",
             "apps/desktop/src/styles.css", "apps/desktop/src/handy-theme.css", "apps/desktop/src/i18n.ts"]
    report = {"at": datetime.now(timezone.utc).isoformat(), "synthetic": True,
              "actualSidebarAndRecordingStatus": True, "nativeIPCOrDeviceTest": False,
              "playwright": installed, "appVersion": json.loads((desktop / "package.json").read_text())["version"],
              "sourceHashes": {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in files},
              "rows": [], "pageErrors": [], "consoleErrors": [], "foreignRequests": [], "success": False}
    server = worker = None
    try:
        with (output / "build.log").open("w", encoding="utf8") as log:
            subprocess.run(["node", str(vite), "build", "--config", str(config_path)], cwd=desktop,
                           stdout=log, stderr=subprocess.STDOUT, check=True, timeout=90,
                           creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))

        class Handler(SimpleHTTPRequestHandler):
            """Serve only the newly built fixture; keep the proof log noise-free."""
            def __init__(self, *values, **kwargs):
                super().__init__(*values, directory=str(output / "dist"), **kwargs)

            def log_message(self, *_):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        base = f"http://127.0.0.1:{server.server_port}/"
        with sync_playwright() as playwright:
            options = {} if args.browser == "chromium" else {"channel": args.browser}
            browser = playwright.chromium.launch(headless=True, **options)
            try:
                report["browser"] = {"channel": args.browser, "version": browser.version}
                for width in [390, 740, 1280]:
                    for locale in ["de", "en"]:
                        for theme in ["light", "dark"]:
                            context = browser.new_context(viewport={"width": width, "height": 900}, color_scheme=theme)
                            try:
                                page = context.new_page()
                                page.on("pageerror", lambda error: report["pageErrors"].append(str(error)))
                                page.on("console", lambda message: report["consoleErrors"].append(message.text)
                                        if message.type == "error" else None)

                                def route_request(route):
                                    if route.request.url.startswith(base):
                                        route.continue_()
                                    else:
                                        report["foreignRequests"].append(route.request.url)
                                        route.abort()

                                context.route("**/*", route_request)
                                page.goto(base + f"?locale={locale}&theme={theme}")
                                aside = page.locator(".sidebar")
                                expect(aside).to_be_visible()
                                for collapsed in [False, True]:
                                    current = "is-collapsed" in (aside.get_attribute("class") or "")
                                    if current != collapsed:
                                        page.locator(".sidebar-toggle").click()
                                    expect(aside).to_have_class(re.compile("is-collapsed" if collapsed else "is-expanded"))
                                    details = page.locator(".sidebar-resource-details")
                                    if details.get_attribute("open") is None:
                                        details.locator("summary").click()
                                    popup = page.locator(".sidebar-resource-content")
                                    expect(popup).to_be_visible()
                                    page.evaluate("new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))")
                                    box = popup.bounding_box()
                                    assert box and box["width"] > 100
                                    assert box["x"] >= -1 and box["y"] >= -1, box
                                    assert box["x"] + box["width"] <= width + 1, box
                                    assert box["y"] + box["height"] <= 901, box
                                    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth"), "Horizontal overflow"
                                    assert page.evaluate("window.__sidebarBrowserFixture.loadCalls > 0")
                                    report["rows"].append({"width": width, "height": 900, "locale": locale,
                                                           "theme": theme, "collapsed": collapsed, "flyout": box,
                                                           "noHorizontalOverflow": True})
                                    if locale == "de" and theme == "dark":
                                        page.screenshot(path=str(output / f"sidebar-{width}-{'collapsed' if collapsed else 'expanded'}.png"))
                            finally:
                                context.close()
                assert len(report["rows"]) == 24
                assert not report["pageErrors"] and not report["consoleErrors"] and not report["foreignRequests"], report
                report["success"] = True
            finally:
                browser.close()
    except Exception as error:
        report["failure"] = str(error)
        raise
    finally:
        if server:
            server.shutdown()
            server.server_close()
        if worker:
            worker.join(timeout=5)
        report["ownedServerStopped"] = worker is None or not worker.is_alive()
        with (output / "result.json").open("x", encoding="utf8") as proof:
            json.dump(report, proof, indent=2)
        print(json.dumps({"success": report["success"], "flows": len(report["rows"]),
                          "proof": str(output / "result.json"), "ownedServerStopped": report["ownedServerStopped"]}))


if __name__ == "__main__":
    main()
