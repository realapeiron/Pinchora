import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import { en } from "./en";
import { tr } from "./tr";

const ready = vi.hoisted(() => vi.fn());
vi.mock("./index", () => ({ ready, locale: () => "tr" }));
import { start } from "./start";

function element() {
  return { textContent: "", type: "", onclick: undefined as (() => void) | undefined, setAttribute: vi.fn() };
}
function dom() {
  const root = { replaceChildren: vi.fn(), append: vi.fn() };
  vi.stubGlobal("document", { documentElement: {}, createElement: element, getElementById: () => root });
  vi.stubGlobal("window", { location: { reload: vi.fn() } });
  return root;
}
afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

it("renders only after catalogues resolve, with localized loading text", async () => {
  const root = dom();
  let finish!: () => void;
  ready.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  const render = vi.fn();
  const pending = start(render);
  expect(render).not.toHaveBeenCalled();
  expect(root.replaceChildren.mock.calls[0]![0].textContent).toBe(`${tr.Loading}…`);
  finish();
  await pending;
  expect(render).toHaveBeenCalledOnce();
});

it("shows an accessible localized failure and reload action for a missing hashed chunk", async () => {
  const root = dom();
  ready.mockRejectedValue(new Error("Failed to fetch dynamically imported module"));
  const render = vi.fn();
  await start(render);
  expect(render).not.toHaveBeenCalled();
  const status = root.replaceChildren.mock.calls[0]![0];
  expect(status.textContent).toBe(tr["Something went wrong on our side. Please try again."]);
  expect(status.setAttribute).toHaveBeenLastCalledWith("role", "alert");
  const reload = root.append.mock.calls[0]![0];
  expect(reload.textContent).toBe(tr.Refresh);
  reload.onclick();
  expect(window.location.reload).toHaveBeenCalledOnce();
  expect(en.Refresh).toBe("Refresh");
});

it.each(["main.tsx", "temp-voice-preview.tsx"])("gates the %s entry point on catalogue readiness", (file) => {
  const source = readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
  expect(source).toMatch(/await start\(\(\) => \{[\s\S]*createRoot/);
});
