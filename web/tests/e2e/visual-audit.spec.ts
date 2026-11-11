import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { openRow, devLogin, testSql } from "./helpers";

const guild = `${Date.now()}1`;
const user = `${Date.now()}2`;
const source = readFileSync(new URL("../../src/features/admin/pages/guildNav.ts", import.meta.url), "utf8");
const paths = [...source.slice(source.indexOf("export const NAV")).matchAll(/to: "([^"]*)"/g)].map(
  (match) => match[1],
);
// Reads are limited per user per minute, so each width signs in as its own user.
const login = (page: Page, as = user) =>
  devLogin(page, as, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "8" }]);

function seedCase() {
  testSql(`INSERT INTO mod_cases (guild_id, number, kind, target_id, target_name, moderator_id, moderator_name, reason, active)
     VALUES (${guild}, 1, 'warn', 81001, 'Mia', 91001, 'Captain', 'spam links', true)
     ON CONFLICT (guild_id, number) DO NOTHING`);
}

async function defects(page: Page, phone: boolean) {
  return page.evaluate((phone) => {
    const out: string[] = [];
    const vw = document.documentElement.clientWidth;
    if (document.documentElement.scrollWidth > vw + 1)
      out.push(`horizontal scroll ${document.documentElement.scrollWidth} > ${vw}`);
    const visible = (el: HTMLElement) => {
      const s = getComputedStyle(el),
        r = el.getBoundingClientRect();
      return (
        s.display !== "none" && s.visibility !== "hidden" && +s.opacity > 0 && r.width > 0 && r.height > 0
      );
    };
    const name = (el: Element) =>
      `${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ""}${typeof el.className === "string" ? `.${el.className.trim().split(/\s+/).slice(0, 2).join(".")}` : ""}`;
    const columns = document.querySelectorAll<HTMLElement>("main.shell-content > .settings-page");
    for (const column of columns.length
      ? columns
      : document.querySelectorAll<HTMLElement>("main.shell-content")) {
      const siblings = [...column.children].filter((child): child is HTMLElement => {
        if (!(child instanceof HTMLElement) || !visible(child)) return false;
        return !["absolute", "fixed"].includes(getComputedStyle(child).position);
      });
      for (let i = 1; i < siblings.length; i++) {
        const previous = siblings[i - 1];
        const next = siblings[i];
        if (!previous || !next) continue;
        const gap = next.getBoundingClientRect().top - previous.getBoundingClientRect().bottom;
        if (gap > 32)
          out.push(`empty main-column block ${name(previous)} → ${name(next)} ${gap.toFixed(0)}px`);
      }
    }
    for (const el of document.querySelectorAll<HTMLElement>(
      ".shell-main *, .shell-tabs *, .shell-sidebar.is-open *, .cases-sheet *",
    )) {
      if (!visible(el) || el.closest(".icon-sprite")) continue;
      const r = el.getBoundingClientRect();
      let p = el.parentElement,
        scroller = false;
      while (p && p !== document.body) {
        const s = getComputedStyle(p);
        if (["auto", "scroll"].includes(s.overflowX) && p.scrollWidth > p.clientWidth) {
          scroller = true;
          break;
        }
        p = p.parentElement;
      }
      if (!scroller && (r.left < -1 || r.right > vw + 1))
        out.push(`outside viewport ${name(el)} ${r.left.toFixed(0)}..${r.right.toFixed(0)}`);
      const s = getComputedStyle(el);
      const standardHidden = el.classList.contains("visually-hidden") && r.width <= 1 && r.height <= 1;
      if (
        !standardHidden &&
        ["hidden", "clip"].includes(s.overflowX) &&
        s.textOverflow !== "ellipsis" &&
        el.childNodes.length === 1 &&
        el.firstChild?.nodeType === Node.TEXT_NODE &&
        el.textContent?.trim() &&
        el.scrollWidth > el.clientWidth + 1
      )
        out.push(`clipped text ${name(el)}`);
    }
    for (const row of document.querySelectorAll<HTMLElement>(".settings-detail-row.is-open")) {
      const title = row
        .querySelector<HTMLElement>(":scope > .settings-detail-head .settings-detail-copy strong")
        ?.textContent?.trim()
        .toLocaleLowerCase();
      if (!title) continue;
      for (const heading of row.querySelectorAll<HTMLElement>(
        ":scope > .settings-detail-body h1, :scope > .settings-detail-body h2, :scope > .settings-detail-body h3, :scope > .settings-detail-body h4, :scope > .settings-detail-body h5, :scope > .settings-detail-body h6",
      )) {
        if (visible(heading) && heading.textContent?.trim().toLocaleLowerCase() === title)
          out.push(`repeated row heading ${title}`);
      }
    }
    if (phone) {
      const main = document.querySelector<HTMLElement>("main.shell-content")!;
      const cs = getComputedStyle(main),
        r = main.getBoundingClientRect();
      if (r.left + parseFloat(cs.paddingLeft) < 16 || vw - r.right + parseFloat(cs.paddingRight) < 16)
        out.push(`main gutter ${cs.paddingLeft}, ${cs.paddingRight}`);
      const controls = [
        ...document.querySelectorAll<HTMLElement>(
          ".shell-content button, .shell-content a[href], .shell-content input, .shell-content select, .shell-content textarea, .shell-content [role='button'], .shell-content [role='switch'], .shell-content [role='tab'], .shell-tabs a, .shell-tabs button, .shell-top button, .cases-sheet button, .cases-sheet input, .cases-sheet textarea",
        ),
      ].filter(
        (el) =>
          visible(el) &&
          !el.hasAttribute("disabled") &&
          !el.closest("p, .rich-text") &&
          getComputedStyle(el).position !== "absolute",
      );
      for (const el of controls) {
        const r = el.getBoundingClientRect();
        const label = el.matches('input[type="checkbox"], input[type="radio"]') ? el.closest("label") : null;
        const target = label ?? el;
        if (target.getBoundingClientRect().height < 43.5)
          out.push(`tap target ${name(el)} ${r.height.toFixed(0)}px`);
      }
      const last = controls.filter((el) => el.closest("main.shell-content")).at(-1);
      const tab = document.querySelector<HTMLElement>(".shell-tabs")!;
      if (
        last &&
        last.getBoundingClientRect().bottom + scrollY >
          document.documentElement.scrollHeight - tab.getBoundingClientRect().height + 1
      )
        out.push(`last control covered ${name(last)}`);
      const save = document.querySelector<HTMLElement>(".save-bar.is-open");
      if (save && last && last.getBoundingClientRect().bottom > save.getBoundingClientRect().top)
        out.push(`save bar overlap ${name(last)}`);
    }
    return out;
  }, phone);
}

for (const [width, height] of [
  [1440, 900],
  [820, 1180],
  [390, 844],
] as const) {
  test(`all guild pages fit at ${width}px`, async ({ page }) => {
    test.setTimeout(10 * 60_000);
    await page.setViewportSize({ width, height });
    await login(page, `${user}${width}`);
    const failures: string[] = [],
      visited: string[] = [];
    for (const path of paths) {
      if (path === "cases") seedCase();
      await page.goto(path ? `/g/${guild}/${path}` : `/g/${guild}`);
      await expect(page.locator("main.shell-content")).toBeVisible();
      await page.waitForLoadState("networkidle");
      await page.evaluate(() => document.fonts.ready);
      const label = path || "overview";
      visited.push(label);
      await page.screenshot({
        path: `test-results/visual/${width}/${label}.png`,
        fullPage: true,
        animations: "disabled",
      });
      for (const defect of await defects(page, width === 390)) failures.push(`${label}: ${defect}`);
      const closedRows = page.locator('.settings-detail-expand[aria-expanded="false"]:visible');
      while (await closedRows.count()) {
        // The attribute filter stops matching once a row opens, so follow the row by the body it controls.
        const body = await closedRows.first().getAttribute("aria-controls");
        await openRow(page.locator(`.settings-detail-expand[aria-controls="${body}"]`));
      }
      for (const defect of await defects(page, width === 390)) failures.push(`${label} expanded: ${defect}`);
      await page.screenshot({
        path: `test-results/visual/${width}/${label}-expanded.png`,
        fullPage: true,
        animations: "disabled",
      });
      if (path === "cases") {
        await page.getByRole(width === 390 ? "button" : "row", { name: "Open case 1" }).click();
        await expect(page.getByRole("dialog", { name: "Case #1" })).toBeVisible();
        for (const defect of await defects(page, width === 390)) failures.push(`${label} sheet: ${defect}`);
        await page.screenshot({
          path: `test-results/visual/${width}/${label}-sheet.png`,
          fullPage: true,
          animations: "disabled",
        });
        await page.getByRole("button", { name: "Close case" }).click();
      }
    }
    console.log(`${width}px visited: ${visited.join(", ")}`);
    expect(failures, failures.join("\n")).toEqual([]);
  });
}

test("palette, drawer and language menu work by keyboard", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}`);
  const search = page.locator(".shell-search");
  await search.focus();
  await page.keyboard.press("Control+k");
  const combo = page.getByRole("combobox");
  await expect(combo).toBeFocused();
  const paletteClose = page.locator(".command-close");
  await page.keyboard.press("Tab");
  await expect(paletteClose).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(combo).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(paletteClose).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(combo).toBeFocused();
  await combo.fill("welcome");
  await page.keyboard.press("ArrowDown");
  await expect(combo).toHaveAttribute("aria-activedescendant", /command-\d+/);
  await page.keyboard.press("Escape");
  await expect(search).toBeFocused();
  await page.keyboard.press("/");
  await expect(combo).toBeFocused();
  await page.keyboard.press("Escape");
  const serverPicker = page.locator(".shell-server");
  await serverPicker.focus();
  await page.keyboard.press("Enter");
  const serverItems = page.locator(".shell-servers a");
  await expect(serverItems.first()).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(serverItems.last()).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(serverItems.first()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(serverPicker).toBeFocused();
  const language = page.locator(".language-trigger");
  await language.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("menuitemradio", { name: /English/ })).toHaveAttribute("aria-checked", "true");
  const languageItems = page.locator(".language-pop button");
  await expect(languageItems.first()).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(languageItems.last()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(language).toBeFocused();
  const account = page.locator(".shell-account-trigger");
  await account.focus();
  await page.keyboard.press("Enter");
  const accountItems = page.locator(".shell-account-menu a, .shell-account-menu button");
  await expect(accountItems.first()).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(accountItems.last()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(account).toBeFocused();
  await page.setViewportSize({ width: 390, height: 844 });
  const menu = page.locator(".shell-tabs button").filter({ hasText: "Menu" });
  await menu.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".shell-sidebar")).toHaveClass(/is-open/);
  await expect(page.getByRole("group", { name: "Language" }).getByRole("button")).toHaveCount(2);
  await expect(page.locator(".shell-close")).toBeFocused();
  const lastDrawerLink = page.locator(".shell-side-foot a").last();
  await page.keyboard.press("Shift+Tab");
  await expect(lastDrawerLink).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.locator(".shell-close")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(menu).toBeFocused();
});

test("open save bar leaves the last control reachable at 390px", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await login(page, `${user}777`);
  await page.goto(`/g/${guild}/moderation`);
  const toggle = page.getByRole("switch", { name: "Tell members" });
  await expect(toggle).toBeVisible();
  await toggle.click();
  const save = page.locator(".save-bar.is-open");
  await expect(save).toBeVisible();
  await openRow(page, /Warn ladder/);
  const lastControl = page.locator(".mod-page .add-actions button").last();
  await lastControl.scrollIntoViewIfNeeded();
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await expect
    .poll(async () => {
      const control = await lastControl.boundingBox();
      const bar = await save.boundingBox();
      return !!control && !!bar && control.y + control.height <= bar.y;
    })
    .toBe(true);
});
