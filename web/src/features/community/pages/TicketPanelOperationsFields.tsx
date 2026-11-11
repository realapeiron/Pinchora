import type { Dispatch, SetStateAction } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { PixelButton } from "../../../components/PixelButton";
import { type GuildInfo } from "../../../lib/api";
import { type PanelOperations } from "../api/ticket-operations";

type PanelEdit = { draft: PanelOperations; setDraft: Dispatch<SetStateAction<PanelOperations | null>> };
export function PanelFields({ draft, setDraft, info }: PanelEdit & { info?: GuildInfo }) {
  const numberField = (label: string, key: keyof PanelOperations, max: number) => (
    <label key={key}>
      {spaTranslate(label)}
      <input
        className="pixel-input"
        type="number"
        min={0}
        max={max}
        value={(draft?.[key] as number) ?? 0}
        onChange={(event) => draft && setDraft({ ...draft, [key]: Number(event.target.value) })}
      />
    </label>
  );
  return (
    <>
      <label>
        {spaTranslate("Claim mode")}
        <select
          className="pixel-input pixel-select"
          value={draft.claim_mode}
          onChange={(event) =>
            setDraft({ ...draft, claim_mode: event.target.value as PanelOperations["claim_mode"] })
          }
        >
          <option value="optional">{spaTranslate("Optional")}</option>
          <option value="required">{spaTranslate("Required")}</option>
          <option value="exclusive">{spaTranslate("Exclusive")}</option>
        </select>
      </label>
      <div className="tk-operations-grid">
        {numberField("Open tickets per panel (0 = unlimited)", "max_open_per_panel", 100)}
        {numberField("Open tickets per category (0 = unlimited)", "max_open_per_category", 100)}
        {numberField("Cooldown (seconds)", "cooldown_seconds", 86400)}
        {numberField("Minimum account age (days)", "minimum_account_age_days", 3650)}
        {numberField("Minimum membership (days)", "minimum_membership_days", 3650)}
        {numberField("Open claims per staff member (0 = unlimited)", "staff_open_limit", 100)}
      </div>
      <label>
        {spaTranslate("Overflow message")}
        <textarea
          className="pixel-input"
          rows={2}
          maxLength={1000}
          value={draft.overflow_message}
          onChange={(event) => setDraft({ ...draft, overflow_message: event.target.value })}
        />
      </label>
      <label>
        {spaTranslate("Bypass roles")}
        <select
          className="pixel-input pixel-select"
          multiple
          value={draft.bypass_roles}
          onChange={(event) =>
            setDraft({
              ...draft,
              bypass_roles: Array.from(event.target.selectedOptions, (option) => option.value),
            })
          }
        >
          {(info?.roles ?? []).map((role) => (
            <option key={role.id} value={role.id}>
              {role.name}
            </option>
          ))}
        </select>
      </label>
    </>
  );
}

export function PanelFaq({ draft, setDraft }: PanelEdit) {
  return (
    <div className="tk-operations-form">
      <strong>{spaTranslate("FAQ before opening")}</strong>
      {draft.faq.map((faq, index) => (
        <div className="tk-operations-faq" key={index}>
          <input
            className="pixel-input"
            aria-label={spaTranslate("Question")}
            maxLength={150}
            value={faq.question}
            onChange={(event) =>
              setDraft({
                ...draft,
                faq: draft.faq.map((f, i) => (i === index ? { ...f, question: event.target.value } : f)),
              })
            }
          />
          <textarea
            className="pixel-input"
            aria-label={spaTranslate("Answer")}
            maxLength={1000}
            rows={2}
            value={faq.answer}
            onChange={(event) =>
              setDraft({
                ...draft,
                faq: draft.faq.map((f, i) => (i === index ? { ...f, answer: event.target.value } : f)),
              })
            }
          />
          <PixelButton
            type="button"
            variant="ghost"
            onClick={() => setDraft({ ...draft, faq: draft.faq.filter((_, i) => i !== index) })}
          >
            {spaTranslate("Remove")}
          </PixelButton>
        </div>
      ))}
      <PixelButton
        type="button"
        variant="secondary"
        disabled={draft.faq.length >= 10}
        onClick={() => setDraft({ ...draft, faq: [...draft.faq, { question: "", answer: "" }] })}
      >
        {spaTranslate("Add FAQ answer")}
      </PixelButton>
    </div>
  );
}
