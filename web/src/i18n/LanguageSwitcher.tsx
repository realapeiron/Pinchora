import { useState } from "react";
import { api, type Me } from "../lib/api";
import { setLocale, t, useLocale, type Locale } from "./index";

export function LanguageSwitcher({ me, disabled = false }: { me?: Me; disabled?: boolean }) {
  const active = useLocale();
  const [error, setError] = useState(false);
  const switchTo = (choice: Locale) => {
    setError(false);
    void setLocale(
      choice,
      me ? (language) => api("/api/me/language", "POST", { language }) : undefined,
    ).catch(() => {
      void setLocale(active);
      setError(true);
    });
  };
  return (
    <div className="language-switcher" role="group" aria-label={t("Language")}>
      {(["en", "tr"] as const).map((choice) => (
        <button
          key={choice}
          type="button"
          className={active === choice ? "is-active" : ""}
          aria-pressed={active === choice}
          disabled={disabled}
          lang={choice}
          onClick={() => switchTo(choice)}
        >
          {choice.toUpperCase()}
        </button>
      ))}
      {error && <span role="alert">{t("Could not save your language choice. Try again.")}</span>}
    </div>
  );
}
