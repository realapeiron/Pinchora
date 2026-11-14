import { t as deferredText } from "../../../../i18n/dashboard";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";

export function StudioSave({
  dirty,
  busy,
  save,
  error,
  notice,
}: Pick<StudioModel, "dirty" | "busy" | "save" | "error" | "notice">) {
  return (
    <div className="studio-save">
      <PixelButton disabled={!dirty || busy} onClick={() => void save()}>
        {busy ? deferredText("Working…") : deferredText("Save kit")}
      </PixelButton>
      {error && (
        <p role="alert" className="field-error">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
    </div>
  );
}
