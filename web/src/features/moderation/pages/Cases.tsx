import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import "../../../styles/cases.css";
import { useState } from "react";
import { useOutletContext, useSearchParams } from "react-router";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import type { Overview } from "../../../lib/api";
import { type CaseFilters, useCases } from "../../../lib/queries";
import { useDebounced } from "../../../lib/useDebounced";
import { t } from "../../../i18n/dashboard";
import { CaseList } from "./CaseList";
import { CaseSheet } from "./CaseSheet";
import { caseFromSearch, kindCounts, searchWithCase, visibleKinds } from "./caseView";

export function Cases() {
  const guild = useOutletContext<Overview>().guild.id;
  const [params, setParams] = useSearchParams();
  const [typed, setTyped] = useState("");
  const [kind, setKind] = useState("");
  const [active, setActive] = useState(false);
  const q = useDebounced(typed.trim(), 300);
  const filters: CaseFilters = { q, kind, member: "", active };
  const cases = useCases(guild, filters);
  const rows = cases.data?.pages.flatMap((page) => page.items) ?? [];
  const kinds = cases.data?.pages[0]?.kinds ?? [];
  const number = caseFromSearch(params.toString());
  const open = (value: number) => {
    setParams(searchWithCase(params.toString(), value));
  };
  const close = () => {
    setParams(searchWithCase(params.toString(), null), { replace: true });
  };
  return (
    <SettingsPage
      className="settings-page cases-page"
      title={spaTranslate("CASES")}
      lead={deferredText(
        "Every moderation action taken with Pinchora, numbered per server. Open one to see it whole and give it a better reason.",
      )}
    >
      <Panel title={t("Find cases")}>
        <div className="cases-filters">
          <label className="cases-search" htmlFor="case-search">
            <span className="field-label">{t("Search cases")}</span>
            <input
              id="case-search"
              type="search"
              className="pixel-input"
              value={typed}
              onChange={(event) => setTyped(event.target.value)}
              placeholder={t("Member, moderator or reason")}
              title={deferredText("spam, Mia, 1234…")}
              aria-describedby="case-search-hint"
            />
          </label>
          <span id="case-search-hint" className="visually-hidden">
            {deferredText("A name, a reason, a word in the details, or a user id.")}
          </span>
          <div className="cases-tabs" role="group" aria-label={t("Kind")}>
            <button
              type="button"
              aria-pressed={!kind}
              title={deferredText("Every kind")}
              onClick={() => setKind("")}
            >
              {t("All")} <span>{kindCounts(kinds)}</span>
            </button>
            {visibleKinds(kinds, kind).map((item) => (
              <button
                key={item.key}
                type="button"
                aria-pressed={kind === item.key}
                onClick={() => setKind(item.key)}
              >
                {t(item.label)} <span>{item.count}</span>
              </button>
            ))}
          </div>
          <div
            className="cases-active"
            title={deferredText("Only bans, warns and timeouts that still stand")}
          >
            <span className="field-label" title={deferredText("Still standing")}>
              {t("Active only")}
            </span>
            <PixelToggle label={t("Active only")} checked={active} onChange={setActive} />
          </div>
        </div>
      </Panel>
      <Panel title={deferredText("ALL CASES")}>
        {cases.isPending && <Loading label={deferredText("Reading the cases")} />}
        {cases.isError && <Problem error={cases.error} />}
        {cases.isSuccess && rows.length === 0 && (
          <div className="cases-empty">
            <p>
              {q || kind || active
                ? deferredText("No case matches.")
                : deferredText("No cases yet. /warn, /timeout, /ban and the others open them.")}
            </p>
            {q && (
              <PixelButton variant="secondary" onClick={() => setTyped("")}>
                {t("Clear search")}
              </PixelButton>
            )}
          </div>
        )}
        {rows.length > 0 && <CaseList rows={rows} onOpen={open} />}
        {cases.hasNextPage && (
          <div className="panel-foot">
            <PixelButton
              variant="secondary"
              disabled={cases.isFetchingNextPage}
              onClick={() => void cases.fetchNextPage()}
            >
              {cases.isFetchingNextPage ? t("Loading…") : deferredText("Older cases")}
            </PixelButton>
          </div>
        )}
      </Panel>
      {number !== null && <CaseSheet guild={guild} number={number} onClose={close} onOpen={open} />}
    </SettingsPage>
  );
}
