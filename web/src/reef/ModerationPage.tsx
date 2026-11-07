import { useEffect, useState } from "react";
import { Link } from "react-router";
import { api } from "../lib/api";
import { t } from "./i18n";
import { safetyError } from "./ChatPanel";
import "./reef.css";
interface Report {
  id: string;
  message_id: string;
  reason: string;
  status: string;
  disposition: string | null;
  appeal: string | null;
  room: string;
  sender_id: string;
  text: string;
  created_at: string;
}
interface Receipt {
  id: string;
  status: string;
  disposition: string | null;
  appeal: string | null;
}
export function ModerationPage() {
  const [reports, setReports] = useState<Report[]>([]);
  const [receipts, setReceipts] = useState<Receipt[]>([]);
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [appeal, setAppeal] = useState("");
  const [operator, setOperator] = useState(false);
  const [sitewide, setSitewide] = useState(false);
  async function refresh() {
    const [queue, mine, profile] = await Promise.all([
      api<{ items: Report[] }>("/api/reef/moderation"),
      api<{ items: Receipt[] }>("/api/reef/reports"),
      api<{ operator: boolean }>("/api/reef/bootstrap"),
    ]);
    setReports(queue.items);
    setReceipts(mine.items);
    setOperator(profile.operator);
  }
  useEffect(() => {
    void refresh().catch((error) => setNotice(safetyError(error)));
  }, []);
  async function act(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
      await refresh();
      setNotice(t("Moderation action recorded."));
    } catch (error) {
      setNotice(safetyError(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <main className="reef-moderation">
      <h1>{t("Reports and moderation")}</h1>
      <Link to="/me/house">{t("Return to your house")}</Link>
      <p>{t("Hosts see reports in their house. Operators see all reports. Evidence access is audited.")}</p>
      {operator && (
        <label>
          <input type="checkbox" checked={sitewide} onChange={(event) => setSitewide(event.target.checked)} />
          {t("Apply across the site")}
        </label>
      )}
      <button disabled={busy} onClick={() => void act(async () => {})}>
        {t("Refresh")}
      </button>
      <h2>{t("Moderation queue")}</h2>
      {!reports.length && <p>{t("No reports.")}</p>}
      {reports.map((report) => (
        <article key={report.id}>
          <h3>
            #{report.id} · {t(report.status)}
          </h3>
          <p>
            {report.room} · {report.created_at}
          </p>
          <blockquote>
            {report.sender_id}: {report.text}
          </blockquote>
          <p>{t(report.reason)}</p>
          {report.appeal && (
            <p>
              {t("Appeal")}: {report.appeal}
            </p>
          )}
          {["no_action", "warned", "mute", "kick", "ban", "revoke"].map((kind) => (
            <button
              key={kind}
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await api("/api/reef/moderation", "POST", {
                    target: report.sender_id,
                    owner: operator && sitewide ? null : report.room.split(":")[1],
                    room_idx: null,
                    kind,
                    seconds: 3600,
                    report: report.id,
                  });
                })
              }
            >
              {t(kind)}
            </button>
          ))}
        </article>
      ))}
      <h2>{t("Your report receipts and decisions")}</h2>
      <label>
        {t("Appeal explanation")}
        <textarea maxLength={1600} value={appeal} onChange={(event) => setAppeal(event.target.value)} />
      </label>
      {receipts.map((receipt) => (
        <article key={receipt.id}>
          #{receipt.id} · {t(receipt.status)} · {receipt.disposition && t(receipt.disposition)}{" "}
          {receipt.appeal}
          {receipt.status === "resolved" && (
            <button
              disabled={busy || !appeal.trim()}
              onClick={() =>
                void act(async () => {
                  await api(`/api/reef/reports/${receipt.id}/appeal`, "POST", { text: appeal });
                  setAppeal("");
                })
              }
            >
              {t("Appeal")}
            </button>
          )}
        </article>
      ))}
      <p role="status">{notice}</p>
    </main>
  );
}
