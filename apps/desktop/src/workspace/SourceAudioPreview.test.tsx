import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import SourceAudioPreview from "./SourceAudioPreview";

it("never attaches a revoked preview URL when retrying a media failure", async () => {
  const revoked = new Set<string>();
  const committed: string[] = [];
  let index = 0;
  const originalCreate = Object.getOwnPropertyDescriptor(
    URL,
    "createObjectURL",
  );
  const originalRevoke = Object.getOwnPropertyDescriptor(
    URL,
    "revokeObjectURL",
  );
  Object.defineProperty(URL, "createObjectURL", {
    configurable: true,
    value: vi.fn(() => `blob:preview-${++index}`),
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    configurable: true,
    value: vi.fn((url: string) => revoked.add(url)),
  });
  const originalSet = Element.prototype.setAttribute;
  const setter = vi
    .spyOn(Element.prototype, "setAttribute")
    .mockImplementation(function (this: Element, name: string, value: string) {
      if (this.tagName === "AUDIO" && name === "src") committed.push(value);
      return originalSet.call(this, name, value);
    });
  const load = vi.fn(async () => new ArrayBuffer(44));
  const view = render(
    <SourceAudioPreview file="synthetic.wav" load={load} offsetSeconds={2} />,
  );
  try {
    fireEvent.click(
      screen.getByRole("button", { name: "Audiovorschau laden" }),
    );
    const audio = await screen.findByLabelText(
      "Audiovorschau des Originalabschnitts",
    );
    expect(audio.getAttribute("src")).toBe("blob:preview-1");
    fireEvent.error(audio);
    expect(revoked.has("blob:preview-1")).toBe(true);
    fireEvent.click(
      screen.getByRole("button", { name: "Audiovorschau laden" }),
    );
    await waitFor(() =>
      expect(
        screen
          .getByLabelText("Audiovorschau des Originalabschnitts")
          .getAttribute("src"),
      ).toBe("blob:preview-2"),
    );
    expect(committed).toEqual(["blob:preview-1", "blob:preview-2"]);
    expect(load).toHaveBeenCalledTimes(2);
    view.unmount();
    expect([...revoked]).toEqual(["blob:preview-1", "blob:preview-2"]);
  } finally {
    view.unmount();
    setter.mockRestore();
    if (originalCreate)
      Object.defineProperty(URL, "createObjectURL", originalCreate);
    else Reflect.deleteProperty(URL, "createObjectURL");
    if (originalRevoke)
      Object.defineProperty(URL, "revokeObjectURL", originalRevoke);
    else Reflect.deleteProperty(URL, "revokeObjectURL");
  }
});
