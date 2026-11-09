import { useState } from "react";
import { t } from "../../../../i18n/workflows";
import type { Playback } from "../../api/workflows";
export function Player({ playback }: { playback: Playback }) {
  const [index, setIndex] = useState(0);
  const step = playback.steps[index];
  return (
    <section className="wf-playback" aria-label={t("Playback")}>
      <p role="status">{t(playback.outcome)}</p>
      <ol>
        {playback.steps.map((s, i) => (
          <li key={i}>
            <button type="button" aria-current={i === index ? "step" : undefined} onClick={() => setIndex(i)}>
              {t(s.phase)} {s.index + 1} · {t(s.status)}
            </button>
          </li>
        ))}
      </ol>
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- Long playback details are keyboard scrollable. */}
      {step && <pre tabIndex={0}>{JSON.stringify(step.detail, null, 2)}</pre>}
    </section>
  );
}
