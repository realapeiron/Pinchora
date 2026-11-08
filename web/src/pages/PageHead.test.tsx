import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it } from "vitest";
import { Panel } from "../components/Panel";
import { setLocale, translate } from "../i18n";
import { SettingsPage } from "./PageHead";

afterEach(() => setLocale("en"));

it.each(["en", "tr"] as const)("keeps one page title and distinct panel headings in %s", async (language) => {
  await setLocale(language);
  const html = renderToStaticMarkup(
    <SettingsPage title={translate("ACHIEVEMENTS")} lead="">
      <Panel title={translate("Achievements")} aside={<button>25</button>}>
        <p>0/25</p>
      </Panel>
      <Panel title={translate("CELEBRATIONS")}>
        <p>100</p>
      </Panel>
    </SettingsPage>,
  );
  expect(html.match(/<h1\b/g)).toHaveLength(1);
  expect(html.match(/<h2\b/g)).toHaveLength(1);
  expect(html).toContain(`<h1 class="pixel-heading">${translate("ACHIEVEMENTS")}</h1>`);
  // The panel now references the visible heading as its accessible region name.
  const headingId = html.match(/aria-labelledby="([^"]+)"/)?.[1];
  expect(headingId).toBeTruthy();
  expect(html.match(/aria-labelledby=/g)).toHaveLength(1);
  expect(html).toContain(`<h2 id="${headingId}" class="pixel-title">${translate("CELEBRATIONS")}</h2>`);
  expect(html).toContain("<button>25</button>");
  expect(html).toContain("<p>0/25</p>");
});

it("keeps standalone panel titles outside a settings page", () => {
  const html = renderToStaticMarkup(
    <Panel title={translate("ACHIEVEMENTS")}>
      <p>0/25</p>
    </Panel>,
  );
  const headingId = html.match(/aria-labelledby="([^"]+)"/)?.[1];
  expect(headingId).toBeTruthy();
  expect(html).toContain(`<h2 id="${headingId}" class="pixel-title">ACHIEVEMENTS</h2>`);
});
