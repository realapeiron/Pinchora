import { useEffect } from "react";
import { translate } from "../i18n";

/** A short note in the corner that goes away by itself. */
export function Toast({
  message,
  onDone,
  action,
}: {
  message: string | null;
  onDone: () => void;
  action?: { label: string; onClick: () => void };
}) {
  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(onDone, action ? 5000 : 2800);
    return () => window.clearTimeout(timer);
  }, [message, onDone, action]);
  if (!message) return null;
  return (
    <div className="toast" role="status">
      {translate(message)}
      {action && (
        <button
          type="button"
          className="toast-action"
          onClick={() => {
            action.onClick();
            onDone();
          }}
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
