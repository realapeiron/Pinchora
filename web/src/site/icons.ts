// Pixel icons for the feature grid: 12 by 12 art pixels, one letter a colour, a dot nothing.

import type { IconName } from "./features";

export const ICON_SIZE = 12;

/** Letter to colour; the same palette as the bot's cards. */
export const PALETTE: Record<string, string> = {
  a: "var(--accent)",
  g: "var(--gold)",
  m: "var(--mint)",
  o: "var(--ink)",
  d: "var(--outline)",
  b: "#3a4c8c",
  p: "#232e5c",
  s: "var(--muted)",
  h: "var(--heart)",
};

export const ICONS: Record<IconName, string[]> = {
  house: [
    ".....aa.....",
    "....aaaa....",
    "...aaaaaa...",
    "..aaaaaaaa..",
    ".aaaaaaaaaa.",
    "aaaaaaaaaaaa",
    ".oooooooooo.",
    ".ommooooggo.",
    ".ommooooggo.",
    ".oooooooggo.",
    ".sssssssssg.",
    "............",
  ],
  dice: [
    ".oooooooooo.",
    "oooooooooooo",
    "ooaaoooooaao",
    "ooaaoooooaao",
    "oooooooooooo",
    "oooooaaooooo",
    "oooooaaooooo",
    "oooooooooooo",
    "ooaaoooooaas",
    "ooaaoooooaas",
    "ossssssssss.",
    ".ssssssssss.",
  ],
  card: [
    "gggggggggggg",
    "gppppppppppg",
    "gpaaapoooopg",
    "gpaaappppppg",
    "gpaaapmmmmpg",
    "gppppppppppg",
    "gpoooooooopg",
    "gppppppppppg",
    "gpoooooopppg",
    "gppppppppppg",
    "gggggggggggg",
    "............",
  ],
  trophy: [
    "gggggggggggg",
    "g.gggggggg.g",
    "g.gggoogg.g.",
    "g.gggoogg.g.",
    "gggggggggggg",
    ".gggggggggg.",
    "..gggggggg..",
    "....gggg....",
    ".....gg.....",
    ".....gg.....",
    "..gggggggg..",
    "..gggggggg..",
  ],
  sliders: [
    "..oo........",
    "mmoossssssss",
    "..oo........",
    "............",
    ".......oo...",
    "mmmmmmmoosss",
    ".......oo...",
    "............",
    "....oo......",
    "mmmmoossssss",
    "....oo......",
    "............",
  ],
  window: [
    "bbbbbbbbbbbb",
    "baahhhbbbbbb",
    "bbbbbbbbbbbb",
    "bppppppppppb",
    "bpmmmpoooopb",
    "bpmmmppppppb",
    "bpmmmpoooopb",
    "bppppppppppb",
    "bpooooooaapb",
    "bppppppppppb",
    "bbbbbbbbbbbb",
    "............",
  ],
};

export interface Run {
  x: number;
  y: number;
  w: number;
  fill: string;
}

/** The icon as horizontal runs of one colour, so it draws as few rectangles as it can. */
export function runs(rows: string[]): Run[] {
  const out: Run[] = [];
  rows.forEach((row, y) => {
    let x = 0;
    while (x < row.length) {
      const letter = row[x];
      let end = x + 1;
      while (end < row.length && row[end] === letter) end += 1;
      const fill = letter ? PALETTE[letter] : undefined;
      if (fill) out.push({ x, y, w: end - x, fill });
      x = end;
    }
  });
  return out;
}
