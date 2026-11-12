import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { GuildInfo } from "../lib/api";
import { ChannelPicker } from "./DashboardPickers";

const info = {
  channels: [
    {
      id: "1",
      name: "announcements",
      type: 5,
      parent_id: null,
      position: 0,
      missing_permissions: ["send_messages"],
    },
    { id: "2", name: "welcome", type: 0, parent_id: null, position: 1, missing_permissions: [] },
  ],
} as GuildInfo;

describe("posting channel picker", () => {
  it("announces the missing permission with the selected posting channel", () => {
    const html = renderToStaticMarkup(
      <ChannelPicker id="channel" info={info} value="1" posting onChange={() => {}} />,
    );
    expect(html).toContain("aria-describedby=");
    expect(html).toContain("Pinchora can&#x27;t post in this channel: Send Messages");
    expect(html).toContain("channel settings");
  });
  it("keeps a writable channel and a nonposting picker clear", () => {
    const writable = renderToStaticMarkup(
      <ChannelPicker info={info} value="2" posting onChange={() => {}} />,
    );
    const reading = renderToStaticMarkup(<ChannelPicker info={info} value="1" onChange={() => {}} />);
    expect(writable).not.toContain("channel-post-warning");
    expect(reading).not.toContain("channel-post-warning");
  });
  it("warns only for permissions required by the selected delivery", () => {
    const imageInfo = {
      ...info,
      channels: [{ ...info.channels[0], missing_permissions: ["embed_links", "attach_files"] }],
    } as GuildInfo;
    const text = renderToStaticMarkup(
      <ChannelPicker info={imageInfo} value="1" posting delivery="text" onChange={() => {}} />,
    );
    const image = renderToStaticMarkup(
      <ChannelPicker info={imageInfo} value="1" posting delivery="image" onChange={() => {}} />,
    );
    expect(text).not.toContain("channel-post-warning");
    expect(image).toContain("Attach Files");
    expect(image).not.toContain("Embed Links");
  });
});
