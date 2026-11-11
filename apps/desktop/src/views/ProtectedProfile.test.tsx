import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type {
  ProtectedProfileProvider,
  ProtectedProfileReceipt,
} from "../types";
import ProtectedProfile from "./ProtectedProfile";

it("copies and opens an encrypted profile only after explicit actions", async () => {
  const readStatus = vi.fn().mockResolvedValue({
    supported: true,
    active: false,
    provider: "windowsEfs",
  });
  let finish!: (receipt: ProtectedProfileReceipt) => void;
  const create = vi.fn(
    () =>
      new Promise<ProtectedProfileReceipt>((resolve) => {
        finish = resolve;
      }),
  );
  const open = vi.fn().mockResolvedValue(true);
  render(
    <ProtectedProfile
      locale="en"
      readStatus={readStatus}
      create={create}
      open={open}
    />,
  );
  const user = userEvent.setup();
  expect(readStatus).not.toHaveBeenCalled();
  await user.click(screen.getByText("Encrypt session storage"));
  const copy = await screen.findByRole("button", {
    name: "Create encrypted profile",
  });
  expect(create).not.toHaveBeenCalled();
  const openProfile = screen.getByRole("button", {
    name: "Open encrypted profile",
  });
  expect(openProfile).toHaveProperty("disabled", false);
  await user.click(copy);
  expect(create).toHaveBeenCalledExactlyOnceWith();
  expect(copy).toHaveProperty("disabled", true);
  expect(openProfile).toHaveProperty("disabled", true);
  await act(async () =>
    finish({ path: "synthetic/protected", files: 4, bytes: 2048 }),
  );
  expect(screen.getByText("synthetic/protected")).toBeTruthy();
  expect(open).not.toHaveBeenCalled();
  await user.click(openProfile);
  expect(open).toHaveBeenCalledExactlyOnceWith();
  expect(await screen.findByText(/Close this window/)).toBeTruthy();
});

it("keeps the previous profile when the folder picker is canceled or copying fails", async () => {
  const readStatus = vi.fn().mockResolvedValue({
    supported: true,
    active: false,
    provider: "windowsEfs",
  });
  const create = vi
    .fn()
    .mockResolvedValueOnce(null)
    .mockRejectedValueOnce("An encrypted target could not be verified.");
  const open = vi.fn();
  render(
    <ProtectedProfile
      locale="en"
      readStatus={readStatus}
      create={create}
      open={open}
    />,
  );
  const user = userEvent.setup();
  await user.click(screen.getByText("Encrypt session storage"));
  const copy = await screen.findByRole("button", {
    name: "Create encrypted profile",
  });
  await user.click(copy);
  expect(screen.queryByText(/Encrypted profile verified/)).toBeNull();
  await user.click(copy);
  expect((await screen.findByRole("alert")).textContent).toBe(
    "An encrypted target could not be verified.",
  );
  expect(screen.queryByText(/Encrypted profile verified/)).toBeNull();
  expect(open).not.toHaveBeenCalled();
});

it("opens an existing profile without first creating a copy", async () => {
  const readStatus = vi.fn().mockResolvedValue({
    supported: true,
    active: false,
    provider: "windowsEfs",
  });
  const create = vi.fn();
  const open = vi.fn().mockResolvedValue(null);
  render(
    <ProtectedProfile
      locale="en"
      readStatus={readStatus}
      create={create}
      open={open}
    />,
  );
  const user = userEvent.setup();
  await user.click(screen.getByText("Encrypt session storage"));
  await user.click(
    await screen.findByRole("button", { name: "Open encrypted profile" }),
  );
  expect(open).toHaveBeenCalledExactlyOnceWith();
  expect(create).not.toHaveBeenCalled();
  expect(screen.queryByText(/Close this window/)).toBeNull();
});

it.each([
  [
    true,
    true,
    "Windows file encryption (EFS) is active for this meeting profile.",
  ],
  [false, false, /This requires Windows with EFS/],
] as const)(
  "reports protection status without offering a new copy",
  async (supported, active, text) => {
    const readStatus = vi
      .fn()
      .mockResolvedValue({ supported, active, provider: "windowsEfs" });
    const create = vi.fn();
    render(
      <ProtectedProfile locale="en" readStatus={readStatus} create={create} />,
    );
    await userEvent.setup().click(screen.getByText("Encrypt session storage"));
    expect(await screen.findByText(text)).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: "Create encrypted profile" }),
    ).toBeNull();
    expect(create).not.toHaveBeenCalled();
  },
);

it.each(["macosImage", "linuxGocryptfs"] as const)(
  "creates a %s profile only with a matching passphrase and clears it before the native call",
  async (provider) => {
    const readStatus = vi
      .fn()
      .mockResolvedValue({ supported: true, active: false, provider });
    const secret = "synthetic create passphrase";
    let input: HTMLInputElement;
    let repeated: HTMLInputElement;
    const create = vi.fn(async (_passphrase?: string) => {
      expect(input.value).toBe("");
      expect(repeated.value).toBe("");
      return { path: "synthetic/protected", files: 4, bytes: 2048 };
    });
    const open = vi.fn();
    render(
      <ProtectedProfile
        locale="en"
        readStatus={readStatus}
        create={create}
        open={open}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByText("Encrypt session storage"));
    await user.click(
      await screen.findByRole("button", { name: "Create encrypted profile" }),
    );
    input = screen.getByLabelText<HTMLInputElement>("Passphrase");
    repeated = screen.getByLabelText<HTMLInputElement>("Repeat passphrase");
    expect(input.type).toBe("password");
    expect(input.autocomplete).toBe("off");
    expect(create).not.toHaveBeenCalled();
    await user.type(input, secret);
    await user.type(repeated, "different passphrase");
    const submit = screen.getByRole("button", { name: "Confirm and create" });
    await user.click(submit);
    expect((await screen.findByRole("alert")).textContent).toMatch(
      /same passphrase twice/,
    );
    expect(create).not.toHaveBeenCalled();
    await user.clear(repeated);
    await user.type(repeated, secret);
    await user.click(submit);
    expect(create).toHaveBeenCalledExactlyOnceWith(secret);
    expect(screen.queryByLabelText("Passphrase")).toBeNull();
    expect(await screen.findByText("synthetic/protected")).toBeTruthy();
    expect(Object.values(localStorage)).not.toContain(secret);
    expect(open).not.toHaveBeenCalled();
  },
);

it("cancels a Linux passphrase and opens an existing profile without a repeat field", async () => {
  const readStatus = vi.fn().mockResolvedValue({
    supported: true,
    active: false,
    provider: "linuxGocryptfs",
  });
  const create = vi.fn();
  const open = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  render(
    <ProtectedProfile
      locale="en"
      readStatus={readStatus}
      create={create}
      open={open}
    />,
  );
  const user = userEvent.setup();
  await user.click(screen.getByText("Encrypt session storage"));
  const openProfile = await screen.findByRole("button", {
    name: "Open encrypted profile",
  });
  await user.click(openProfile);
  expect(screen.queryByLabelText("Repeat passphrase")).toBeNull();
  await user.type(screen.getByLabelText("Passphrase"), "discarded");
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(open).not.toHaveBeenCalled();
  await user.click(openProfile);
  expect(screen.getByLabelText<HTMLInputElement>("Passphrase").value).toBe("");
  await user.type(screen.getByLabelText("Passphrase"), "existing");
  await user.click(screen.getByRole("button", { name: "Unlock and open" }));
  expect(open).toHaveBeenCalledExactlyOnceWith("existing");
  expect(screen.queryByText(/Close this window/)).toBeNull();
  await user.click(openProfile);
  await user.type(screen.getByLabelText("Passphrase"), "existing");
  await user.click(screen.getByRole("button", { name: "Unlock and open" }));
  expect(await screen.findByText(/Close this window/)).toBeTruthy();
  expect(create).not.toHaveBeenCalled();
});

it.each([
  ["macosImage", true, true, /private workspace drafts.+temporary/i],
  ["linuxGocryptfs", false, false, /install gocryptfs.+FUSE/i],
] satisfies [ProtectedProfileProvider, boolean, boolean, RegExp][])(
  "reports storage availability and privacy for %s",
  async (provider, supported, active, text) => {
    const readStatus = vi
      .fn()
      .mockResolvedValue({ supported, active, provider });
    const create = vi.fn();
    const open = vi.fn();
    render(
      <ProtectedProfile
        locale="en"
        readStatus={readStatus}
        create={create}
        open={open}
      />,
    );
    await userEvent.setup().click(screen.getByText("Encrypt session storage"));
    expect(await screen.findByText(text)).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: "Create encrypted profile" }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Open encrypted profile" }),
    ).toBeNull();
  },
);
