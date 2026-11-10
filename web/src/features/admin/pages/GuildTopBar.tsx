import { useEffect, useRef, useState } from "react";
import { Link } from "react-router";
import type { Me } from "../../../lib/api";
import { t, translate } from "../../../i18n";
import { useLogout } from "../../../lib/queries";
import { Icon } from "../../../components/icons/Icon";

import { GuildLanguageMenu } from "./GuildLanguageMenu";
import { trapTab } from "./overlayFocus";

export function GuildTopBar({
  me,
  guild,
}: {
  me: Me;
  guild: { name: string; group: string; page: string; openDrawer: () => void };
}) {
  const logout = useLogout();
  const [accountOpen, setAccountOpen] = useState(false);
  const account = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const name = me.user.global_name ?? me.user.username;
  useEffect(() => {
    if (!accountOpen) return;
    menu.current?.querySelector<HTMLElement>("a, button")?.focus();
    const outside = (e: PointerEvent) => {
      if (!account.current?.contains(e.target as Node)) {
        if (menu.current?.contains(document.activeElement)) trigger.current?.focus();
        setAccountOpen(false);
      }
    };
    const escape = (e: KeyboardEvent) => {
      if (
        e.key === "Escape" &&
        (menu.current?.contains(document.activeElement) || trigger.current === document.activeElement)
      ) {
        setAccountOpen(false);
        trigger.current?.focus();
      }
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [accountOpen]);
  return (
    <header className="top-bar shell-top">
      <button
        type="button"
        className="shell-top-server"
        onClick={guild.openDrawer}
        aria-label={t("Open menu")}
      >
        <Icon name="menu" />
        <span>{guild.name}</span>
        <Icon name="chevron-down" />
      </button>
      <div className="shell-crumbs">
        <span>{translate(guild.group)}</span>
        <Icon name="chevron-right" />
        <strong>{guild.page}</strong>
      </div>
      <nav className="top-bar-links" aria-label={t("Player pages")}>
        <Link to="/me" className="top-bar-link">
          {t("My world")}
        </Link>
      </nav>
      <div className="top-bar-user">
        <GuildLanguageMenu me={me} />
        <div ref={account} className="shell-account">
          <button
            ref={trigger}
            type="button"
            className="shell-account-trigger"
            aria-label={name}
            aria-haspopup="menu"
            aria-expanded={accountOpen}
            onClick={() => setAccountOpen(!accountOpen)}
          >
            <img className="avatar" src={me.user.avatar_url} alt="" width={34} height={34} />
            <span className="top-bar-name">{name}</span>
            <Icon name="chevron-down" />
          </button>
          {accountOpen && (
            <div
              ref={menu}
              role="menu"
              tabIndex={-1}
              className="shell-account-menu shell-pop"
              onKeyDown={(event) => trapTab(event, event.currentTarget)}
            >
              <Link
                role="menuitem"
                to="/me"
                onClick={() => {
                  setAccountOpen(false);
                  trigger.current?.focus();
                }}
              >
                {t("My world")}
              </Link>
              <button
                type="button"
                role="menuitem"
                onClick={() => logout.mutate()}
                disabled={logout.isPending}
              >
                <Icon name="log-out" />
                {t("Log out")}
              </button>
            </div>
          )}
        </div>
      </div>
    </header>
  );
}
