import { useRef, useState } from "react";
import { t } from "../../../i18n/dashboard";
import type { ModuleEntry } from "../../../lib/api";
import { useSaveModules } from "../../../lib/queries";

export function useModuleToggle(guildId: string) {
  const save = useSaveModules(guildId);
  const [notice, setNotice] = useState<string | null>(null);
  const [undo, setUndo] = useState<(() => void) | undefined>();
  const mutationId = useRef(0);
  const toggle = (module: ModuleEntry) => {
    const next = !module.enabled;
    const current = ++mutationId.current;
    setUndo(undefined);
    save.mutate([{ key: module.key, patch: { enabled: next } }], {
      onSuccess: () => {
        if (current !== mutationId.current) return;
        setNotice(t(next ? "Module enabled" : "Module disabled"));
        setUndo(() => () => save.mutate([{ key: module.key, patch: { enabled: module.enabled } }]));
      },
      onError: () => {
        if (current !== mutationId.current) return;
        setUndo(undefined);
        setNotice(t("Could not change the module. Try again."));
      },
    });
  };
  return {
    toggle,
    pending: save.isPending,
    notice,
    undo,
    clear: () => {
      setNotice(null);
      setUndo(undefined);
    },
  };
}
