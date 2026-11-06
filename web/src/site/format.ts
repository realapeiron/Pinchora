/** `12,345` below ten thousand, `12.3K` and `1.2M` above. */
export function compact(n: number): string {
  if (n < 10_000) return n.toLocaleString("en-US");
  const unit = n < 1_000_000 ? { div: 1_000, suffix: "K" } : { div: 1_000_000, suffix: "M" };
  const value = n / unit.div;
  return `${value >= 100 ? Math.round(value) : Math.round(value * 10) / 10}${unit.suffix}`;
}
