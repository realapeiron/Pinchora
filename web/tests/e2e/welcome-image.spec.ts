// The welcome image editor in a real browser: drag the avatar, edit the text, save, and the picture the bot
// draws for the saved design is the one the editor showed; then templates, stickers, the layer list, an upload,
// a test message, the goodbye image, and the tablet and phone layouts. Screenshots go to $CARD_SCREENS (or
// test-results/screens).

import { mkdirSync } from "node:fs";
import { deflateSync } from "node:zlib";

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow } from "./helpers";

const run = Date.now();
const guild = `${run}2`;
const user = `${run}3`;
const SCREENS = process.env.CARD_SCREENS ?? "test-results/screens";
mkdirSync(SCREENS, { recursive: true });

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

async function shot(page: Page, name: string, fullPage = false) {
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: `${SCREENS}/${name}.png`, animations: "disabled", fullPage });
}

/** The pixels of the bot's picture the editor shows right now, once it has finished drawing. The picture sits
 * behind a toggle in the Try it panel; this opens it first. */
async function botPicture(page: Page): Promise<string> {
  const toggle = page.getByRole("button", { name: /the bot's picture$/ });
  if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
  const frame = page.locator(".card-bot-preview .preview-frame");
  await expect(frame).not.toHaveClass(/is-busy/, { timeout: 20_000 });
  return page.locator(".card-bot-preview .preview-image").evaluate(async (img: HTMLImageElement) => {
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    canvas.getContext("2d")!.drawImage(img, 0, 0);
    return canvas.toDataURL("image/png");
  });
}

/** The saved design, as `GET /cards` has it. */
async function saved(page: Page, kind: "welcome" | "goodbye") {
  const response = await page.request.get(`/api/guilds/${guild}/cards`);
  expect(response.ok()).toBeTruthy();
  return (await response.json())[kind];
}

/** A plain PNG of `w`×`h`, for the upload. */
function png(w: number, h: number): Buffer {
  const crc = (bytes: Buffer) => {
    let c = ~0;
    for (const b of bytes) {
      c ^= b;
      for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
    }
    return ~c >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const out = Buffer.alloc(12 + data.length);
    out.writeUInt32BE(data.length, 0);
    body.copy(out, 4);
    out.writeUInt32BE(crc(body), 8 + data.length);
    return out;
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(w, 0);
  header.writeUInt32BE(h, 4);
  header.set([8, 2, 0, 0, 0], 8);
  const rows = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      rows.set([(40 + (x * 200) / w) | 0, 60, (120 + (y * 100) / h) | 0], y * (w * 3 + 1) + 1 + x * 3);
    }
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(rows)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

test.describe.configure({ mode: "serial" });

test("drag the avatar, edit the text, save: the bot draws what the editor showed", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/welcome-image`);
  await expect(page.getByRole("heading", { name: "WELCOME IMAGE", exact: true })).toBeVisible();
  // A server that never opened the editor starts from the classic card.
  const avatar = page.locator('[data-layer="avatar"]');
  await expect(avatar).toBeVisible();
  const classic = await botPicture(page);
  await shot(page, "editor-classic", true);

  // Drag the avatar to the left with the mouse.
  const box = (await avatar.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 - 160, box.y + box.height / 2 + 10, { steps: 12 });
  await shot(page, "editor-dragging");
  await page.mouse.up();
  await expect(page.locator(".card-selection")).toBeVisible();
  const moved = (await avatar.boundingBox())!;
  expect(moved.x).toBeLessThan(box.x - 100);

  // Edit the headline, undo it, redo it.
  await page.locator(".layer-name", { hasText: "WELCOME" }).click();
  const text = page.getByLabel("Text", { exact: true });
  await expect(text).toHaveValue("WELCOME");
  await text.fill("AHOY {user}");
  await page.getByRole("button", { name: "Undo" }).click();
  await expect(text).toHaveValue("WELCOME");
  await page.getByRole("button", { name: "Redo" }).click();
  await expect(text).toHaveValue("AHOY {user}");
  // A placeholder the bot does not know blocks saving with the bot's own sentence.
  await text.fill("AHOY {nickname}");
  await expect(page.getByText("{nickname} isn't a placeholder").first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Save changes" })).toBeDisabled();
  await text.fill("AHOY {user}");
  await expect(page.getByText("1 unsaved change")).toBeVisible();
  const edited = await botPicture(page);
  expect(edited).not.toEqual(classic);
  await shot(page, "editor-edited", true);

  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const welcome = await saved(page, "welcome");
  expect(welcome.custom).toBe(true);
  const savedAvatar = welcome.design.layers.find((l: { id: string }) => l.id === "avatar");
  expect(savedAvatar.x).toBeLessThan(450 - 100);
  expect(welcome.design.layers.find((l: { id: string }) => l.id === "title").text).toBe("AHOY {user}");

  // The bot's picture of the saved design, asked for afresh, is pixel for pixel what the editor shows.
  const shown = await botPicture(page);
  const fresh = await page.evaluate(
    async ({ guild, design }) => {
      const me = await (await fetch("/api/me")).json();
      const response = await fetch(`/api/guilds/${guild}/preview/card`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-CSRF-Token": me.csrf, Accept: "image/png" },
        body: JSON.stringify({ design }),
      });
      const bitmap = await createImageBitmap(await response.blob());
      const canvas = document.createElement("canvas");
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      canvas.getContext("2d")!.drawImage(bitmap, 0, 0);
      return canvas.toDataURL("image/png");
    },
    { guild, design: welcome.design },
  );
  expect(fresh).toEqual(shown);

  // The audit log has it.
  await page.goto(`/g/${guild}/audit`);
  await expect(page.getByText("saved an image design").first()).toBeVisible();
});

test("templates, stickers, the layer list, uploads, a test message and goodbyes", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/welcome-image`);
  await expect(page.locator('[data-layer="avatar"]')).toBeVisible();

  // The gallery: the starlit banner is wider than the classic card.
  await page.getByRole("button", { name: "Templates" }).click();
  await expect(page.getByRole("dialog", { name: "TEMPLATES" })).toBeVisible();
  await expect(page.locator(".template")).toHaveCount(8);
  // Every tile is as tall as the others, whatever the shape of the design in it.
  const tiles = await page
    .locator(".template")
    .evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().height)));
  expect(new Set(tiles).size, `tile heights ${tiles}`).toBe(1);
  await shot(page, "editor-templates");
  await page.locator(".template", { hasText: "Starlit banner" }).click();
  await expect(page.locator(".card-size")).toHaveText("1200×400");

  // A sticker from the pets, found by searching, then duplicated, hidden, locked and deleted.
  await page.getByRole("button", { name: "+ Sticker" }).click();
  await page.getByLabel("Search stickers").fill("cat");
  await shot(page, "editor-stickers");
  await page.getByRole("button", { name: "Drowsepatch cat" }).click();
  const list = page.getByRole("list", { name: "Layers, top first" });
  await expect(list.locator(".layer-row").first()).toContainText("Drowsepatch cat");
  await page.getByRole("button", { name: "Duplicate" }).click();
  await expect(list.locator(".layer-row", { hasText: "Drowsepatch cat" })).toHaveCount(2);
  await page.getByRole("button", { name: "Hide Drowsepatch cat" }).first().click();
  await expect(list.locator(".layer-row.is-hidden")).toHaveCount(1);
  await page.getByRole("button", { name: "Lock Drowsepatch cat" }).last().click();
  await page.getByRole("button", { name: "Move Drowsepatch cat down" }).last().click();
  await page.locator(".layer-name", { hasText: "Drowsepatch cat" }).first().click();
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(list.locator(".layer-row", { hasText: "Drowsepatch cat" })).toHaveCount(1);

  // An uploaded background, re-encoded by the server and shown on the canvas.
  await page.locator(".card-stage").click({ position: { x: 4, y: 4 } });
  await page
    .getByTestId("card-upload")
    .setInputFiles({ name: "beach.png", mimeType: "image/png", buffer: png(640, 360) });
  await expect(page.getByRole("listbox", { name: "Uploaded pictures" })).toBeVisible();
  await expect(page.locator(".card-svg image").first()).toHaveAttribute("href", /cards\/uploads\//);

  // Sent as the image first, then the text; a test goes to #welcome.
  await page.getByLabel("How the image is sent").selectOption("before_message");
  await page.getByLabel("Post a test in").selectOption({ label: "#welcome" });
  await page.getByRole("button", { name: "Send test" }).click();
  // The bot draws the image first; on a busy machine that takes a while.
  await expect(page.getByRole("status").filter({ hasText: "Test posted in #welcome" })).toBeVisible({
    timeout: 30_000,
  });
  await botPicture(page);
  await shot(page, "editor-banner", true);

  // The goodbye image: no direct messages for someone who left.
  await page.getByRole("tab", { name: "Goodbye" }).click();
  const send = page.getByLabel("How the image is sent");
  await expect(send).toHaveValue("text_only");
  await expect(send.locator("option")).toHaveCount(4);
  await send.selectOption("with_message");
  await page.getByRole("button", { name: "Templates" }).click();
  await page.locator(".template", { hasText: "Clean card" }).click();
  await expect(page.locator(".card-stage")).toHaveClass(/is-transparent/);
  await botPicture(page);
  await shot(page, "editor-goodbye", true);
  await expect(page.getByText("2 unsaved changes")).toBeVisible();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const [welcome, goodbye] = [await saved(page, "welcome"), await saved(page, "goodbye")];
  expect([welcome.delivery, welcome.design.width, welcome.design.background.type]).toEqual([
    "before_message",
    1200,
    "upload",
  ]);
  expect([goodbye.delivery, goodbye.custom, goodbye.design.background.type]).toEqual([
    "with_message",
    true,
    "transparent",
  ]);
});

test("the editor on a tablet and a phone", async ({ page }) => {
  await login(page);
  await page.setViewportSize({ width: 900, height: 1180 });
  await page.goto(`/g/${guild}/welcome-image`);
  await expect(page.locator('[data-layer="avatar"]')).toBeVisible();
  await botPicture(page);
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "editor-tablet", true);

  // One column, the canvas first.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await expect(page.locator('[data-layer="avatar"]')).toBeVisible();
  await page.locator('[data-layer="avatar"]').click();
  // The phone keeps to the settings a thumb needs: no coordinates.
  await expect(page.getByRole("radiogroup", { name: "Shape" })).toBeVisible();
  await expect(page.getByLabel("X", { exact: true })).toHaveCount(0);
  await botPicture(page);
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "editor-phone", true);
});

test("laptop and desktop widths keep the canvas and the panels in view together", async ({ page }) => {
  await login(page);
  const welcome = await saved(page, "welcome");
  expect(welcome.design.layers.some((l: { type: string }) => l.type === "avatar")).toBe(true);
  for (const [width, height] of [
    [1280, 800],
    [1440, 900],
    [1920, 1080],
  ] as const) {
    await page.setViewportSize({ width, height });
    await page.goto(`/g/${guild}/welcome-image`);
    await expect(page.locator('[data-layer="avatar"]')).toBeVisible();
    // The bot's picture is not a second canvas on the page until it is asked for.
    await expect(page.locator(".card-svg")).toHaveCount(1);
    await expect(page.locator(".card-bot-preview")).toHaveCount(0);

    // Nothing selected: the background's segments fill their rows, no dark gap trailing the last one.
    await page.locator(".card-stage").click({ position: { x: 4, y: 4 } });
    const gaps = await page
      .locator(".prop .segmented")
      .first()
      .evaluate((box) => {
        const outer = box.getBoundingClientRect();
        const rows = new Map<number, number>();
        for (const child of box.children) {
          const r = child.getBoundingClientRect();
          rows.set(Math.round(r.top), Math.max(rows.get(Math.round(r.top)) ?? 0, r.right));
        }
        return [...rows.values()].map((right) => Math.round(outer.right - right));
      });
    expect(Math.max(...gaps), `row gaps ${gaps}`).toBeLessThanOrEqual(3);

    // A layer selected: the canvas, the layer list and its properties are all on screen.
    await page.locator('[data-layer="avatar"]').click();
    await page.evaluate(() => {
      const top = document.querySelector(".card-workspace")!.getBoundingClientRect().top + window.scrollY;
      window.scrollTo(0, Math.max(0, top - 84));
    });
    for (const target of [
      page.locator(".card-stage"),
      page.getByRole("heading", { name: "LAYERS", exact: true }),
      page.getByRole("heading", { name: "PROPERTIES", exact: true }),
    ]) {
      await expect(target).toBeInViewport({ ratio: 1 });
      // Not hidden under the sticky top bar.
      expect((await target.boundingBox())!.y).toBeGreaterThanOrEqual(72);
    }
    await shot(page, `fix-${width}`);
  }

  // The toast rides above the save bar, not on top of its buttons.
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto(`/g/${guild}/welcome-image`);
  await page.locator('[data-layer="avatar"]').click();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByText("1 unsaved change")).toBeVisible();
  await page.getByLabel("Post a test in").selectOption({ label: "#welcome" });
  await page.getByRole("button", { name: "Send test" }).click();
  const toast = page.getByRole("status").filter({ hasText: "Test posted in #welcome" });
  await expect(toast).toBeVisible({ timeout: 30_000 });
  const [t, bar] = [await toast.boundingBox(), await page.locator(".save-bar-inner").boundingBox()];
  expect(t!.y + t!.height, "the toast ends above the save bar").toBeLessThanOrEqual(bar!.y);
  await shot(page, "fix-toast");

  // The tablet: the canvas on top, the panels side by side under it.
  await page.setViewportSize({ width: 900, height: 1180 });
  await page.goto(`/g/${guild}/welcome-image`);
  await expect(page.locator('[data-layer="avatar"]')).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "fix-tablet", true);
});

test("a design stays within what a card may cost to draw", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/welcome-image`);
  await expect(page.locator('[data-layer="avatar"]')).toBeVisible();
  const meter = page.getByRole("meter");
  await expect(meter).toBeVisible();

  // A layer is never bigger than the canvas: a typed width is held to it.
  const canvasWidth = Number((await page.locator(".card-size").innerText()).split("×")[0]);
  await page.locator(".layer-name", { hasText: "Rounded box" }).click();
  const width = page.getByLabel("Width", { exact: true });
  await width.fill("99999");
  await expect(width).toHaveValue(String(canvasWidth));
  await page.locator(".card-stage").click({ position: { x: 4, y: 4 } });

  // Four avatars at most.
  for (let i = 0; i < 4; i++) await page.getByRole("button", { name: "+ Avatar" }).click();
  await expect(page.getByRole("status").filter({ hasText: "at most 4 avatars" })).toBeVisible();
  await expect(page.locator(".layer-row", { hasText: "AVATAR" })).toHaveCount(4);

  // Text weighs by its characters: layer after layer, the meter climbs until the next one is refused.
  const before = Number((await meter.getAttribute("aria-valuenow")) ?? "0");
  for (let i = 0; i < 10; i++) await page.getByRole("button", { name: "+ Text" }).click();
  await expect(page.getByRole("status").filter({ hasText: "too heavy to draw quickly" })).toBeVisible();
  expect(Number(await meter.getAttribute("aria-valuenow"))).toBeGreaterThan(before);
  expect(Number(await meter.getAttribute("aria-valuenow"))).toBeLessThanOrEqual(100);
  await shot(page, "fix-cost");
});
