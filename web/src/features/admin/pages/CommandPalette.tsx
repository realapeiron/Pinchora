import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { translate } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import type { ModuleEntry } from "../../../lib/api";
import { visibleNav, type NavGroup } from "../../../lib/nav";
import { canOpenPage, type AccessOverview } from "../../moderation";
import { Icon } from "../../../components/icons/Icon";
import { Toast } from "../../../components/Toast";
import { navModule } from "./navModules";
import { trapTab } from "../../../components/overlayFocus";
import { useModuleToggle } from "./useModuleToggle";

interface Item {
  label: string;
  hint: string;
  run: () => void;
}
export function CommandPalette({
  guildId,
  groups,
  modules,
  overview,
  open,
  onClose,
  returnTo,
}: {
  guildId: string;
  groups: NavGroup[];
  modules: ModuleEntry[];
  overview: AccessOverview | undefined;
  open: boolean;
  onClose: () => void;
  returnTo: HTMLElement | null;
}) {
  const navigate = useNavigate();
  const moduleToggle = useModuleToggle(guildId);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!open) return;
    returnFocus.current = returnTo ?? (document.activeElement as HTMLElement);
    input.current?.focus();
    return () => returnFocus.current?.focus();
  }, [open, returnTo]);
  const close = () => {
    onClose();
    setQuery("");
    setSelected(0);
  };
  const pages: Item[] = visibleNav(groups, query).flatMap((g) =>
    g.pages
      .filter((p) => canOpenPage(overview, p.to))
      .map((p) => ({
        label: p.label,
        hint: translate(g.group),
        run: () => navigate(p.to ? `/g/${guildId}/${p.to}` : `/g/${guildId}`),
      })),
  );
  const allowedModules = new Set(
    groups.flatMap((g) => g.pages.filter((p) => canOpenPage(overview, p.to)).map((p) => navModule(p.to))),
  );
  const actions: Item[] = modules
    .filter((m) => canOpenPage(overview, "modules") && (overview?.access?.full || allowedModules.has(m.key)))
    .filter((m) =>
      `${m.label} ${m.blurb} ${m.key} ${m.enabled ? "disable" : "enable"}`
        .toLocaleLowerCase()
        .includes(query.toLocaleLowerCase()),
    )
    .map((m) => ({
      label: `${m.label}: ${t(m.enabled ? "Turn off" : "Turn on")}`,
      hint: t("Module"),
      run: () => moduleToggle.toggle(m),
    }));
  const items = [...(query ? pages : pages.slice(0, 5)), ...(query ? actions : actions.slice(0, 3))];
  const activate = (index: number) => {
    const item = items[index];
    if (!item) return;
    close();
    item.run();
  };
  return (
    <>
      <Toast
        message={moduleToggle.notice}
        onDone={moduleToggle.clear}
        action={moduleToggle.undo ? { label: t("Undo"), onClick: moduleToggle.undo } : undefined}
      />
      {open && (
        <div className="command-backdrop">
          <button type="button" className="command-underlay" aria-label={t("Close menu")} onClick={close} />
          <div
            ref={dialog}
            className="command-box"
            role="dialog"
            aria-modal="true"
            aria-label={t("Search pages")}
          >
            <div className="command-input">
              <Icon name="search" />
              <input
                ref={input}
                role="combobox"
                aria-autocomplete="list"
                aria-controls="command-list"
                aria-expanded="true"
                aria-activedescendant={items.length ? `command-${selected}` : undefined}
                value={query}
                placeholder={t("Search pages and actions")}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setSelected(0);
                }}
                onKeyDown={(e) => {
                  if (dialog.current) trapTab(e, dialog.current);
                  if (e.key === "Escape") {
                    e.preventDefault();
                    close();
                  }
                  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                    e.preventDefault();
                    setSelected(
                      (selected + (e.key === "ArrowDown" ? 1 : -1) + items.length) %
                        Math.max(items.length, 1),
                    );
                  }
                  if (e.key === "Enter") {
                    e.preventDefault();
                    activate(selected);
                  }
                }}
              />
              <button
                type="button"
                className="command-close"
                aria-label={t("Close menu")}
                onClick={close}
                onKeyDown={(event) => {
                  if (dialog.current) trapTab(event, dialog.current);
                  if (event.key === "Escape") close();
                }}
              >
                <Icon name="x" />
              </button>
            </div>
            <div id="command-list" className="command-list" role="listbox" aria-label={t("Search results")}>
              {items.map((item, index) => (
                <button
                  key={`${item.hint}-${item.label}`}
                  type="button"
                  id={`command-${index}`}
                  role="option"
                  tabIndex={-1}
                  aria-selected={selected === index}
                  onMouseEnter={() => setSelected(index)}
                  onClick={() => activate(index)}
                >
                  <span>{item.label}</span>
                  <small>{item.hint}</small>
                </button>
              ))}
              {!items.length && <div className="command-empty">{t("No page matches.")}</div>}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
