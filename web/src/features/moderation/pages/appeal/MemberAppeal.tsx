import { useState } from "react";
import { useParams } from "react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { translate as t } from "../../../../i18n";
import { api } from "../../../../lib/api";
import { Panel } from "../../../../components/Panel";
import { Field } from "../../../../components/Field";
import { PixelButton } from "../../../../components/PixelButton";
import { PageHead } from "../../../../components/PageHead";
import { Loading, Problem } from "../../../../components/States";

export function MemberAppeal() {
  const { guild, caseNumber } = useParams();
  const path = `/api/appeals/${guild}/${caseNumber}`;
  const [body, setBody] = useState("");
  const query = useQuery({
    queryKey: ["appeal", guild, caseNumber],
    queryFn: () =>
      api<{
        kind: string;
        reason: string | null;
        active: boolean;
        appeal: { status: string; decision: string | null } | null;
      }>(path),
    retry: false,
  });
  const submit = useMutation({ mutationFn: () => api(path, "POST", { body }) });
  return (
    <main className="settings-page mod-parity">
      <PageHead
        title={t("Appeal a moderation case")}
        lead={t("Explain what happened and why staff should reconsider this case.")}
      />
      {query.isPending && <Loading />}
      {query.isError && <Problem error={query.error} />}
      {query.data && (
        <Panel title={`#${caseNumber}`}>
          <p>
            {t(query.data.kind)} · {query.data.reason}
          </p>
          {query.data.appeal ? (
            <div role="status">
              <p>{t(query.data.appeal.status)}</p>
              <p>{query.data.appeal.decision}</p>
            </div>
          ) : submit.isSuccess ? (
            <p role="status">{t("Your appeal is waiting for staff review.")}</p>
          ) : (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                submit.mutate();
              }}
            >
              <Field label={t("Your appeal")} htmlFor="appeal-body">
                <textarea
                  id="appeal-body"
                  className="pixel-input"
                  minLength={10}
                  maxLength={2000}
                  required
                  value={body}
                  onChange={(e) => setBody(e.target.value)}
                />
              </Field>
              <p>{body.length}/2000</p>
              {submit.isError && <Problem error={submit.error} />}
              <PixelButton
                type="submit"
                disabled={!query.data.active || body.trim().length < 10 || submit.isPending}
              >
                {t("Submit appeal")}
              </PixelButton>
            </form>
          )}
        </Panel>
      )}
    </main>
  );
}
