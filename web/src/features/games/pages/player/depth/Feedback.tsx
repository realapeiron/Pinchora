import { t } from "../../../../../i18n";
import { useAction } from "./useAction";

export function Feedback({ action }: { action: ReturnType<typeof useAction> }) {
  if (action.isError)
    return (
      <p role="alert" className="notice notice-heart">
        {action.error.message}
      </p>
    );
  if (action.isSuccess)
    return (
      <p role="status" className="notice">
        {t("Done")}
      </p>
    );
  return null;
}
