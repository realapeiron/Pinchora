import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { t } from "../../../../i18n/workflows";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { Problem } from "../../../../components/States";
import { api } from "../../../../lib/api";
export function Variables({ path }: { path: string }) {
  const client = useQueryClient();
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const [scope, setScope] = useState("server");
  const [subject, setSubject] = useState("");
  const effectiveSubject = scope === "member" ? subject : "";
  const query = useQuery({
    queryKey: [path, "variables", effectiveSubject],
    queryFn: () =>
      api<{ items: [string, string, string][] }>(
        `${path}/variables?subject=${encodeURIComponent(effectiveSubject)}`,
      ),
  });
  const save = useMutation({
    mutationFn: () =>
      api(`${path}/variables`, "PUT", { scope, key, value, subject_id: effectiveSubject || null }),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: [path, "variables"] });
      setKey("");
      setValue("");
    },
  });
  return (
    <Panel title={t("Variables")}>
      <fieldset className="wf-fake wf-edit" disabled={save.isPending}>
        <label>
          {t("Scope")}
          <select
            className="pixel-input pixel-select"
            value={scope}
            onChange={(e) => setScope(e.target.value)}
          >
            <option value="server">{t("Server")}</option>
            <option value="member">{t("Member")}</option>
          </select>
        </label>
        {scope === "member" && (
          <label>
            {t("Subject ID (blank: you)")}
            <input className="pixel-input" value={subject} onChange={(e) => setSubject(e.target.value)} />
          </label>
        )}
        <label>
          {t("Variable key")}
          <input
            className="pixel-input"
            maxLength={32}
            value={key}
            onChange={(e) => setKey(e.target.value)}
          />
        </label>
        <label>
          {t("Variable value")}
          <input
            className="pixel-input"
            maxLength={200}
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
        </label>
        <PixelButton
          disabled={
            !key ||
            query.isPending ||
            query.isError ||
            save.isPending ||
            query.data?.items.some(([s, k, v]) => s === scope && k === key && v === value)
          }
          onClick={() => save.mutate()}
        >
          {t("Save variable")}
        </PixelButton>
      </fieldset>
      {save.error && <p role="alert">{save.error.message}</p>}
      {save.isSuccess && <p role="status">{t("Variable saved.")}</p>}
      {query.isError && <Problem error={query.error} />}
      <dl>
        {query.data?.items.map(([s, k, v]) => (
          <div key={`${s}:${k}`}>
            <dt>
              <code>{`{var.${s}.${k}}`}</code>
            </dt>
            <dd>{v}</dd>
          </div>
        ))}
      </dl>
    </Panel>
  );
}
