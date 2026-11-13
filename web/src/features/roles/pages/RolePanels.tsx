import { t as deferredText } from "../../../i18n/dashboard";
import { plural as spaPlural, translate as spaTranslate } from "../../../i18n";
import { useState } from "react";
import { Panel } from "../../../components/Panel";
import { ChannelTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import { ApiError, type GuildInfo } from "../../../lib/api";
import {
  type Panel as PanelData,
  type PanelDraft,
  type RolesView,
  bodyOf,
  draftOf,
  editsOf,
  emptyDraft,
  newKey,
  problemWithDraft,
  useRolesWrite,
} from "../api/roles";
import { Editor } from "./RolePanelEditor";
import { SettingsRow } from "../../../components/SettingsRow";
import { rolePanelSummary, rolePanelsSummary } from "./roleSummary";
import { t } from "../../../i18n/dashboard";

type Editing = string | "new" | null;

export function RolePanels({
  guild,
  info,
  view,
}: {
  guild: string;
  info: GuildInfo | undefined;
  view: RolesView;
}) {
  const write = useRolesWrite(guild);
  const panels = view.panels.items;
  const [editing, setEditing] = useState<Editing>(null);
  const [saved, setSaved] = useState<PanelDraft>(emptyDraft());
  const [draft, setDraft] = useState<PanelDraft>(emptyDraft());
  const [toast, setToast] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const edits = editing ? editsOf(saved, draft) : 0;
  const local = editing ? problemWithDraft(draft) : null;
  const serverProblem = write.error instanceof ApiError ? write.error : null;

  const open = (panel: PanelData | null) => {
    write.reset();
    const next = panel ? draftOf(panel) : emptyDraft();
    setSaved(next);
    setDraft(panel ? draftOf(panel) : emptyDraft());
    setEditing(panel ? panel.id : "new");
  };

  const save = () =>
    write.mutate(
      {
        method: editing === "new" ? "POST" : "PUT",
        sub: editing === "new" ? "/panels" : `/panels/${editing}`,
        body: bodyOf(draft),
      },
      {
        onSuccess: (answer) => {
          const title = draft.title.trim().toLowerCase();
          const panel =
            answer.panels.items.find((p) => p.id === editing) ??
            answer.panels.items.find((p) => p.title.toLowerCase() === title);
          if (panel) {
            setSaved(draftOf(panel));
            setDraft(draftOf(panel));
            setEditing(panel.id);
          }
          setToast(deferredText("Panel saved."));
        },
      },
    );

  const current = panels.find((p) => p.id === editing) ?? null;

  return (
    <>
      <Panel
        title={deferredText("ROLE PANELS")}
        aside={
          <span className="pixel-label muted">
            {spaPlural(panels.length, { one: "{count} panel", other: "{count} panels" })}
          </span>
        }
      >
        <p className="muted panel-lead">
          {" "}
          {deferredText(
            "A message where members pick their own roles: with buttons, a menu or reactions. The bot posts it, and edits the same message when you change the panel.",
          )}{" "}
        </p>
        <p className="muted fine">{rolePanelsSummary(panels)}</p>
        {panels.length === 0 ? (
          <p className="empty-row muted">{spaTranslate("No panels yet.")}</p>
        ) : (
          <div className="settings-detail-list role-panel-settings" aria-label={deferredText("Role panels")}>
            {panels.map((panel) => (
              <div key={panel.id} data-panel={panel.id}>
                <SettingsRow
                  rowId={`role-panel-${panel.id}`}
                  key={`${panel.id}-${editing === panel.id}`}
                  title={panel.title}
                  description={t("Edit the roles, message and posting channel.")}
                  summary={rolePanelSummary(panel, info)}
                  initialOpen={editing === panel.id}
                >
                  <div className="settings-detail-fields">
                    <p className="panel-card-meta">
                      <span className="badge badge-ink">{spaTranslate(panel.style.toUpperCase())}</span>
                      <span className="muted">
                        {spaPlural(panel.roles.length, { one: "{count} role", other: "{count} roles" })}
                      </span>
                      {panel.channel_id ? (
                        <ChannelTag info={info} id={panel.channel_id} />
                      ) : (
                        <span className="badge badge-gold">{deferredText("DRAFT")}</span>
                      )}
                    </p>
                    <div className="panel-card-actions">
                      <PixelButton
                        variant="secondary"
                        className="button-small"
                        aria-label={deferredText("Edit {arg1}", { arg1: panel.title })}
                        onClick={() => open(panel)}
                      >
                        {spaTranslate("Edit")}
                      </PixelButton>
                      {confirmDelete === panel.id ? (
                        <>
                          <PixelButton
                            className="button-small"
                            disabled={write.isPending}
                            onClick={() =>
                              write.mutate(
                                { method: "DELETE", sub: `/panels/${panel.id}` },
                                {
                                  onSettled: () => setConfirmDelete(null),
                                  onSuccess: () => {
                                    if (editing === panel.id) setEditing(null);
                                  },
                                },
                              )
                            }
                          >
                            {" "}
                            {spaTranslate("Delete it")}{" "}
                          </PixelButton>
                          <PixelButton
                            variant="ghost"
                            className="button-small"
                            onClick={() => setConfirmDelete(null)}
                          >
                            {spaTranslate("Keep it")}
                          </PixelButton>
                        </>
                      ) : (
                        <PixelButton
                          variant="ghost"
                          className="button-small"
                          aria-label={deferredText("Delete {arg1}", { arg1: panel.title })}
                          onClick={() => setConfirmDelete(panel.id)}
                        >
                          {spaTranslate("Delete")}
                        </PixelButton>
                      )}
                    </div>
                  </div>
                  {editing === panel.id && (
                    <Editor
                      key={editing}
                      info={info}
                      draft={draft}
                      setDraft={setDraft}
                      saved={saved}
                      current={current}
                      unsaved={edits > 0}
                      problem={local}
                      server={serverProblem}
                      posting={write}
                    />
                  )}
                </SettingsRow>
              </div>
            ))}
          </div>
        )}
        <div className="panel-foot">
          <PixelButton onClick={() => open(null)} disabled={editing === "new"}>
            {" "}
            {spaTranslate("New panel")}{" "}
          </PixelButton>
        </div>
      </Panel>

      {editing === "new" && (
        <div className="role-panel-settings">
          <SettingsRow
            rowId="role-panels-new-panel"
            key="new-panel"
            title={t("row.New panel", {}, "NEW PANEL")}
            description={t("Create a role panel and preview its message.")}
            summary={t("Draft")}
            initialOpen
          >
            <Editor
              key={editing}
              info={info}
              draft={draft}
              setDraft={setDraft}
              saved={saved}
              current={current}
              unsaved={edits > 0}
              problem={local}
              server={serverProblem}
              posting={write}
            />
          </SettingsRow>
        </div>
      )}

      <SaveBar
        count={edits}
        saving={write.isPending && !!write.variables && !write.variables.sub.endsWith("/post")}
        blocked={!!local}
        error={
          serverProblem && serverProblem.field !== "channel_id"
            ? serverProblem.message
            : local
              ? deferredText("Fix first: {arg1}", { arg1: local.message })
              : null
        }
        onSave={save}
        onReset={() => {
          write.reset();
          setDraft({ ...saved, roles: saved.roles.map((r) => ({ ...r, key: newKey() })) });
        }}
      />
      <Toast message={toast} onDone={() => setToast(null)} />
    </>
  );
}
