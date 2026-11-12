// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import type { Settings } from "./api";
import { useSettingsForm, type SettingsForm } from "./useSettingsForm";

const save = vi.hoisted(() => ({
  mutate: vi.fn((_patch: unknown, callbacks: { onSuccess: (updated: { points_name: string }) => void }) =>
    callbacks.onSuccess({ points_name: "coins" }),
  ),
  mutateAsync: vi.fn(async () => ({ skipped: ["points_name"] })),
  reset: vi.fn(),
  isPending: false,
  isError: false,
}));
vi.mock("./queries", () => ({ useSaveSettings: () => save }));
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
const container = document.createElement("div");
const root = createRoot(container);
document.body.append(container);
let form: SettingsForm;
function Page() {
  form = useSettingsForm("1", { points_name: "points" } as Settings, () => []);
  return null;
}
afterEach(async () => {
  await act(async () => root.render(null));
  vi.clearAllMocks();
});

it("undo sends the saved values as expect and shows the server's skipped-field notice", async () => {
  await act(async () => root.render(<Page />));
  await act(async () => form.set("points_name", "coins"));
  await act(async () => form.submit());
  expect(save.mutate).toHaveBeenCalledWith({ points_name: "coins" }, expect.any(Object));
  await act(async () => form.undo?.());
  expect(save.mutateAsync).toHaveBeenCalledWith({ points_name: "points", expect: { points_name: "coins" } });
  expect(form.toast).toBe("Some values changed since this save and were left alone.");
});
