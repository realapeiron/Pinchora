import { useState } from "react";
import { Field } from "../../../components/Field";
import { RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import type { GuildInfo } from "../../../lib/api";
import { validUserId, type AntiNukeConfig, type AntiNukeData, type words } from "../api/anti_nuke";

type Text = ReturnType<typeof words>;
export function AntiNukePeople({
  saved,
  config,
  t,
  info,
  locked,
  saving,
  summaryTrusted,
  summaryVip,
  onList,
}: {
  saved: AntiNukeData;
  config: AntiNukeConfig;
  t: Text;
  info: GuildInfo | undefined;
  locked: boolean;
  saving: boolean;
  summaryTrusted: string;
  summaryVip: string;
  onList: (key: "trusted_users" | "vip_roles", value: string[]) => void;
}) {
  const [user, setUser] = useState("");
  const [role, setRole] = useState<string | null>(null);
  const roleName = (id: string) => info?.roles.find((r) => r.id === id)?.name ?? id;
  return (
    <div className="antinuke-people-grid">
      <SettingsRow
        rowId="anti-nuke-people-trust"
        title={t("ui.trust")}
        description={t("ui.trust_hint")}
        summary={summaryTrusted}
      >
        <div className="settings-detail-fields">
          <p className="muted">{t("ui.trust_hint")}</p>
          <div className="antinuke-add">
            <Field label={t("ui.member")} htmlFor="an-user">
              <input
                id="an-user"
                className="pixel-input"
                inputMode="numeric"
                autoComplete="off"
                value={user}
                disabled={!saved.can_trust}
                onChange={(e) => setUser(e.target.value.trim())}
              />
            </Field>
            <PixelButton
              variant="secondary"
              disabled={
                !saved.can_trust ||
                saving ||
                !validUserId(user, saved.viewer) ||
                config.trusted_users.includes(user) ||
                config.trusted_users.length >= saved.max_list
              }
              onClick={() => {
                onList("trusted_users", [...config.trusted_users, user]);
                setUser("");
              }}
            >
              {t("ui.add")}
            </PixelButton>
          </div>
          <ul className="antinuke-members">
            {config.trusted_users.map((id) => (
              <li key={id}>
                <code>{id}</code>
                <PixelButton
                  variant="ghost"
                  disabled={!saved.can_trust || saving}
                  aria-label={`${t("ui.remove")} ${id}`}
                  onClick={() =>
                    onList(
                      "trusted_users",
                      config.trusted_users.filter((u) => u !== id),
                    )
                  }
                >
                  {t("ui.remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
        </div>
      </SettingsRow>
      <SettingsRow
        rowId="anti-nuke-people-vip"
        title={t("ui.vip")}
        description={t("ui.vip_hint")}
        summary={summaryVip}
      >
        <div className="settings-detail-fields">
          <p className="muted">{t("ui.vip_hint")}</p>
          <div className="antinuke-add">
            <Field label={t("ui.add_role")} htmlFor="an-vip">
              <RolePicker
                id="an-vip"
                info={info}
                value={role}
                none={t("ui.none")}
                disabled={locked}
                onChange={setRole}
              />
            </Field>
            <PixelButton
              variant="secondary"
              disabled={
                locked ||
                !role ||
                config.vip_roles.includes(role) ||
                config.vip_roles.length >= saved.max_list
              }
              onClick={() => {
                if (role) onList("vip_roles", [...config.vip_roles, role]);
                setRole(null);
              }}
            >
              {t("ui.add_role")}
            </PixelButton>
          </div>
          <ul className="antinuke-members">
            {config.vip_roles.map((id) => (
              <li key={id}>
                <span>{roleName(id)}</span>
                <PixelButton
                  variant="ghost"
                  disabled={locked}
                  aria-label={`${t("ui.remove")} ${roleName(id)}`}
                  onClick={() =>
                    onList(
                      "vip_roles",
                      config.vip_roles.filter((r) => r !== id),
                    )
                  }
                >
                  {t("ui.remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
        </div>
      </SettingsRow>
    </div>
  );
}
