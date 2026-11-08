import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ReefLoop, type ReefHome } from "./ReefLoop";
const home: ReefHome = {
  name: "Ripple <Nook>",
  fish: [],
  garden: { seaweed: 4, planted_at: null, watered_at: null },
};
describe("house reef loop", () => {
  it("escapes names and gives empty-tank and garden instructions", () => {
    const html = renderToStaticMarkup(<ReefLoop data={home} own />);
    expect(html).toContain("Ripple &lt;Nook&gt;");
    expect(html).toContain("/house aquarium");
    expect(html).toContain("/house craft");
    expect(html).not.toContain("<h1");
  });
  it("shows caught fish without leaking the owner's private garden", () => {
    const html = renderToStaticMarkup(
      <ReefLoop
        data={{
          ...home,
          garden: null,
          fish: [
            {
              slot: 1,
              id: "toaststripe_minnow",
              name: "Toaststripe Minnow",
              name_tr: "Kızarmış Çizgili Balık",
              size_cm: 5,
            },
          ],
        }}
        own={false}
      />,
    );
    expect(html).toContain("Toaststripe Minnow");
    expect(html).not.toContain("/house craft");
  });
});
