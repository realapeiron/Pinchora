import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { Field, SwitchRow } from "../../../components/Field";
import { PixelToggle } from "../../../components/PixelToggle";
import { CategoryPicker, SupportRoles } from "./TicketPanelFields";
import type { TabProps } from "./TicketPanelTabProps";

const WELCOME_PLACEHOLDERS = ["{user}", "{server}", "{number}", "{topic}"];

export function BehaviourTab({ draft, set, limits, info, errors, base }: TabProps) {
  return (
    <>
      <Field
        label={deferredText("Support roles")}
        htmlFor={`${base}-roles`}
        error={errors.support_roles}
        hint={deferredText(
          "They see these tickets, get pinged in new ones and may claim, lock and close them. Manage Server and Manage Channels always may.",
        )}
        aside={
          <span className="counter">
            {draft.support_roles.length}/{limits.support_roles}
          </span>
        }
      >
        <SupportRoles
          id={`${base}-roles`}
          info={info}
          ids={draft.support_roles}
          max={limits.support_roles}
          onChange={(ids) => set("support_roles", ids)}
        />
      </Field>
      <Field
        label={deferredText("Welcome text")}
        htmlFor={`${base}-welcome`}
        error={errors.welcome_text}
        hint={deferredText("The ticket's first message. Empty uses a friendly default.")}
        aside={
          <span className="counter">
            {[...draft.welcome_text].length}/{limits.welcome}
          </span>
        }
      >
        <textarea
          id={`${base}-welcome`}
          className="pixel-input pixel-textarea"
          placeholder={deferredText(
            "Hi {user}, thanks for reaching out. Tell us what's going on and a staff member will be with you soon.",
          )}
          value={draft.welcome_text}
          onChange={(e) => set("welcome_text", e.target.value)}
          aria-invalid={errors.welcome_text ? true : undefined}
        />
        <div className="chips" role="group" aria-label={spaTranslate("Insert a placeholder")}>
          {WELCOME_PLACEHOLDERS.map((token) => (
            <button
              key={token}
              type="button"
              className="chip"
              onClick={() => set("welcome_text", `${draft.welcome_text}${token}`)}
            >
              {token}
            </button>
          ))}
        </div>
      </Field>
      <Field
        label={spaTranslate("Category")}
        htmlFor={`${base}-category`}
        error={errors.open_category_id}
        hint={deferredText("Where this panel's ticket channels go, if not the server's ticket category.")}
      >
        <CategoryPicker
          id={`${base}-category`}
          info={info}
          value={draft.open_category_id}
          onChange={(id) => set("open_category_id", id)}
          none={deferredText("The server's ticket category")}
          invalid={!!errors.open_category_id}
        />
      </Field>
      <SwitchRow
        title={deferredText("Takes tickets")}
        control={
          <PixelToggle
            label={deferredText("Panel takes tickets")}
            checked={draft.enabled}
            onChange={(on) => set("enabled", on)}
          />
        }
      >
        {" "}
        {deferredText("Off, its buttons are greyed out once you update the post.")}{" "}
      </SwitchRow>
    </>
  );
}
