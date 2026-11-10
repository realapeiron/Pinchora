import { useEffect, useRef, useState } from "react";
import { api, type Me } from "../../../lib/api";
import { Icon } from "../../../components/icons/Icon";
import { setLocale, t, useLocale, type Locale } from "../../../i18n";

const choices = [
  { code: "tr", name: "Türkçe", description: "Panel and bot replies in Turkish" },
  { code: "en", name: "English", description: "Panel and bot replies in English" },
] as const;

export function GuildLanguageMenu({
  me,
  disabled = false,
  drawer = false,
}: {
  me?: Me;
  disabled?: boolean;
  drawer?: boolean;
}) {
  const active = useLocale();
  const [error, setError] = useState(false);
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        trigger.current?.focus();
      }
    };
    const outside = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("keydown", close);
    document.addEventListener("pointerdown", outside);
    return () => {
      document.removeEventListener("keydown", close);
      document.removeEventListener("pointerdown", outside);
    };
  }, [open]);
  const switchTo = (choice: Locale) => {
    setError(false);
    setOpen(false);
    void setLocale(
      choice,
      me ? (language) => api("/api/me/language", "POST", { language }) : undefined,
    ).catch(() => setError(true));
  };
  const options = choices.map((choice) => (
    <button
      key={choice.code}
      type="button"
      role={drawer ? undefined : "menuitemradio"}
      className={active === choice.code ? "is-active" : ""}
      aria-checked={drawer ? undefined : active === choice.code}
      aria-pressed={drawer ? active === choice.code : undefined}
      disabled={disabled}
      lang={choice.code}
      onClick={() => switchTo(choice.code)}
    >
      {!drawer && <span className="language-code">{choice.code.toUpperCase()}</span>}
      <span className="language-text">
        <strong>{choice.name}</strong>
        {!drawer && <small>{t(choice.description)}</small>}
      </span>
      {!drawer && active === choice.code && <Icon name="check" />}
    </button>
  ));
  return (
    <div
      ref={wrap}
      className={`language-switcher ${drawer ? "language-drawer" : "language-menu"}`}
      role={drawer ? "group" : undefined}
      aria-label={t("Language")}
    >
      {drawer ? (
        options
      ) : (
        <>
          <button
            ref={trigger}
            type="button"
            className="language-trigger"
            aria-expanded={open}
            aria-haspopup="menu"
            disabled={disabled}
            onClick={() => setOpen(!open)}
          >
            <Icon name="globe" />
            {choices.find((choice) => choice.code === active)?.name}
            <Icon name="chevron-down" />
          </button>
          {open && (
            <div className="language-pop shell-pop" role="menu" aria-label={t("Language")}>
              <div className="language-pop-head">
                <strong>{t("Panel language")}</strong>
                <small>{t("Only for you. Change the server bot language on Branding.")}</small>
              </div>
              {options}
            </div>
          )}
        </>
      )}
      {error && <span role="alert">{t("Could not save your language choice. Try again.")}</span>}
    </div>
  );
}
