import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { Field } from "../../../components/Field";
import { RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { ApiError } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useInviteWords } from "../api/inviteWords";
import { useInvites } from "../api/invites";

export function Rewards({
  guild,
  rewards,
  limit,
  reward,
  remove,
  onToast,
}: {
  guild: string;
  rewards: { invites: number; role_id: string }[];
  limit: number;
  reward: ReturnType<typeof useInvites>["reward"];
  remove: ReturnType<typeof useInvites>["remove"];
  onToast: (s: string) => void;
}) {
  const { t } = useInviteWords();
  const discord = useDiscord(guild);
  const [at, setAt] = useState(5);
  const [role, setRole] = useState<string | null>(null);
  const error = reward.error instanceof ApiError ? reward.error : null;
  return (
    <div className="settings-detail-fields">
      <span className="badge badge-gold">
        {deferredText("{count}/{limit} used", { count: rewards.length, limit })}
      </span>
      <p className="panel-lead muted">
        {deferredText(
          "A member earns a role when enough of their invitees are still here with eligible accounts. The role is checked again before Pinchora gives it.",
        )}
      </p>
      {rewards.length === 0 ? (
        <p className="empty-row muted">{deferredText("No milestones yet. Add the first one below.")}</p>
      ) : (
        <ul className="list" aria-label={deferredText("Invite rewards")}>
          {rewards.map((r) => (
            <li className="list-row inv-reward" key={r.invites}>
              <span className="inv-reward-count">
                {deferredText("{count} invites", { count: r.invites })}
              </span>
              <span>→</span>
              <strong>
                {discord.data?.roles.find((role) => role.id === r.role_id)?.name ??
                  deferredText("Role {id}", { id: r.role_id })}
              </strong>
              <PixelButton
                variant="ghost"
                aria-label={deferredText("Remove reward at {arg1}", {
                  arg1: deferredText("{count} invites", { count: r.invites }),
                })}
                onClick={() =>
                  remove.mutate(r.invites, { onSuccess: () => onToast(deferredText("Reward removed.")) })
                }
              >
                {t("Remove")}
              </PixelButton>
            </li>
          ))}
        </ul>
      )}
      <div className="inv-add-reward">
        <Field
          label={deferredText("Invites needed")}
          htmlFor="inv-threshold"
          error={error?.field === "invites" ? error.message : null}
        >
          <input
            id="inv-threshold"
            type="number"
            min="1"
            max="1000000"
            className="pixel-input"
            value={at}
            onChange={(e) => setAt(Number(e.target.value))}
          />
        </Field>
        <Field label={t("Role")} htmlFor="inv-role" error={error?.field === "role_id" ? error.message : null}>
          <RolePicker
            id="inv-role"
            info={discord.data}
            value={role}
            onChange={setRole}
            assignable
            none={deferredText("Choose a role")}
          />
        </Field>
        <PixelButton
          disabled={!role || at < 1 || rewards.length >= limit || reward.isPending}
          onClick={() =>
            role &&
            reward.mutate(
              { invites: at, role_id: role },
              {
                onSuccess: () => {
                  setRole(null);
                  onToast(deferredText("Reward saved."));
                },
              },
            )
          }
        >
          {deferredText("Add reward")}
        </PixelButton>
      </div>
    </div>
  );
}
