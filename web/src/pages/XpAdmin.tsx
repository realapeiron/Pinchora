import { useState } from "react";

import { Field } from "../components/Field";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { useEditXp, useImportJob, useImportMee6, useXpMembers } from "../lib/xpAdmin";
import { t, tf } from "../lib/xpAdminI18n";
import "./XpAdmin.css";

export function XpAdmin({ guild }: { guild: string }) {
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [operation, setOperation] = useState<"add" | "remove" | "set">("add");
  const [amount, setAmount] = useState("");
  const [mode, setMode] = useState<"merge" | "replace">("merge");
  const [confirm, setConfirm] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const members = useXpMembers(guild, search);
  const edit = useEditXp(guild);
  const start = useImportMee6(guild);
  const job = useImportJob(guild, jobId);
  const picked = members.data?.items.find((item) => item.id === selected);
  const validAmount =
    amount.trim() !== "" &&
    Number.isInteger(Number(amount)) &&
    Number(amount) >= 0 &&
    Number(amount) <= 1_000_000_000;

  return (
    <>
      <Panel title={t("MEMBER XP")}>
        <p className="muted panel-lead">
          {t("Find a member by name or Discord ID, then adjust their server XP.")}
        </p>
        <div className="form-grid">
          <Field label={t("Find member")} htmlFor="xp-member-search">
            <input
              id="xp-member-search"
              className="pixel-input"
              value={search}
              maxLength={80}
              onChange={(event) => {
                setSearch(event.target.value);
                setSelected(null);
              }}
              placeholder={t("Name or Discord ID")}
            />
          </Field>
          <Field
            label={t("Amount")}
            htmlFor="xp-member-amount"
            error={amount && !validAmount ? t("Enter a whole number from 0 to 1,000,000,000.") : null}
          >
            <input
              id="xp-member-amount"
              className="pixel-input input-short"
              type="number"
              min={0}
              max={1_000_000_000}
              value={amount}
              onChange={(event) => setAmount(event.target.value)}
            />
          </Field>
        </div>
        {members.isError && (
          <p className="field-error" role="alert">
            {members.error.message}
          </p>
        )}
        {members.data && members.data.items.length === 0 && (
          <p className="empty-row muted">{t("No members found.")}</p>
        )}
        {!!members.data?.items.length && (
          <ul className="list" aria-label={t("Member search results")}>
            {members.data.items.map((item) => (
              <li key={item.id} className="list-row">
                <label className="list-main">
                  <input
                    type="radio"
                    name="xp-member"
                    checked={selected === item.id}
                    onChange={() => setSelected(item.id)}
                  />{" "}
                  {item.name} <span className="muted">({item.id})</span>
                </label>
                <span className="pixel-label">{item.xp.toLocaleString()} XP</span>
              </li>
            ))}
          </ul>
        )}
        <div className="add-row">
          <Field label={t("Change")} htmlFor="xp-operation">
            <select
              id="xp-operation"
              className="pixel-input"
              value={operation}
              onChange={(event) => setOperation(event.target.value as typeof operation)}
            >
              <option value="add">{t("Add XP")}</option>
              <option value="remove">{t("Remove XP")}</option>
              <option value="set">{t("Set XP")}</option>
            </select>
          </Field>
          <div className="add-actions">
            <PixelButton
              disabled={!picked || !validAmount || edit.isPending}
              onClick={() => picked && edit.mutate({ user: picked.id, operation, amount: Number(amount) })}
            >
              {t("Save XP")}
            </PixelButton>
          </div>
        </div>
        {edit.isSuccess && (
          <p className="notice" role="status">
            {t("Member XP saved.")}
          </p>
        )}
        {edit.isError && (
          <p className="field-error" role="alert">
            {edit.error.message}
          </p>
        )}
      </Panel>

      <Panel title={t("IMPORT FROM MEE6")} tone="gold">
        <p className="muted panel-lead">
          {t("Copy member levels from MEE6. You can run one import per server every 24 hours.")}
        </p>
        <div className="form-grid">
          <Field label={t("When a member already has XP")} htmlFor="mee6-mode">
            <select
              id="mee6-mode"
              className="pixel-input"
              value={mode}
              onChange={(event) => {
                setMode(event.target.value as typeof mode);
                setConfirm(false);
              }}
            >
              <option value="merge">{t("Keep the higher XP")}</option>
              <option value="replace">{t("Use MEE6 XP")}</option>
            </select>
          </Field>
        </div>
        {!confirm ? (
          <div className="xp-import-actions">
            <PixelButton onClick={() => setConfirm(true)}>{t("Review import")}</PixelButton>
          </div>
        ) : (
          <div className="sync-status xp-import-actions">
            <p>
              {mode === "merge"
                ? t("Existing XP stays if it is higher than MEE6 XP.")
                : t("Imported members will use their MEE6 XP.")}{" "}
              {t("Confirm to start the import.")}
            </p>
            <PixelButton
              disabled={start.isPending}
              onClick={() =>
                start.mutate(mode, {
                  onSuccess: ({ job_id }) => {
                    setJobId(job_id);
                    setConfirm(false);
                  },
                })
              }
            >
              {t("Confirm import")}
            </PixelButton>{" "}
            <PixelButton variant="ghost" onClick={() => setConfirm(false)}>
              {t("Cancel")}
            </PixelButton>
          </div>
        )}
        {start.isError && (
          <p className="field-error" role="alert">
            {start.error.message}
          </p>
        )}
        {jobId && (
          <div className="sync-status" aria-live="polite">
            <div
              className="meter"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={job.data?.total ? Math.round((100 * job.data.checked) / job.data.total) : 0}
            >
              <span
                className="meter-fill"
                style={{
                  width: `${job.data?.total ? Math.round((100 * job.data.checked) / job.data.total) : 0}%`,
                }}
              />
            </div>
            <p>
              {job.data?.message ??
                tf("Checked {count} members. Import {status}.", {
                  count: job.data?.checked ?? 0,
                  status: t(job.data?.status ?? "starting"),
                })}
            </p>
          </div>
        )}
      </Panel>
    </>
  );
}
