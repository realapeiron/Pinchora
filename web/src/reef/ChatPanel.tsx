import { useEffect, useState, useSyncExternalStore } from "react";
import { Link } from "react-router";
import { api, ApiError } from "../lib/api";
import type { ReefNet } from "./net";
import { playerStatus, t } from "./i18n";
import type { ChatLine } from "./chat";

export const safetyError = (error: unknown) =>
  playerStatus(
    error instanceof ApiError &&
      ["access_denied", "invalid_safety_request", "rate_limited", "session_expired"].includes(error.code)
      ? error.code
      : "safety_unavailable",
  );
export function ChatPanel({
  net,
  roomKey,
  guildManager = false,
}: {
  net: ReefNet;
  roomKey: string;
  guildManager?: boolean;
}) {
  const lines = useSyncExternalStore(net.chat.subscribe, net.chat.snapshot);
  const [eligible, setEligible] = useState(false);
  const [version, setVersion] = useState(1);
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState("");
  const [notice, setNotice] = useState("");
  const [report, setReport] = useState<ChatLine | null>(null);
  const [reason, setReason] = useState("harassment");
  const [blocked, setBlocked] = useState<string[]>([]);
  const [operator, setOperator] = useState(false);
  const own = roomKey.split(":")[1] === net.userId;
  useEffect(() => {
    let cancelled = false;
    void api<{ chat_enabled: boolean; consent_version: number; operator: boolean }>("/api/reef/bootstrap")
      .then((data) => {
        if (!cancelled) {
          setEligible(data.chat_enabled);
          setVersion(data.consent_version);
          setOperator(data.operator);
        }
      })
      .catch((error) => {
        if (!cancelled) setNotice(safetyError(error));
      });
    void api<{ users: string[] }>("/api/reef/blocks")
      .then((data) => {
        if (!cancelled) setBlocked(data.users);
      })
      .catch(() => {});
    const select = (event: Event) => {
      const id = (event as CustomEvent<string>).detail;
      setReport(net.chat.snapshot().find((line) => line.message_id === id) ?? null);
    };
    window.addEventListener("reef-report", select);
    return () => {
      cancelled = true;
      window.removeEventListener("reef-report", select);
    };
  }, [net]);
  async function run(action: () => Promise<void>) {
    setBusy(true);
    setNotice("");
    try {
      await action();
    } catch (error) {
      setNotice(safetyError(error));
    } finally {
      setBusy(false);
    }
  }
  const moderate = (line: ChatLine, kind: string) =>
    run(async () => {
      await api("/api/reef/moderation", "POST", {
        target: line.sender_id,
        owner: roomKey.split(":")[1],
        room_idx: null,
        kind,
        seconds: 3600,
        report: null,
      });
      setNotice(t("Moderation action recorded."));
    });
  return (
    <section className="reef-chat" aria-label={t("Room chat")}>
      <header>
        <strong>{t("Room chat")}</strong>{" "}
        <Link to={guildManager ? `/reef/moderation?guild=${roomKey.split(":")[1]}` : "/reef/moderation"}>
          {t("Reports and moderation")}
        </Link>
      </header>
      {!eligible ? (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void run(async () => {
              const result = await api<{ eligible: boolean }>("/api/reef/consent", "PUT", {
                version,
                age_band: "13_plus",
                accepted: ack,
              });
              setEligible(result.eligible);
            });
          }}
        >
          <p>{t("House hosts moderate chat. Messages are kept for 7 days, or 90 days when reported.")}</p>
          <label>
            <input type="checkbox" checked={ack} onChange={(event) => setAck(event.target.checked)} />
            {t(
              "I am 13 or older and agree to the chat rules: be respectful and do not share personal information.",
            )}
          </label>
          <p>{t("Discord login does not verify your age.")}</p>
          <button disabled={!ack || busy}>{t("Enable chat")}</button>
        </form>
      ) : (
        <>
          <div
            role="log"
            aria-live="polite"
            aria-relevant="additions text"
            aria-label={t("Chat transcript")}
            className="reef-transcript"
          >
            {lines.map((line) => (
              <article
                key={line.message_id}
                id={`chat-${line.message_id}`}
                tabIndex={-1}
                data-delivery={line.delivery}
              >
                <strong>{line.name || t("You")}</strong>: <span>{line.text}</span>{" "}
                <small>{t(line.delivery)}</small>
                {line.reason && <span>{playerStatus(line.reason)}</span>}
                {line.delivery === "delivered" && (
                  <div className="reef-chat-actions">
                    <button onClick={() => setReport(line)}>{t("Report")}</button>
                    {line.sender_id !== net.userId && (
                      <>
                        <button
                          disabled={busy}
                          onClick={() =>
                            void run(async () => {
                              await api(`/api/reef/blocks/${line.sender_id}`, "PUT");
                              setBlocked((old) => [...old, line.sender_id]);
                              net.chat.clear();
                              setNotice(t("Block saved."));
                            })
                          }
                        >
                          {t("Block")}
                        </button>
                        {(own || operator || guildManager) &&
                          ["mute", "kick", "ban"].map((kind) => (
                            <button key={kind} disabled={busy} onClick={() => void moderate(line, kind)}>
                              {t(kind)}
                            </button>
                          ))}
                      </>
                    )}
                  </div>
                )}
              </article>
            ))}
          </div>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (text.trim() && net.say(text)) setText("");
            }}
          >
            <label>
              {t("Message")}
              <input value={text} onChange={(event) => setText(event.target.value)} maxLength={1600} />
            </label>
            <button disabled={!net.ready || !text.trim()}>{t("Send")}</button>
          </form>
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await api("/api/reef/consent", "PUT", { version, age_band: "13_plus", accepted: false });
                setEligible(false);
                net.chat.clear();
              })
            }
          >
            {t("Withdraw chat consent")}
          </button>
        </>
      )}
      {blocked.length > 0 && (
        <details>
          <summary>{t("Blocked accounts")}</summary>
          {blocked.map((user) => (
            <div key={user}>
              {user}{" "}
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await api(`/api/reef/blocks/${user}`, "DELETE");
                    setBlocked((old) => old.filter((id) => id !== user));
                  })
                }
              >
                {t("Unblock")}
              </button>
            </div>
          ))}
        </details>
      )}
      {report && (
        <form
          className="reef-report"
          aria-label={t("Report message")}
          onSubmit={(event) => {
            event.preventDefault();
            void run(async () => {
              const result = await api<{ receipt: string }>("/api/reef/reports", "POST", {
                message_id: report.message_id,
                reason,
              });
              setNotice(`${t("Report receipt")}: ${result.receipt}`);
              setReport(null);
            });
          }}
        >
          <blockquote>
            {report.name}: {report.text}
          </blockquote>
          <label>
            {t("Reason")}
            <select value={reason} onChange={(event) => setReason(event.target.value)}>
              {["harassment", "personal_info", "spam", "other"].map((item) => (
                <option key={item} value={item}>
                  {t(item)}
                </option>
              ))}
            </select>
          </label>
          <button disabled={busy}>{t("Submit report")}</button>
          <button type="button" onClick={() => setReport(null)}>
            {t("Cancel")}
          </button>
        </form>
      )}
      <p role="status">{notice}</p>
    </section>
  );
}
