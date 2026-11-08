import { translate as t } from "../../i18n";
import { IdList } from "../../components/IdList";
import type { Editor } from "./useWorkflowEditor";
export function FakeEvent({ editor }: { editor: Editor }) {
  const { discord, fake, setFake, fakeVars, setFakeVars, cooldowns, setCooldowns } = editor;
  return (
    <details>
      <summary>{t("Fake event")}</summary>
      <div className="wf-fake">
        {(["user_id", "channel_id", "message_id", "value", "level", "account_age_days", "roll"] as const).map(
          (key) => (
            <label key={key}>
              {t(
                {
                  user_id: "User ID",
                  channel_id: "Channel",
                  message_id: "Message ID",
                  value: "Event value",
                  level: "Level",
                  account_age_days: "Account age in days",
                  roll: "Random seed",
                }[key],
              )}
              <input
                className="pixel-input"
                value={fake[key]}
                onChange={(e) =>
                  setFake({
                    ...fake,
                    [key]: typeof fake[key] === "number" ? Number(e.target.value) : e.target.value,
                  })
                }
              />
            </label>
          ),
        )}
      </div>
      <IdList
        kind="role"
        info={discord.data}
        ids={fake.roles}
        onChange={(roles) => setFake({ ...fake, roles })}
        max={25}
        name={t("Held roles")}
        none={t("No roles")}
      />
      <label>
        {t("Variable values (scope:key=value, one per line)")}
        <textarea className="pixel-input" value={fakeVars} onChange={(e) => setFakeVars(e.target.value)} />
      </label>
      <label>
        {t("Failed cooldown indexes (comma separated)")}
        <input className="pixel-input" value={cooldowns} onChange={(e) => setCooldowns(e.target.value)} />
      </label>
    </details>
  );
}
