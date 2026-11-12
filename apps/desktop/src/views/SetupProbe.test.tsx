import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import SetupProbe from "./SetupProbe";

it("reports the three synthetic setup stages only after explicit start", async () => {
  const run = vi.fn().mockResolvedValue({
    audioRoundtrip: true,
    transcriptionWorker: true,
    textAnswer: null,
  });
  render(<SetupProbe run={run} disabled={false} locale="de" />);
  expect(run).not.toHaveBeenCalled();
  expect(
    screen.getByText(/Die Erkennung von Wörtern.*separate Prüfungen/),
  ).toBeTruthy();
  await userEvent.click(
    screen.getByRole("button", { name: "Synthetische Funktionsprobe starten" }),
  );
  const status = screen.getByRole("status");
  expect(run).toHaveBeenCalledTimes(1);
  expect(status.textContent).toContain("Audiospeicherung · bestanden");
  expect(status.textContent).toContain(
    "Transkriptionssoftware (Testton) · bestanden",
  );
  expect(status.textContent).toContain("Lokale Textauswertung · übersprungen");
});

it("requests cancellation and waits for native cleanup before allowing a new check", async () => {
  let stopped!: (reason: Error) => void;
  const run = vi.fn(
    () =>
      new Promise<never>((_, reject) => {
        stopped = reject;
      }),
  );
  const cancel = vi.fn().mockResolvedValue(true);
  render(<SetupProbe run={run} cancel={cancel} disabled={false} locale="en" />);
  const start = screen.getByRole("button", {
    name: "Run synthetic setup check",
  });
  await userEvent.click(start);
  await userEvent.click(
    screen.getByRole("button", { name: "Cancel setup check" }),
  );
  expect(cancel).toHaveBeenCalledTimes(1);
  expect((start as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByRole("status").textContent).toContain(
    "Stopping setup check",
  );
  stopped(new Error("The setup check was cancelled."));
  await waitFor(() =>
    expect((start as HTMLButtonElement).disabled).toBe(false),
  );
  expect(screen.getByRole("alert").textContent).toContain(
    "The setup check was cancelled.",
  );
});

it("cancels a still-running check when its view closes", async () => {
  const run = vi.fn(() => new Promise<never>(() => {}));
  const cancel = vi.fn().mockResolvedValue(true);
  const { unmount } = render(
    <SetupProbe run={run} cancel={cancel} disabled={false} locale="en" />,
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Run synthetic setup check" }),
  );
  unmount();
  expect(cancel).toHaveBeenCalledTimes(1);
});
