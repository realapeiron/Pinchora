let measurer: CanvasRenderingContext2D | null = null;

/** How wide `text` draws, as near as the browser can tell: the bot measures with the same font files. */
export function textWidth(text: string, family: string, bold: boolean, size: number): number {
  measurer ??= document.createElement("canvas").getContext("2d");
  if (!measurer) return text.length * size * 0.6;
  measurer.font = `${bold ? 700 : 400} ${size}px "${family}"`;
  return measurer.measureText(text).width;
}

/** The longest start of `text` that fits `max` with an ellipsis after it; `text` itself when it fits. */
function fit(text: string, max: number, width: (t: string) => number): string {
  if (width(text) <= max) return text;
  const graphemes = [...new Intl.Segmenter("en", { granularity: "grapheme" }).segment(text)].map(
    (s) => s.segment,
  );
  let [fits, over] = [0, graphemes.length];
  while (over - fits > 1) {
    const mid = (fits + over) >> 1;
    if (width(graphemes.slice(0, mid).join("") + "…") <= max) fits = mid;
    else over = mid;
  }
  return graphemes.slice(0, fits).join("") + "…";
}

/** `text` in at most `maxLines` lines of at most `max` px, as the bot wraps it. */
export function wrapText(
  text: string,
  max: number,
  maxLines: number,
  width: (t: string) => number,
): string[] {
  const words = text.split(/\s+/).filter(Boolean);
  if (maxLines <= 1) return [fit(words.join(" "), max, width)];
  const lines: string[] = [];
  const starts = [0];
  let line = "";
  words.forEach((word, i) => {
    if (!line) {
      line = word;
      return;
    }
    const candidate = `${line} ${word}`;
    if (width(candidate) <= max) line = candidate;
    else {
      lines.push(line);
      line = word;
      starts.push(i);
    }
  });
  if (line) lines.push(line);
  if (lines.length > maxLines) {
    lines.length = maxLines;
    lines[maxLines - 1] = words.slice(starts[maxLines - 1]).join(" ");
  }
  return lines.map((l) => fit(l, max, width));
}
