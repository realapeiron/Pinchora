import { type ReactNode, useState } from "react";
import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Field, SwitchRow } from "../../../../components/Field";
import { Panel } from "../../../../components/Panel";
import { ChannelPicker, ChannelTag, RolePicker, RoleTag } from "../../../../components/DashboardPickers";
import { PixelButton } from "../../../../components/PixelButton";
import { PixelToggle } from "../../../../components/PixelToggle";
import type { GuildInfo } from "../../../../lib/api";
import { isDiscordId } from "../../../../lib/ids";
import type { LogDraft } from "../../api/moderation";

type Props = {
  draft: Pick<LogDraft, "ignore_bots" | "ignored_channels" | "ignored_roles" | "ignored_users">;
  set: (next: Partial<LogDraft>) => void;
  maxIgnored: number;
  info: GuildInfo | undefined;
  fieldError: (field: string) => string | null;
};

export function IgnoreLists({ draft, set, maxIgnored, info, fieldError }: Props) {
  return (
    <Panel title={deferredText("LEFT ALONE")}>
      <p className="panel-lead muted">
        {" "}
        {deferredText(
          "Nothing is logged about these members, members with these roles, or what happens in these channels (a category covers its channels, a channel its threads). Up to",
        )}{" "}
        {maxIgnored} {deferredText("of each.")}{" "}
      </p>
      <SwitchRow
        title={spaTranslate("Bots")}
        control={
          <PixelToggle
            label={deferredText("Leave bots alone")}
            checked={draft.ignore_bots}
            onChange={(bots) => set({ ignore_bots: bots })}
          />
        }
      >
        {" "}
        {deferredText("Messages, joins and changes of bots stay out of the log.")}{" "}
      </SwitchRow>
      <IgnoreSection
        label={deferredText("Channels and categories")}
        htmlFor="ignore-channel"
        list={draft.ignored_channels}
        onChange={(list) => set({ ignored_channels: list })}
        error={fieldError("ignored_channels")}
        input={(value, onChange) => (
          <ChannelPicker
            id="ignore-channel"
            info={info}
            value={value}
            onChange={onChange}
            none={spaTranslate("Pick a channel")}
            any
          />
        )}
        disabled={(value) => !value || draft.ignored_channels.includes(value)}
        onAdd={(value) => value && set({ ignored_channels: [...draft.ignored_channels, value] })}
        empty={deferredText("No channels.")}
        chipLabel={(id) => <ChannelTag info={info} id={id} />}
        removeLabel={deferredText("Log this channel again")}
      />
      <IgnoreSection
        label={spaTranslate("Roles")}
        htmlFor="ignore-role"
        list={draft.ignored_roles}
        onChange={(list) => set({ ignored_roles: list })}
        error={fieldError("ignored_roles")}
        input={(value, onChange) => (
          <RolePicker
            id="ignore-role"
            info={info}
            value={value}
            onChange={onChange}
            none={spaTranslate("Pick a role")}
          />
        )}
        disabled={(value) => !value || draft.ignored_roles.includes(value)}
        onAdd={(value) => value && set({ ignored_roles: [...draft.ignored_roles, value] })}
        empty={deferredText("No roles.")}
        chipLabel={(id) => <RoleTag info={info} id={id} />}
        removeLabel={deferredText("Log this role again")}
      />
      <IgnoreSection
        label={spaTranslate("Members")}
        htmlFor="ignore-user"
        list={draft.ignored_users}
        onChange={(list) => set({ ignored_users: list })}
        error={fieldError("ignored_users")}
        hint={deferredText(
          "Paste a member's user id (with Developer Mode on in Discord: right-click them, Copy User ID).",
        )}
        input={(value, onChange) => {
          const id = (value ?? "").trim();
          return (
            <input
              id="ignore-user"
              className="pixel-input"
              inputMode="numeric"
              placeholder={deferredText("User id")}
              value={value ?? ""}
              onChange={(event) => onChange(event.target.value)}
              aria-invalid={(id !== "" && !isDiscordId(id)) || undefined}
            />
          );
        }}
        disabled={(value) => {
          const id = (value ?? "").trim();
          return !isDiscordId(id) || draft.ignored_users.includes(id);
        }}
        onAdd={(value) => set({ ignored_users: [...draft.ignored_users, (value ?? "").trim()] })}
        empty={deferredText("No members.")}
        chipLabel={(user) => <span className="channel-tag">{user}</span>}
        removeLabel={deferredText("Log this member again")}
      />
    </Panel>
  );
}

function Chips({ children, empty }: { children: ReactNode[]; empty: string }) {
  if (children.length === 0) return <p className="muted ignore-empty">{empty}</p>;
  return <ul className="chips ignore-chips">{children}</ul>;
}

function RemoveChip({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button type="button" className="chip-remove" aria-label={label} onClick={onClick}>
      ×
    </button>
  );
}

type SectionProps = {
  label: string;
  htmlFor: string;
  list: string[];
  onChange: (list: string[]) => void;
  error: string | null;
  hint?: string;
  input: (value: string | null, onChange: (value: string | null) => void) => ReactNode;
  disabled: (value: string | null) => boolean;
  onAdd: (value: string | null) => void;
  empty: string;
  chipLabel: (id: string) => ReactNode;
  removeLabel: string;
};

function IgnoreSection({
  label,
  htmlFor,
  list,
  onChange,
  error,
  hint,
  input,
  disabled,
  onAdd,
  empty,
  chipLabel,
  removeLabel,
}: SectionProps) {
  const [value, setValue] = useState<string | null>(null);
  return (
    <Field label={label} htmlFor={htmlFor} error={error} hint={hint}>
      <div className="ignore-add">
        {input(value, setValue)}
        <PixelButton
          variant="secondary"
          disabled={disabled(value)}
          onClick={() => {
            onAdd(value);
            setValue(null);
          }}
        >
          {" "}
          {spaTranslate("Add")}{" "}
        </PixelButton>
      </div>
      <Chips empty={empty}>
        {list.map((id) => (
          <li key={id} className="channel-chip">
            {chipLabel(id)}
            <RemoveChip label={removeLabel} onClick={() => onChange(list.filter((x) => x !== id))} />
          </li>
        ))}
      </Chips>
    </Field>
  );
}
