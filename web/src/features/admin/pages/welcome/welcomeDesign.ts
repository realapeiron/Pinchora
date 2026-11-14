import { t as deferredText } from "../../../../i18n/dashboard";
import {
  type CardDesign,
  type CardKind,
  type CardView,
  type Delivery,
  CARD,
  designCost,
  heavyMessage,
  unknownPlaceholders,
} from "../../../../lib/card";

/** One image in the editor: its design, how it is sent, and whether it is the server's own design. */
export interface Side {
  design: CardDesign;
  delivery: Delivery;
  custom: boolean;
}

export type Draft = Record<CardKind, Side>;

export const KINDS: { kind: CardKind; label: string }[] = [
  { kind: "welcome", label: "Welcome" },
  { kind: "goodbye", label: "Goodbye" },
  { kind: "boost", label: "Boost" },
  { kind: "level_up", label: "Level up" },
];

/** The member number the canvas shows, as the bot's preview does. */
export const SAMPLE_MEMBERS = 1234;
export const ZOOMS = [0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2];

export const sideOf = (view: CardView): Side => ({
  design: view.design,
  delivery: view.delivery,
  custom: view.custom,
});
export const same = (a: Side, b: Side) => JSON.stringify(a) === JSON.stringify(b);

/** What the browser can tell is wrong with a design before the server sees it. */
export function problems(design: CardDesign): string[] {
  const layers = design.layers.flatMap((layer, i) => {
    if (layer.type !== "text") return [];
    const name = deferredText("Layer {arg1} (Text)", { arg1: i + 1 });
    if (layer.text.trim() === "") return [`${name}: write something, or delete the layer.`];
    const unknown = unknownPlaceholders(layer.text);
    return unknown.length ? [`${name}: ${unknown[0]} isn't a placeholder.`] : [];
  });
  const avatars = design.layers.filter((l) => l.type === "avatar").length;
  return [
    ...layers,
    ...(avatars > CARD.max_avatars
      ? [deferredText("A design has at most {arg1} avatars.", { arg1: CARD.max_avatars })]
      : []),
    ...(designCost(design.layers) > CARD.layer_cost_max ? [heavyMessage(design.layers)] : []),
  ];
}
