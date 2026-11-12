import { useEffect, useId, useRef } from "react";
import { PixelButton } from "../../../components/PixelButton";
import { t } from "../../../i18n/dashboard";
import "../../../styles/module-confirm.css";

export function ModuleOffConfirm({
  open,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const title = useId();
  useEffect(() => {
    if (open) dialog.current?.showModal();
    else dialog.current?.close();
  }, [open]);
  return (
    <dialog
      ref={dialog}
      className="panel panel-gold dialog module-off-confirm"
      onCancel={onCancel}
      aria-labelledby={title}
    >
      <div className="panel-body">
        <h2 id={title} className="pixel-title">
          {t("Turn off command permissions?")}
        </h2>
        <p>
          {t(
            "Pinchora stops applying command permissions and aliases until you switch this back on. Discord may still enforce permissions set in Server Settings. Nothing is deleted.",
          )}
        </p>
        <div className="dialog-actions">
          <PixelButton variant="secondary" onClick={onCancel}>
            {t("Keep on")}
          </PixelButton>
          <PixelButton onClick={onConfirm}>{t("Turn off")}</PixelButton>
        </div>
      </div>
    </dialog>
  );
}
