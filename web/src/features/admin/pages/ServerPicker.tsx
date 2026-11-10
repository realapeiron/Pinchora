import { useEffect, useRef, useState } from "react";
import { NavLink } from "react-router";
import { Icon } from "../../../components/icons/Icon";
import { ServerIcon } from "../../../components/ServerIcon";
import { t } from "../../../i18n/dashboard";
import type { GuildEntry } from "../../../lib/api";
import { trapTab } from "./overlayFocus";

export function ServerPicker({
  guildId,
  guildName,
  guildIcon,
  guilds,
  drawerOpen,
  serverOpenRequested,
  closeDrawer,
}: {
  guildId: string;
  guildName: string;
  guildIcon: string | null;
  guilds: GuildEntry[];
  drawerOpen: boolean;
  serverOpenRequested: boolean;
  closeDrawer: () => void;
}) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (serverOpenRequested) setOpen(true);
  }, [serverOpenRequested]);
  useEffect(() => {
    if (!drawerOpen && window.matchMedia("(max-width: 960px)").matches) setOpen(false);
  }, [drawerOpen]);
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLElement>("a")?.focus();
    const outside = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node)) {
        if (menu.current?.contains(document.activeElement)) trigger.current?.focus();
        setOpen(false);
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        (menu.current?.contains(document.activeElement) || trigger.current === document.activeElement)
      ) {
        setOpen(false);
        trigger.current?.focus();
      }
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);
  return (
    <div className="shell-server-wrap" ref={wrap}>
      <button
        ref={trigger}
        type="button"
        className="shell-server"
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen(!open)}
      >
        <ServerIcon id={guildId} name={guildName} url={guildIcon} size={36} />
        <span>
          <strong>{guildName}</strong>
          <small>{t("Switch server")}</small>
        </span>
        <Icon name="chevrons-up-down" />
      </button>
      {open && (
        <div
          ref={menu}
          className="shell-pop shell-servers"
          role="menu"
          tabIndex={-1}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              setOpen(false);
              trigger.current?.focus();
            } else trapTab(event, event.currentTarget);
          }}
        >
          {guilds
            .filter((g) => g.bot_present)
            .map((g) => (
              <NavLink
                key={g.id}
                role="menuitem"
                to={`/g/${g.id}`}
                onClick={() => {
                  setOpen(false);
                  trigger.current?.focus();
                  closeDrawer();
                }}
              >
                <ServerIcon id={g.id} name={g.name} url={g.icon_url} size={30} />
                <span>{g.name}</span>
                {g.id === guildId && <Icon name="check" />}
              </NavLink>
            ))}
          <a role="menuitem" href="/invite" onClick={() => trigger.current?.focus()}>
            <Icon name="plus" />
            {t("Add a server")}
          </a>
          <NavLink
            role="menuitem"
            to="/servers"
            onClick={() => {
              setOpen(false);
              trigger.current?.focus();
            }}
          >
            {t("Switch server")}
          </NavLink>
        </div>
      )}
    </div>
  );
}
