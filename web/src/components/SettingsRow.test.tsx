// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { SettingsRow } from "./SettingsRow";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

const host = document.createElement("div");
document.body.append(host);
const root = createRoot(host);

afterEach(async () => {
  await act(async () => root.render(null));
  vi.restoreAllMocks();
  sessionStorage.clear();
});

it("keeps an open row across a remount when session storage throws", async () => {
  const title = "Blocked storage";
  const details = "Details";
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("Storage blocked");
  });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("Storage blocked");
  });
  const renderRow = async () =>
    act(async () =>
      root.render(
        <SettingsRow rowId="blocked-storage-row" title={title} description="Description" summary="Summary">
          <p>{details}</p>
        </SettingsRow>,
      ),
    );

  await renderRow();
  const button = () => host.querySelector(".settings-detail-expand") as HTMLButtonElement;
  expect(button().getAttribute("aria-expanded")).toBe("false");
  await act(async () => button().click());
  expect(button().getAttribute("aria-expanded")).toBe("true");
  await act(async () => root.render(null));
  await renderRow();
  expect(button().getAttribute("aria-expanded")).toBe("true");
  expect(host.textContent).toContain(details);
});

it("keeps an open row through a remount and language switch on the same page", async () => {
  const renderRow = async (title: string) =>
    act(async () =>
      root.render(
        <SettingsRow rowId="branding-bio" title={title} description="Description" summary="Summary">
          <p>{title}</p>
        </SettingsRow>,
      ),
    );

  await renderRow("Bio");
  const button = () => host.querySelector(".settings-detail-expand") as HTMLButtonElement;
  expect(button().getAttribute("aria-expanded")).toBe("false");
  await act(async () => button().click());
  expect(button().getAttribute("aria-expanded")).toBe("true");
  await act(async () => root.render(null));
  await renderRow("Biyografi");
  expect(button().getAttribute("aria-expanded")).toBe("true");
  expect(host.textContent).toContain("Biyografi");
});
