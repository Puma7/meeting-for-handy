import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import SetupProbe from "./SetupProbe";

it("reports the three synthetic setup stages only after explicit start", async () => {
  const run = vi
    .fn()
    .mockResolvedValue({
      audioRoundtrip: true,
      recognition: false,
      textAnswer: null,
    });
  render(<SetupProbe run={run} disabled={false} locale="de" />);
  expect(run).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole("button", { name: "Synthetische Funktionsprobe starten" }),
  );
  const status = screen.getByRole("status");
  expect(run).toHaveBeenCalledTimes(1);
  expect(status.textContent).toContain("Audiospeicherung · bestanden");
  expect(status.textContent).toContain(
    "Transkriptionssoftware · bitte Einrichtung prüfen",
  );
  expect(status.textContent).toContain("Lokale Textauswertung · übersprungen");
});
