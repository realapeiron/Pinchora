import { t as deferredText } from "../../../i18n/dashboard";
import { InviteWordsContext, useInviteWords } from "../api/inviteWords";
import "../../../styles/invites.css";

import { useCallback, useState } from "react";
import { Link, useOutletContext } from "react-router";

import { PixelButton, PixelLink } from "../../../components/PixelButton";

import { Toast } from "../../../components/Toast";
import { type Overview } from "../../../lib/api";
import { inviteExportUrl, useInvites } from "../api/invites";

import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { invitesSummary } from "./invitesSummary";
import { InviteSettingsForm } from "./InviteSettingsForm";
import { Rewards } from "./InviteRewards";
import { Leaderboard } from "./InviteLeaderboard";

export function Invites() {
  const data = useOutletContext<Overview>();
  const invites = useInvites(data.guild.id);
  if (invites.query.isPending) return <Loading />;
  if (invites.query.isError) return <Problem error={invites.query.error} />;
  return (
    <InviteWordsContext.Provider value={invites.query.data}>
      <InvitesPage />
    </InviteWordsContext.Provider>
  );
}

function InvitesPage() {
  const { t } = useInviteWords();
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const invites = useInvites(guild);
  const [toast, setToast] = useState<string | null>(null);
  const closeToast = useCallback(() => setToast(null), []);
  const module = data.modules.find((m) => m.key === "invites");
  return (
    <SettingsPage
      className="settings-page inv-page"
      title={deferredText("INVITES")}
      lead={deferredText(
        "See where new members came from, reward real invitations, and give members short-lived links for friends.",
      )}
      action={
        <div className="inv-head-actions">
          <PixelButton
            variant="secondary"
            onClick={() => {
              void invites.query
                .refetch()
                .then(({ data }) =>
                  setToast(
                    data?.can_read_invites
                      ? deferredText("Invite list is available.")
                      : deferredText("Manage Server permission is missing for the bot."),
                  ),
                );
            }}
          >
            {deferredText("Test tracking")}
          </PixelButton>
          <PixelLink href={inviteExportUrl(guild)} variant="secondary">
            {deferredText("Export CSV")}
          </PixelLink>
        </div>
      }
    >
      {module && !module.enabled && (
        <p className="notice" role="note">
          <strong>{deferredText("Invite tracking is off.")}</strong> {t("Switch it on in")}{" "}
          <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link> {t("to count new joins.")}
        </p>
      )}
      {invites.query.isPending && <Loading label={deferredText("Reading invite settings")} />}
      {invites.query.isError && <Problem error={invites.query.error} />}
      {invites.query.data && (
        <>
          {!invites.query.data.can_read_invites && (
            <p className="notice" role="note">
              <strong>{deferredText("Manage Server permission missing.")}</strong>{" "}
              {deferredText(
                "Pinchora needs Manage Server to read the invite list. New joins stay unknown until the bot has it.",
              )}
            </p>
          )}
          <div className="inv-overview">
            <div className="inv-overview-mark" aria-hidden="true">
              ↗
            </div>
            <div>
              <strong>{deferredText("Every join has a source.")}</strong>
              <p>
                {deferredText(
                  "Invite uses are compared when someone joins. If Discord cannot identify a code, the join stays unknown; it is never credited by guesswork.",
                )}
              </p>
            </div>
          </div>
          <InviteSettingsForm
            guild={guild}
            saved={invites.query.data.settings}
            save={invites.save}
            onToast={setToast}
          />
          <div className="settings-detail-list">
            <SettingsRow
              rowId="invites-reward-roles"
              title={dashboardT("Reward roles", {}, "REWARD ROLES")}
              description={dashboardT("Set role rewards for eligible invites.")}
              summary={invitesSummary(invites.query.data.settings, invites.query.data.rewards).rewards}
            >
              <Rewards
                guild={guild}
                rewards={invites.query.data.rewards}
                limit={invites.query.data.limits.rewards}
                reward={invites.reward}
                remove={invites.remove}
                onToast={setToast}
              />
            </SettingsRow>
          </div>
          <Leaderboard guild={guild} />
        </>
      )}
      <Toast message={toast} onDone={closeToast} />
    </SettingsPage>
  );
}
