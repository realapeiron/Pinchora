import { useEffect, useMemo, useRef } from "react";
import { Link, useSearchParams } from "react-router";
import { number, t, translate, useLocale } from "../i18n";

import { Amp } from "./Amp";
import { InviteButton } from "./Buttons";
import { type Catalog, type CommandEntry, countCommands, filterAreas } from "./commandsSearch";
import catalogJson from "../lib/commands.json";
import catalogTrJson from "../lib/commands.tr.json";

const enCatalog = catalogJson as Catalog;
const trCatalog = catalogTrJson as Catalog;

/** `/give <user> <amount>` with the words coloured by what they are. */
function Usage({ command }: { command: CommandEntry }) {
  return (
    <code className="cmd-usage">
      <span className="cmd-path">/{command.path}</span>
      {command.options.map((o) => (
        <span key={o.name} className={o.required ? "cmd-arg cmd-arg-required" : "cmd-arg"}>
          {o.required ? ` <${o.name}>` : ` [${o.name}]`}
        </span>
      ))}
    </code>
  );
}

function Command({ command }: { command: CommandEntry }) {
  return (
    <li className="cmd">
      <div className="cmd-head">
        <Usage command={command} />
        {command.manager && (
          <span className="cmd-manager" title={t("Needs the Manage Server permission")}>
            {t("MANAGE SERVER")}
          </span>
        )}
      </div>
      <p className="cmd-description">{translate(command.description)}</p>
      {command.options.length > 0 && (
        <details className="cmd-options">
          <summary>
            {number(command.options.length)} {command.options.length === 1 ? t("option") : t("options")}
          </summary>
          <dl>
            {command.options.map((o) => (
              <div key={o.name} className="cmd-option">
                <dt>
                  <code>{o.name}</code>{" "}
                  <span className="muted">{o.required ? t("required") : t("optional")}</span>
                </dt>
                <dd>
                  {translate(o.description)}
                  {o.choices && (
                    <span className="muted">
                      {" "}
                      {t("One of:")} {o.choices.join(", ")}.
                    </span>
                  )}
                </dd>
              </div>
            ))}
          </dl>
        </details>
      )}
    </li>
  );
}

export function Commands() {
  const language = useLocale();
  const catalog = language === "tr" ? trCatalog : enCatalog;
  const [params, setParams] = useSearchParams();
  const query = params.get("q") ?? "";
  const managersOnly = params.get("managers") === "1";
  const search = useRef<HTMLInputElement>(null);

  const areas = useMemo(
    () => filterAreas(catalog.areas, query, managersOnly),
    [catalog, query, managersOnly],
  );
  const shown = countCommands(areas);

  const update = (next: { q?: string; managers?: boolean }) => {
    const copy = new URLSearchParams(params);
    const q = next.q ?? query;
    const managers = next.managers ?? managersOnly;
    if (q) copy.set("q", q);
    else copy.delete("q");
    if (managers) copy.set("managers", "1");
    else copy.delete("managers");
    // These URL values control native inputs; a deferred router transition restores the old
    // checked/value state at the end of the input event before the navigation commits.
    setParams(copy, { replace: true, preventScrollReset: true, flushSync: true });
  };

  // "/" jumps to the search, as on most sites with a lot to look through.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable);
      if (event.key === "/" && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        search.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="wrap commands-page">
      <header className="page-intro">
        <p className="pixel-label eyebrow">{t("COMMANDS")}</p>
        <h1 className="section-title">{t("Everything Pinchora can do")}</h1>
        <p className="section-lead">
          {t(
            "{count} slash commands in {areas} areas, listed from the same command tree the bot answers with. Type",
            { count: catalog.count, areas: catalog.areas.length },
          )}{" "}
          <code>/help</code> {t("in Discord for the same guide as pixel cards.")}
        </p>
      </header>

      <div className="commands-tools">
        <label className="commands-search" htmlFor="command-search">
          <span className="visually-hidden">{t("Search commands")}</span>
          <input
            id="command-search"
            ref={search}
            type="search"
            value={query}
            placeholder={t("Search, e.g. fish or welcome")}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => update({ q: e.target.value })}
          />
          <kbd className="commands-hint" aria-hidden="true">
            /
          </kbd>
        </label>
        <label className="commands-managers">
          <input
            type="checkbox"
            checked={managersOnly}
            onChange={(e) => update({ managers: e.target.checked })}
          />
          <span>{t("Only for server managers")}</span>
        </label>
        <p className="commands-count muted" role="status" aria-live="polite">
          {shown === catalog.count
            ? t("{count} commands", { count: shown })
            : t("{shown} of {count} commands", { shown, count: catalog.count })}
        </p>
      </div>

      <div className="commands-layout">
        <nav className="commands-nav" aria-label={t("Areas")}>
          <ul>
            {areas.map((area) => (
              <li key={area.key}>
                <a href={`#${area.key}`}>
                  {translate(area.title)} <span className="muted">{number(area.commands.length)}</span>
                </a>
              </li>
            ))}
          </ul>
        </nav>
        <div className="commands-list">
          {areas.length === 0 && (
            <div className="panel commands-empty">
              <div className="panel-body">
                <p className="pixel-title">{t("NOTHING WASHED UP")}</p>
                <p>
                  {t("No command matches")} <b>{query}</b>.{" "}
                  <button
                    type="button"
                    className="link-button"
                    onClick={() => update({ q: "", managers: false })}
                  >
                    {t("Show every command")}
                  </button>
                </p>
              </div>
            </div>
          )}
          {areas.map((area) => (
            <section
              key={area.key}
              id={area.key}
              className="commands-area"
              aria-labelledby={`${area.key}-title`}
            >
              <h2 id={`${area.key}-title`} className="pixel-title commands-area-title">
                <Amp text={translate(area.title)} />
              </h2>
              <p className="muted commands-area-intro">{translate(area.intro)}</p>
              <ul className="cmd-list">
                {area.commands.map((c) => (
                  <Command key={c.path} command={c} />
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>

      <div className="commands-cta panel">
        <div className="panel-body">
          <p>
            {t("Want to try them?")} <Link to="/#how">{t("Here is how to get started")}</Link>, {t("or")}
          </p>
          <InviteButton />
        </div>
      </div>
    </div>
  );
}
