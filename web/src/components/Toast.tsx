import { createContext, useCallback, useContext, useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { translate } from "../i18n";

type Action = { label: string; onClick: () => void };
type Notice = { id: string; message: string; onDone: () => void; action?: Action };
type ToastQueue = (notice: Notice | { id: string; message: null }) => void;

const ToastContext = createContext<ToastQueue | null>(null);

export function ToastProvider({ children, className }: { children: ReactNode; className?: string }) {
  const [notices, setNotices] = useState<Notice[]>([]);
  const publish = useCallback<ToastQueue>((notice) => {
    setNotices((current) => {
      const index = current.findIndex((item) => item.id === notice.id);
      if (notice.message === null)
        return index < 0 ? current : current.filter((item) => item.id !== notice.id);
      if (index < 0) return [...current, notice as Notice];
      return current.map((item, at) => (at === index ? (notice as Notice) : item));
    });
  }, []);
  const active = notices[0];
  const dismiss = useCallback(() => {
    if (!active) return;
    publish({ id: active.id, message: null });
    active.onDone();
  }, [active, publish]);
  return (
    <ToastContext.Provider value={publish}>
      {className ? (
        <div className={className}>
          {children}
          <ToastDisplay
            noticeId={active?.id}
            waiting={notices.length > 1}
            message={active?.message ?? null}
            onDone={dismiss}
            action={active?.action}
          />
        </div>
      ) : (
        <>
          {children}
          <ToastDisplay
            noticeId={active?.id}
            waiting={notices.length > 1}
            message={active?.message ?? null}
            onDone={dismiss}
            action={active?.action}
          />
        </>
      )}
    </ToastContext.Provider>
  );
}

/** A short note in the corner. Inside a provider, notices share one visible queue. */
export function Toast({
  message,
  onDone,
  action,
}: {
  message: string | null;
  onDone: () => void;
  action?: Action;
}) {
  const publish = useContext(ToastContext);
  const id = useId();
  const latest = useRef({ onDone, action });
  latest.current = { onDone, action };
  useEffect(() => {
    if (!publish) return;
    if (message)
      publish({
        id,
        message,
        onDone: () => latest.current.onDone(),
        action: action
          ? {
              label: action.label,
              onClick: () => latest.current.action?.onClick(),
            }
          : undefined,
      });
    else publish({ id, message: null });
    return () => publish({ id, message: null });
  }, [publish, id, message, !!action]);
  return publish ? null : <ToastDisplay message={message} onDone={onDone} action={action} />;
}

function ToastDisplay({
  noticeId,
  waiting = false,
  message,
  onDone,
  action,
}: {
  noticeId?: string;
  waiting?: boolean;
  message: string | null;
  onDone: () => void;
  action?: Action;
}) {
  const done = useRef(onDone);
  done.current = onDone;
  const hasAction = !!action;
  // The timer belongs to the notice shown, not to callback identities that change on every render;
  // a notice waiting behind it cuts the wait short, so a confirmation never sits behind an Undo for long.
  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(() => done.current(), waiting ? 1500 : hasAction ? 5000 : 2800);
    return () => window.clearTimeout(timer);
  }, [noticeId ?? message, message, waiting, hasAction]);
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
