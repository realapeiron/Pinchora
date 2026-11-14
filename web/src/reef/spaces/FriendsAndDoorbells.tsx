import { Link } from "react-router";
import { api } from "../../lib/api";
import { t } from "../i18n";
import type { RunAction } from "../SpacesPanel";

export type Person = {
  id: string;
  name: string;
  partner: boolean;
  online: boolean;
  visitable: boolean;
  doorbell: boolean;
};
export type Bell = { id: string; host_id: string; guest_id: string; status: string };

export function FriendsAndDoorbells({
  people,
  socialPending,
  bellItems,
  user,
  busy,
  run,
}: {
  people: Person[] | undefined;
  socialPending: boolean;
  bellItems: Bell[] | undefined;
  user: string;
  busy: boolean;
  run: RunAction;
}) {
  const visit = (host: string) => `/house/${host}?play=1`;
  return (
    <>
      <h3>{t("Friends and partner")}</h3>
      {socialPending && <p>{t("Loading…")}</p>}
      <ul className="reef-objects">
        {people?.map((person) => (
          <li key={person.id}>
            <span>
              {person.name || person.id}
              {person.partner ? ` · ${t("Partner")}` : ""}
              {person.online ? ` · ${t("Online")}` : ""}
            </span>
            {person.visitable ? (
              <Link to={visit(person.id)}>{t("Visit")}</Link>
            ) : person.doorbell ? (
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void run(() => api(`/api/reef/houses/${person.id}/doorbells`, "POST", {}), "Doorbell sent")
                }
              >
                {t("Ring doorbell")}
              </button>
            ) : null}
          </li>
        ))}
      </ul>
      <h3>{t("Doorbells")}</h3>
      <ul className="reef-objects">
        {bellItems?.map((b) => (
          <li key={b.id}>
            <span>{b.host_id === user ? b.guest_id : b.host_id}</span>
            {b.host_id === user && b.status === "pending" ? (
              <>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(() => api(`/api/reef/doorbells/${b.id}/decision`, "POST", { accept: true }))
                  }
                >
                  {t("Accept")}
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(() => api(`/api/reef/doorbells/${b.id}/decision`, "POST", { accept: false }))
                  }
                >
                  {t("Decline")}
                </button>
              </>
            ) : b.guest_id === user && b.status === "accepted" ? (
              <Link to={visit(b.host_id)}>{t("Visit")}</Link>
            ) : (
              <span>
                {t(b.status === "pending" ? "Pending" : b.status === "accepted" ? "Accepted" : "Declined")}
              </span>
            )}
          </li>
        ))}
      </ul>
    </>
  );
}
