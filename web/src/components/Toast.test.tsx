// @vitest-environment jsdom

import { act, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";
import { Toast, ToastProvider } from "./Toast";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

const container = document.createElement("div");
const root = createRoot(container);
document.body.append(container);
const labels = { draft: "Draft", edit: "Edit", show: "Show toast", hide: "Hide toast" };

afterEach(async () => {
  await act(async () => root.render(null));
});

it("keeps page content mounted and its draft when a toast appears and closes", async () => {
  let mounts = 0;
  let unmounts = 0;

  function Page() {
    const [draft, setDraft] = useState("original");
    const [notice, setNotice] = useState<string | null>(null);
    useEffect(() => {
      mounts++;
      return () => {
        unmounts++;
      };
    }, []);
    return (
      <>
        <input aria-label={labels.draft} value={draft} onChange={(event) => setDraft(event.target.value)} />
        <button type="button" onClick={() => setDraft("edited")}>
          {labels.edit}
        </button>
        <button type="button" onClick={() => setNotice("Saved")}>
          {labels.show}
        </button>
        <button type="button" onClick={() => setNotice(null)}>
          {labels.hide}
        </button>
        <Toast message={notice} onDone={() => setNotice(null)} />
      </>
    );
  }

  await act(async () =>
    root.render(
      <ToastProvider className="page shell-page">
        <Page />
      </ToastProvider>,
    ),
  );
  await act(async () => (container.querySelectorAll("button")[0] as HTMLButtonElement).click());
  await act(async () => (container.querySelectorAll("button")[1] as HTMLButtonElement).click());
  expect(container.querySelector(".toast")?.textContent).toBe("Saved");
  expect((container.querySelector("input") as HTMLInputElement).value).toBe("edited");
  expect(mounts).toBe(1);
  expect(unmounts).toBe(0);
  await act(async () => (container.querySelectorAll("button")[2] as HTMLButtonElement).click());
  expect(container.querySelector(".toast")).toBeNull();
  expect((container.querySelector("input") as HTMLInputElement).value).toBe("edited");
  expect(mounts).toBe(1);
  expect(unmounts).toBe(0);
});
