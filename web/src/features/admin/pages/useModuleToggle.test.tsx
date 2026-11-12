// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import { ApiError, type ModuleEntry } from "../../../lib/api";
import { ModuleSaveError } from "../../../lib/queries";
import { useModuleToggle } from "./useModuleToggle";

const save = vi.hoisted(() => ({
  mutate: vi.fn(
    (_changes: unknown, callbacks: { onSuccess: () => void; onError: (error: Error) => void }) => {
      if (save.mutate.mock.calls.length === 1) callbacks.onSuccess();
      else callbacks.onError(new Error("stale"));
    },
  ),
  isPending: false,
}));
vi.mock("../../../lib/queries", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../lib/queries")>()),
  useSaveModules: () => save,
}));
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
const container = document.createElement("div");
const root = createRoot(container);
document.body.append(container);
let toggle: ReturnType<typeof useModuleToggle>;
function Page() {
  toggle = useModuleToggle("1");
  return null;
}
const module = { key: "pets", enabled: true } as ModuleEntry;
afterEach(async () => {
  await act(async () => root.render(null));
  vi.clearAllMocks();
});

it("undo guards the switch and explains a 409", async () => {
  save.mutate
    .mockImplementationOnce((_changes, callbacks) => callbacks.onSuccess())
    .mockImplementationOnce((_changes, callbacks) =>
      callbacks.onError(new ModuleSaveError("pets", new ApiError(409, "conflict", "stale"))),
    );
  await act(async () => root.render(<Page />));
  await act(async () => toggle.toggle(module));
  await act(async () => toggle.undo?.());
  expect(save.mutate).toHaveBeenNthCalledWith(
    2,
    [{ key: "pets", patch: { enabled: true, expect_enabled: false } }],
    expect.any(Object),
  );
  expect(toggle.notice).toBe("The module changed since this save and was left alone.");
});

it("asks before undo turns command permissions off", async () => {
  save.mutate.mockImplementation((_changes, callbacks) => callbacks.onSuccess());
  const commandControl = { key: "command_control", enabled: false } as ModuleEntry;
  await act(async () => root.render(<Page />));
  await act(async () => toggle.toggle(commandControl));
  expect(save.mutate).toHaveBeenCalledTimes(1);
  await act(async () => toggle.undo?.());
  expect(toggle.confirm?.key).toBe("command_control");
  expect(save.mutate).toHaveBeenCalledTimes(1);
  await act(async () => toggle.cancel());
  expect(save.mutate).toHaveBeenCalledTimes(1);
  await act(async () => toggle.undo?.());
  await act(async () => toggle.approve());
  expect(save.mutate).toHaveBeenNthCalledWith(
    2,
    [{ key: "command_control", patch: { enabled: false, expect_enabled: true } }],
    expect.any(Object),
  );
});
