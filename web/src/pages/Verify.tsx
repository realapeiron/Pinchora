import { useEffect, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useParams } from "react-router";
import { api } from "../lib/api";
import { configureGuild } from "../i18n";
import { words } from "../lib/verification";
import { RequireLogin } from "./RequireLogin";
import { Loading, Problem } from "./States";
import { Panel } from "../components/Panel";
import { Field } from "../components/Field";
import { PixelButton } from "../components/PixelButton";
import "../styles/verification.css";
export function Verify() {
  return <RequireLogin>{() => <MemberGate />}</RequireLogin>;
}
function MemberGate() {
  const { token } = useParams();
  const path = `/api/verify/${token}`;
  const [answer, setAnswer] = useState("");
  const data = useQuery({
    queryKey: ["verification", token],
    queryFn: () => api<{ language: string; strings: Record<string, string> }>(path),
    retry: false,
  });
  useEffect(() => {
    const language = data.data?.language;
    configureGuild(language === "en" || language === "tr" ? language : null);
  }, [data.data?.language]);
  const card = useMutation({
    mutationFn: () => api<{ nonce: string; image: string }>(`${path}/captcha`, "POST"),
  });
  const check = useMutation({
    mutationFn: () =>
      api<{ message: string; done: boolean }>(`${path}/answer`, "POST", { nonce: card.data?.nonce, answer }),
    onSuccess: (result) => {
      setAnswer("");
      card.reset();
      if (!result.done) card.mutate();
    },
  });
  const help = useMutation({ mutationFn: () => api<{ message: string }>(`${path}/help`, "POST") });
  if (data.isPending) return <Loading full />;
  if (data.isError) return <Problem full error={data.error} />;
  const t = words(data.data.strings, data.data.language);
  const error = card.error ?? check.error ?? help.error;
  return (
    <main className="safety-member">
      <Panel title={t("web_title")}>
        <p>{t("member_hint")}</p>
        {check.data && <p role="status">{check.data.message}</p>}
        {error && (
          <p className="field-error" role="alert">
            {error.message}
          </p>
        )}
        {help.data && <p role="status">{help.data.message}</p>}
        <PixelButton
          variant="ghost"
          disabled={help.isPending || check.data?.done}
          onClick={() => help.mutate()}
        >
          {t("ask")}
        </PixelButton>
        {!check.data?.done && (
          <>
            {card.data && (
              <>
                <img
                  className="safety-captcha"
                  src={card.data.image}
                  alt={t("preview_alt")}
                  width={640}
                  height={280}
                />
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    check.mutate();
                  }}
                >
                  <Field label={t("captcha")} htmlFor="verify-answer">
                    <input
                      id="verify-answer"
                      className="pixel-input"
                      value={answer}
                      minLength={5}
                      maxLength={5}
                      required
                      autoComplete="off"
                      onChange={(e) => setAnswer(e.target.value)}
                    />
                  </Field>
                  <PixelButton disabled={check.isPending || answer.length !== 5} type="submit">
                    {t("submit")}
                  </PixelButton>
                </form>
              </>
            )}
            <PixelButton
              variant="secondary"
              disabled={card.isPending || check.isPending}
              onClick={() => {
                setAnswer("");
                check.reset();
                card.mutate();
              }}
            >
              {t(card.data ? "new_card" : "get_card")}
            </PixelButton>
          </>
        )}
      </Panel>
    </main>
  );
}
