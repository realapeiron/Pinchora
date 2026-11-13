import { useEffect, useId, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { api } from "../../../lib/api";
import { locale, useLocale } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import { Icon } from "../../../components/icons/Icon";
import { trapTab } from "../../../components/overlayFocus";
import "../../../styles/notifications.css";

type Kind = "ticket" | "automod" | "report" | "appeal" | "modmail" | "members" | "audit";
export interface NotificationItem {
  id: string;
  kind: Kind;
  at: string;
  link: string;
  data: {
    number?: number;
    opener_id?: string;
    opener_name?: string;
    rule_name?: string;
    channel_id?: string | null;
    count?: number;
    user_id?: string;
    user_name?: string;
    actor_name?: string;
    action?: string;
  };
}
interface NotificationList {
  items: NotificationItem[];
  unread: number;
  read_at: string | null;
}

export function relativeTime(at: string, now = Date.now(), language = locale()) {
  const seconds = Math.max(0, Math.floor((now - Date.parse(at)) / 1000));
  const [amount, unit]: [number, Intl.RelativeTimeFormatUnit] =
    seconds < 60
      ? [Math.max(1, seconds), "second"]
      : seconds < 3600
        ? [Math.floor(seconds / 60), "minute"]
        : seconds < 86400
          ? [Math.floor(seconds / 3600), "hour"]
          : [Math.floor(seconds / 86400), "day"];
  return new Intl.RelativeTimeFormat(language, { numeric: "auto" }).format(-amount, unit);
}

function itemText(item: NotificationItem) {
  const d = item.data;
  switch (item.kind) {
    case "ticket":
      return [
        t("Ticket #{number} opened", { number: d.number ?? 0 }),
        d.opener_name ? t("Opened by {user}", { user: d.opener_name }) : "",
      ];
    case "automod":
      return [
        t((d.count ?? 0) === 1 ? "AutoMod acted once" : "AutoMod acted {count} times", {
          count: d.count ?? 0,
        }),
        [d.channel_id ? `#${d.channel_id}` : "", d.rule_name].filter(Boolean).join(" · "),
      ];
    case "report":
      return [t("New pending report"), t("Staff inbox")];
    case "appeal":
      return [t("New pending appeal"), t("Staff inbox")];
    case "modmail":
      return [t("Modmail thread opened"), d.user_name ? t("Member {user}", { user: d.user_name }) : ""];
    case "members":
      return [
        t((d.count ?? 0) === 1 ? "One new member joined" : "{count} new members joined", {
          count: d.count ?? 0,
        }),
        t("Last 24 hours"),
      ];
    case "audit":
      return [t("Dashboard changed"), d.actor_name ?? ""];
  }
}

const icons: Record<Kind, Parameters<typeof Icon>[0]["name"]> = {
  ticket: "ticket",
  automod: "shield",
  report: "flag",
  appeal: "message-square",
  modmail: "message-square",
  members: "user-plus",
  audit: "scroll-text",
};

function useNotifications(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/notifications`;
  const query = useQuery({
    queryKey: [path],
    queryFn: () => api<NotificationList>(path),
    refetchInterval: () => (document.visibilityState === "visible" ? 60_000 : false),
  });
  useEffect(() => {
    const visible = () => {
      if (document.visibilityState === "visible") void query.refetch();
    };
    document.addEventListener("visibilitychange", visible);
    return () => document.removeEventListener("visibilitychange", visible);
  }, [query.refetch]);
  const read = useMutation({
    mutationFn: (up_to: string) => api<{ read_at: string }>(`${path}/read`, "POST", { up_to }),
    onSuccess: ({ read_at }) =>
      client.setQueryData<NotificationList>(
        [path],
        (old) =>
          old && {
            ...old,
            read_at,
            unread: old.items.filter(
              (item) => item.kind !== "members" && Date.parse(item.at) > Date.parse(read_at),
            ).length,
          },
      ),
  });
  return { query, read };
}

export function NotificationBell({ guild }: { guild: string }) {
  const language = useLocale();
  const { query, read } = useNotifications(guild);
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const headingId = useId();
  const unread = query.data?.unread ?? 0;
  useEffect(() => {
    if (!open) return;
    void query.refetch();
    panel.current?.focus();
    const outside = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node)) {
        setOpen(false);
        trigger.current?.focus();
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        trigger.current?.focus();
      }
    };
    const tabs = (event: KeyboardEvent) => {
      if (panel.current?.contains(document.activeElement)) trapTab(event, panel.current);
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    document.addEventListener("keydown", tabs);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
      document.removeEventListener("keydown", tabs);
    };
  }, [open]);
  const newest = query.data?.items.find((item) => item.kind !== "members")?.at;
  return (
    <div className="notification-wrap" ref={wrap}>
      <button
        ref={trigger}
        type="button"
        className="notification-trigger"
        aria-label={unread ? t("Notifications, {count} unread", { count: unread }) : t("Notifications")}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <Icon name="bell" />
        {unread > 0 && (
          <span className="notification-badge" aria-hidden="true">
            {unread > 9 ? "9+" : unread}
          </span>
        )}
      </button>
      {open && (
        <div
          ref={panel}
          className="notification-pop shell-pop"
          role="dialog"
          aria-labelledby={headingId}
          tabIndex={-1}
        >
          <div className="notification-head">
            <strong id={headingId}>{t("Notifications")}</strong>
            <button
              type="button"
              disabled={!newest || read.isPending}
              onClick={() => newest && read.mutate(newest)}
            >
              {t("Mark all as read")}
            </button>
          </div>
          {query.isPending && <p role="status">{t("Loading notifications…")}</p>}
          {query.isError && <p role="alert">{t("Could not load notifications.")}</p>}
          {read.isError && <p role="alert">{t("Could not mark notifications as read.")}</p>}
          {query.data?.items.length === 0 && (
            <p className="notification-empty">{t("Nothing new in the last 7 days.")}</p>
          )}
          {query.data?.items.map((item) => {
            const [title, detail] = itemText(item);
            const isUnread =
              item.kind !== "members" &&
              (!query.data?.read_at || Date.parse(item.at) > Date.parse(query.data.read_at));
            return (
              <Link
                key={item.id}
                className={`notification-item${isUnread ? " is-unread" : ""}`}
                to={item.link}
                onClick={() => {
                  setOpen(false);
                  trigger.current?.focus();
                }}
              >
                <span
                  className={`notification-tile t-${item.kind === "ticket" ? "gold" : item.kind === "members" ? "mint" : "heart"}`}
                >
                  <Icon name={icons[item.kind]} />
                </span>
                <span className="notification-copy">
                  <strong>{title}</strong>
                  <small>
                    {detail}
                    {detail && " · "}
                    <time dateTime={item.at}>{relativeTime(item.at, Date.now(), language)}</time>
                  </small>
                </span>
                {isUnread && <span className="notification-new">{t("New")}</span>}
              </Link>
            );
          })}
        </div>
      )}
    </div>
  );
}
