import { useEffect, useMemo, useRef } from "react";
import { Link, useSearchParams } from "react-router";

import { Amp } from "./Amp";
import { InviteButton } from "./Buttons";
import { type Catalog, type CommandEntry, countCommands, filterAreas } from "./commandsSearch";
import catalogJson from "../lib/commands.json";

const catalog = catalogJson as Catalog;

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
          <span className="cmd-manager" title="Needs the Manage Server permission">
            MANAGE SERVER
          </span>
        )}
      </div>
      <p className="cmd-description">{command.description}</p>
      {command.options.length > 0 && (
        <details className="cmd-options">
          <summary>
            {command.options.length} {command.options.length === 1 ? "option" : "options"}
          </summary>
          <dl>
            {command.options.map((o) => (
              <div key={o.name} className="cmd-option">
                <dt>
                  <code>{o.name}</code> <span className="muted">{o.required ? "required" : "optional"}</span>
                </dt>
                <dd>
                  {o.description}
                  {o.choices && <span className="muted"> One of: {o.choices.join(", ")}.</span>}
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
  const [params, setParams] = useSearchParams();
  const query = params.get("q") ?? "";
  const managersOnly = params.get("managers") === "1";
  const search = useRef<HTMLInputElement>(null);

  const areas = useMemo(() => filterAreas(catalog.areas, query, managersOnly), [query, managersOnly]);
  const shown = countCommands(areas);

  const update = (next: { q?: string; managers?: boolean }) => {
    const copy = new URLSearchParams(params);
    const q = next.q ?? query;
    const managers = next.managers ?? managersOnly;
    if (q) copy.set("q", q);
    else copy.delete("q");
    if (managers) copy.set("managers", "1");
    else copy.delete("managers");
    setParams(copy, { replace: true, preventScrollReset: true });
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
        <p className="pixel-label eyebrow">COMMANDS</p>
        <h1 className="section-title">Everything Pinchora can do</h1>
        <p className="section-lead">
          {catalog.count} slash commands in {catalog.areas.length} areas, listed from the same command tree
          the bot answers with. Type <code>/help</code> in Discord for the same guide as pixel cards.
        </p>
      </header>

      <div className="commands-tools">
        <label className="commands-search" htmlFor="command-search">
          <span className="visually-hidden">Search commands</span>
          <input
            id="command-search"
            ref={search}
            type="search"
            value={query}
            placeholder="Search, e.g. fish or welcome"
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
          <span>Only for server managers</span>
        </label>
        <p className="commands-count muted" role="status" aria-live="polite">
          {shown === catalog.count ? `${shown} commands` : `${shown} of ${catalog.count} commands`}
        </p>
      </div>

      <div className="commands-layout">
        <nav className="commands-nav" aria-label="Areas">
          <ul>
            {areas.map((area) => (
              <li key={area.key}>
                <a href={`#${area.key}`}>
                  {area.title} <span className="muted">{area.commands.length}</span>
                </a>
              </li>
            ))}
          </ul>
        </nav>
        <div className="commands-list">
          {areas.length === 0 && (
            <div className="panel commands-empty">
              <div className="panel-body">
                <p className="pixel-title">NOTHING WASHED UP</p>
                <p>
                  No command matches <b>{query}</b>.{" "}
                  <button
                    type="button"
                    className="link-button"
                    onClick={() => update({ q: "", managers: false })}
                  >
                    Show every command
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
                <Amp text={area.title} />
              </h2>
              <p className="muted commands-area-intro">{area.intro}</p>
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
            Want to try them? <Link to="/#how">Here is how to get started</Link>, or
          </p>
          <InviteButton />
        </div>
      </div>
    </div>
  );
}
