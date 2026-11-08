import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { t } from "../../i18n/workflows";
import { PixelButton } from "../../components/PixelButton";
import { api } from "../../lib/api";
export function PostButton({ path }: { path: string }) {
  const [channel, setChannel] = useState("");
  const [text, setText] = useState("");
  const [label, setLabel] = useState("");
  const post = useMutation({ mutationFn: () => api(path, "POST", { channel_id: channel, text, label }) });
  return (
    <fieldset className="wf-fields wf-edit" disabled={post.isPending}>
      <legend>{t("Post workflow button")}</legend>
      <label>
        {t("Channel")}
        <input className="pixel-input" value={channel} onChange={(e) => setChannel(e.target.value)} />
      </label>
      <label>
        {t("Button message")}
        <textarea
          className="pixel-input"
          maxLength={2000}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <label>
        {t("Button label")}
        <input
          className="pixel-input"
          maxLength={80}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
      </label>
      <PixelButton disabled={!channel || !text || !label} onClick={() => post.mutate()}>
        {t("Post workflow button")}
      </PixelButton>
      {post.error && <p role="alert">{post.error.message}</p>}
      {post.isSuccess && <p role="status">{t("Workflow button posted.")}</p>}
    </fieldset>
  );
}
