import { translate as t } from "../../../../i18n";
import type { HouseEditorModel } from "./useHouseEditor";
import { PixelButton } from "../../../../components/PixelButton";

export function HouseConfirmation({
  model,
}: {
  model: Pick<HouseEditorModel, "confirm" | "pending" | "send" | "setConfirm">;
}) {
  const { confirm, pending, send, setConfirm } = model;
  return (
    <>
      {confirm && (
        <div className="house-confirm" role="alertdialog" aria-modal="false" aria-label={confirm.label}>
          <p>{confirm.label}</p>
          <PixelButton
            /* eslint-disable-next-line jsx-a11y/no-autofocus -- The confirmation action receives focus when its dialog opens. */
            autoFocus
            disabled={pending}
            onClick={() => {
              send(confirm.op);
              setConfirm(null);
            }}
          >
            {t("Confirm")}
          </PixelButton>
          <PixelButton variant="secondary" onClick={() => setConfirm(null)}>
            {t("Cancel")}
          </PixelButton>
        </div>
      )}
    </>
  );
}
