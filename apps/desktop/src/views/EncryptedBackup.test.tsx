import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import EncryptedBackup from "./EncryptedBackup";

it("requires matching passphrases, clears both fields and shows the verified file", async () => {
  const recoveryKey = "ab".repeat(32);
  const run = vi
    .fn()
    .mockResolvedValue({ file: "example.hmvault", recoveryKey });
  render(<EncryptedBackup locale="de" run={run} />);
  const user = userEvent.setup();
  await user.click(
    screen.getByText("Verschlüsselte Kopie erstellen", { selector: "summary" }),
  );
  const first = screen.getByLabelText("Passphrase") as HTMLInputElement;
  const repeat = screen.getByLabelText(
    "Passphrase wiederholen",
  ) as HTMLInputElement;
  await user.type(first, "synthetic-test-passphrase");
  await user.type(repeat, "synthetic-wrong-passphrase");
  const submit = screen.getByRole("button", {
    name: "Verschlüsselte Kopie erstellen",
  });
  await user.click(submit);
  expect(screen.getByRole("alert").textContent).toContain(
    "wiederhole dieselbe Passphrase",
  );
  expect(run).not.toHaveBeenCalled();
  await user.clear(repeat);
  await user.type(repeat, "synthetic-test-passphrase");
  await user.click(submit);
  expect(run).toHaveBeenCalledExactlyOnceWith(
    "synthetic-test-passphrase",
    "passphrase",
  );
  expect(first.value).toBe("");
  expect(repeat.value).toBe("");
  expect(await screen.findByText("example.hmvault")).toBeTruthy();
  expect(screen.queryByText("HANDYMEETING_DATA_DIR")).toBeNull();
  expect(screen.getByDisplayValue(recoveryKey)).toBeTruthy();
  await user.click(
    screen.getByRole("button", { name: "Schlüssel ausblenden" }),
  );
  expect(screen.queryByDisplayValue(recoveryKey)).toBeNull();
  expect(screen.getByText("example.hmvault")).toBeTruthy();
});

it("requests cancellation of the active recovery without displaying a success receipt", async () => {
  let complete!: (value: null) => void;
  const run = vi.fn(
    () =>
      new Promise<null>((resolve) => {
        complete = resolve;
      }),
  );
  const cancel = vi.fn().mockResolvedValue(true);
  render(<EncryptedBackup locale="en" restore run={run} cancel={cancel} />);
  const user = userEvent.setup();
  await user.click(
    screen.getByText("Recover an encrypted meeting", { selector: "summary" }),
  );
  await user.type(
    screen.getByLabelText("Passphrase"),
    "synthetic-test-passphrase",
  );
  await user.click(
    screen.getByRole("button", { name: "Recover an encrypted meeting" }),
  );
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(await screen.findByText("Cancellation requested …")).toBeTruthy();
  expect(cancel).toHaveBeenCalledOnce();
  await act(async () => complete(null));
  await screen.findByRole("button", { name: "Recover an encrypted meeting" });
  expect(screen.queryByText("Recovered profile")).toBeNull();
  expect(screen.queryByText("HANDYMEETING_DATA_DIR")).toBeNull();
});

it("clears a passphrase when switching to key recovery and submits only the chosen credential", async () => {
  const run = vi.fn().mockResolvedValue({ file: "isolated/profile" });
  const user = userEvent.setup();
  render(<EncryptedBackup locale="en" restore run={run} />);
  await user.click(
    screen.getByText("Recover an encrypted meeting", { selector: "summary" }),
  );
  await user.type(screen.getByLabelText("Passphrase"), "synthetic-passphrase");
  await user.selectOptions(screen.getByLabelText("Unlock with"), "recoveryKey");
  const field = screen.getByLabelText("Recovery key") as HTMLInputElement;
  expect(field.value).toBe("");
  const key = "cd".repeat(32);
  await user.type(field, key);
  await user.click(
    screen.getByRole("button", { name: "Recover an encrypted meeting" }),
  );
  expect(run).toHaveBeenCalledExactlyOnceWith(key, "recoveryKey");
  expect(field.value).toBe("");
  expect(await screen.findByText("isolated/profile")).toBeTruthy();
  expect(
    screen.getByText("HANDYMEETING_DATA_DIR").parentElement?.textContent,
  ).toBe(
    "This profile contains readable session files. To open it, set HANDYMEETING_DATA_DIR to the displayed profile folder in your terminal, then launch Meeting for Handy from that terminal.",
  );
});
