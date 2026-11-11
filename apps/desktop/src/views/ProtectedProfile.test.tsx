import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { ProtectedProfileReceipt } from "../types";
import ProtectedProfile from "./ProtectedProfile";

it("copies and opens an encrypted profile only after explicit actions", async () => {
  const readStatus = vi
    .fn()
    .mockResolvedValue({ supported: true, active: false });
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
  const readStatus = vi
    .fn()
    .mockResolvedValue({ supported: true, active: false });
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
  const readStatus = vi
    .fn()
    .mockResolvedValue({ supported: true, active: false });
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
    const readStatus = vi.fn().mockResolvedValue({ supported, active });
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
