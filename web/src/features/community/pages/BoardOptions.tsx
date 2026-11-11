import { useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import type { GuildInfo } from "../../../lib/api";
import type { Board } from "../api/starboard";
import { boardSummary } from "./starboardSummary";
export function BoardOptions({
  board,
  info,
  change,
}: {
  board: Board;
  info: GuildInfo | undefined;
  change: (next: Partial<Board>) => void;
}) {
  const [role, setRole] = useState("");
  const [weight, setWeight] = useState(2);
  const channels = info?.channels.filter((c) => c.type === 0 || c.type === 5) ?? [];
  const summary = boardSummary(board, info);
  return (
    <>
      <SettingsRow
        title={dt("Only from channels")}
        description={dt("Limit which channels can send posts to this board.")}
        summary={summary.exclusions}
      >
        <div className="settings-detail-fields">
          <div className="star-options">
            <label>
              {" "}
              {spaTranslate("Only from channels")}{" "}
              <select
                className="pixel-input pixel-select"
                multiple
                value={board.only_channels}
                onChange={(e) =>
                  change({ only_channels: Array.from(e.target.selectedOptions, (o) => o.value) })
                }
              >
                {channels.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
              <small>
                {" "}
                {spaTranslate(
                  "Leave empty for all channels. Private sources remain excluded to protect their audience.",
                )}{" "}
              </small>
            </label>
            <label>
              {" "}
              {spaTranslate("Except channels")}{" "}
              <select
                className="pixel-input pixel-select"
                multiple
                value={board.except_channels}
                onChange={(e) =>
                  change({ except_channels: Array.from(e.target.selectedOptions, (o) => o.value) })
                }
              >
                {channels.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
        </div>
      </SettingsRow>
      {(
        [
          ["self_star", spaTranslate("Allow self stars"), dt("Count stars from the post author.")],
          ["allow_bots", spaTranslate("Include bot posts and stars"), dt("Include bot posts and reactions.")],
          ["require_image", spaTranslate("Require an image"), dt("Only collect posts with an image.")],
          [
            "remove_below",
            spaTranslate("Remove below threshold"),
            dt("Remove posts when their score falls."),
          ],
          ["enabled", spaTranslate("Board enabled"), dt("Collect posts for this board.")],
        ] as const
      ).map(([key, title, description]) => (
        <SettingsRow
          key={key}
          title={title}
          description={description}
          summary={dt(board[key] ? "On" : "Off")}
          enabled={board[key]}
          onToggle={(value) => change({ [key]: value })}
          toggleLabel={title}
        >
          <p className="muted">{description}</p>
        </SettingsRow>
      ))}
      <SettingsRow
        title={dt("Role weights")}
        description={dt("Give selected roles more voting weight.")}
        summary={summary.weights}
      >
        <div className="settings-detail-fields">
          <div className="star-weights">
            <strong>
              {spaTranslate("Role weights ·")} {Object.keys(board.role_weights).length}
              {spaTranslate("/25 used")}
            </strong>
            <p className="muted">
              {" "}
              {spaTranslate(
                "Members with a weighted role count for its highest weight. Everyone else counts once.",
              )}{" "}
            </p>
            <div className="star-weight-inputs">
              <select
                className="pixel-input pixel-select"
                aria-label={spaTranslate("Weighted role")}
                value={role}
                onChange={(e) => setRole(e.target.value)}
              >
                <option value="">{spaTranslate("Choose a role")}</option>
                {(info?.roles ?? [])
                  .filter((r) => !r.everyone)
                  .map((r) => (
                    <option key={r.id} value={r.id}>
                      @{r.name}
                    </option>
                  ))}
              </select>
              <input
                className="pixel-input"
                aria-label={spaTranslate("Role weight")}
                type="number"
                min={1}
                max={50}
                value={weight}
                onChange={(e) => setWeight(Number(e.target.value))}
              />
              <button
                type="button"
                disabled={!role || weight < 1 || weight > 50 || Object.keys(board.role_weights).length >= 25}
                onClick={() => change({ role_weights: { ...board.role_weights, [role]: weight } })}
              >
                {" "}
                {spaTranslate("Add weight")}{" "}
              </button>
            </div>
            {Object.entries(board.role_weights).map(([id, amount]) => (
              <span className="star-weight" key={id}>
                @{info?.roles.find((r) => r.id === id)?.name ?? id} ×{amount}{" "}
                <button
                  type="button"
                  aria-label={spaTranslate("Remove weight for {arg1}", { arg1: id })}
                  onClick={() => {
                    const next = { ...board.role_weights };
                    delete next[id];
                    change({ role_weights: next });
                  }}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        </div>
      </SettingsRow>
    </>
  );
}
