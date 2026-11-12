import { t } from "../i18n/dashboard";

const permissionNames: Record<string, string> = {
  view_channel: "View Channel",
  send_messages: "Send Messages",
  embed_links: "Embed Links",
  attach_files: "Attach Files",
};

export function PostingPermissionWarning({ id, missing }: { id: string; missing: string[] }) {
  return (
    <p id={id} className="field-hint channel-post-warning" role="status">
      {t(
        "Pinchora can't post in this channel: {permissions}. Give Pinchora's role the permissions in the channel settings.",
        { permissions: missing.map((key) => t(permissionNames[key] ?? key)).join(", ") },
      )}
    </p>
  );
}
