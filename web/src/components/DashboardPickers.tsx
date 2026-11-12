import { useId, type ComponentProps } from "react";
import { ChannelPicker as ChannelSelect } from "./Pickers";
import { PostingPermissionWarning } from "./PostingPermissionWarning";

export { ChannelTag, RolePicker, RoleTag, channelLabel, roleColor, why } from "./Pickers";

export function ChannelPicker({
  posting = false,
  delivery,
  describedBy,
  ...props
}: ComponentProps<typeof ChannelSelect> & { posting?: boolean; delivery?: "text" | "image" }) {
  const warningId = useId();
  const missing =
    posting && !props.any
      ? (
          props.info?.channels.find((channel) => channel.id === props.value)?.missing_permissions ?? []
        ).filter(
          (key) =>
            !delivery ||
            (delivery === "image"
              ? ["view_channel", "send_messages", "attach_files"].includes(key)
              : ["view_channel", "send_messages"].includes(key)),
        )
      : [];
  return (
    <>
      <ChannelSelect
        {...props}
        describedBy={[describedBy, missing.length ? warningId : null].filter(Boolean).join(" ") || undefined}
      />
      {missing.length > 0 && <PostingPermissionWarning id={warningId} missing={missing} />}
    </>
  );
}
