import { useRef, useState } from "react";
import { t } from "../../../i18n/dashboard";
import { ApiError, type ModuleEntry } from "../../../lib/api";
import { ModuleSaveError, useSaveModules } from "../../../lib/queries";

export function useModuleToggle(guildId: string) {
  const save = useSaveModules(guildId);
  const [notice, setNotice] = useState<string | null>(null);
  const [undo, setUndo] = useState<(() => void) | undefined>();
  const [confirm, setConfirm] = useState<{ module: ModuleEntry; restore?: () => void } | null>(null);
  const mutationId = useRef(0);
  const toggle = (module: ModuleEntry) => {
    if (module.key === "command_control" && module.enabled) {
      setConfirm({ module });
      return;
    }
    change(module);
  };
  const restore = (module: ModuleEntry, next: boolean) => {
    setUndo(undefined);
    save.mutate([{ key: module.key, patch: { enabled: module.enabled, expect_enabled: next } }], {
      onSuccess: () => setNotice(t("Previous value restored.")),
      onError: (error) => {
        const reason =
          error instanceof ModuleSaveError ? error.reason : error instanceof ApiError ? error : null;
        setNotice(
          t(
            reason instanceof ApiError && reason.status === 409
              ? "The module changed since this save and was left alone."
              : "Could not change the module. Try again.",
          ),
        );
      },
    });
  };
  const change = (module: ModuleEntry) => {
    const next = !module.enabled;
    const current = ++mutationId.current;
    setUndo(undefined);
    save.mutate([{ key: module.key, patch: { enabled: next, expect_enabled: module.enabled } }], {
      onSuccess: () => {
        if (current !== mutationId.current) return;
        setNotice(t(next ? "Module enabled" : "Module disabled"));
        setUndo(() => () => {
          if (module.key === "command_control" && !module.enabled) {
            setConfirm({ module: { ...module, enabled: true }, restore: () => restore(module, next) });
          } else {
            restore(module, next);
          }
        });
      },
      onError: (error) => {
        if (current !== mutationId.current) return;
        setUndo(undefined);
        const reason =
          error instanceof ModuleSaveError ? error.reason : error instanceof ApiError ? error : null;
        setNotice(
          t(
            reason instanceof ApiError && reason.status === 409
              ? "The module changed since this save and was left alone."
              : "Could not change the module. Try again.",
          ),
        );
      },
    });
  };
  return {
    toggle,
    confirm: confirm?.module ?? null,
    approve: () => {
      if (confirm) {
        if (confirm.restore) confirm.restore();
        else change(confirm.module);
      }
      setConfirm(null);
    },
    cancel: () => setConfirm(null),
    pending: save.isPending,
    notice,
    undo,
    clear: () => {
      setNotice(null);
      setUndo(undefined);
    },
  };
}
