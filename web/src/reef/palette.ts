import data from "./palettes.json";
import { clone, type Img } from "../iso/image";
export const KEYS = {
  skin: [0x8a4b3b, 0xc97a5c, 0xf0a98a, 0xffd8c2],
  hair: [0x3b1f6b, 0x5e35a8, 0x8a5ad8, 0xb994f0],
  top: [0x0b5e6b, 0x0f8a9a, 0x19b8c8, 0x6fe0ea, 0xc0f6fa],
  bottom: [0x1d2b53, 0x2e3d75, 0x4a5ba3, 0x8fa0d8],
};
type RGB = [number, number, number];
export const rgb = (v: number): RGB => [(v >>> 16) & 255, (v >>> 8) & 255, v & 255];
const clamp = (n: number, a: number, b: number) => Math.min(b, Math.max(a, n));
const mod = (n: number) => ((n % 360) + 360) % 360;
/** Bot palette.rs OKLCH ramp: same hue lean, gamut reduction and shade spacing. */
export function deriveRamp(base: number, len: number, at = 2): RGB[] {
  const [r, g, b] = rgb(base).map((v) => {
    const c = v / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  }) as RGB;
  const l = Math.cbrt(0.41222146 * r + 0.53633255 * g + 0.051445995 * b),
    m = Math.cbrt(0.2119035 * r + 0.6806995 * g + 0.10739696 * b),
    s = Math.cbrt(0.08830246 * r + 0.28171885 * g + 0.6299787 * b);
  let light = 0.21045426 * l + 0.7936178 * m - 0.004072047 * s;
  const a = 1.9779985 * l - 2.4285922 * m + 0.4505937 * s,
    bb = 0.025904037 * l + 0.78277177 * m - 0.80867577 * s;
  const chroma = Math.hypot(a, bb),
    hue = mod((Math.atan2(bb, a) * 180) / Math.PI);
  let up = 0.105;
  if (light - at * 0.13 < 0.31) light = 0.31 + at * 0.13;
  const above = len - 1 - at;
  if (above > 0 && light + above * up > 0.975) {
    up = Math.max((0.975 - light) / above, 0.045);
    if (light + above * up > 0.975) light = 0.975 - above * up;
  }
  const lean = (target: number, degrees: number) =>
    mod(hue + clamp(mod(target - hue + 540) - 180, -degrees, degrees));
  return Array.from({ length: len }, (_, i) => {
    const k = i - at,
      steps = Math.abs(k),
      warmth = Math.min(chroma / 0.05, 1);
    const L = clamp(light + (k < 0 ? k * 0.13 : k * up), 0, 1),
      h = ((k < 0 ? lean(290, 7 * steps * warmth) : k > 0 ? lean(85, 5 * k * warmth) : hue) * Math.PI) / 180;
    let c =
      chroma *
      (k < 0 ? [1, 0.94, 0.82, 0.7][Math.min(steps, 3)]! : k > 0 ? [1, 0.78, 0.5, 0.32][Math.min(k, 3)]! : 1);
    const linear = (): RGB => {
      const a = c * Math.cos(h),
        b = c * Math.sin(h),
        l = (L + 0.39633778 * a + 0.21580376 * b) ** 3,
        m = (L - 0.105561346 * a - 0.06385417 * b) ** 3,
        s = (L - 0.08948418 * a - 1.2914855 * b) ** 3;
      return [
        4.0767417 * l - 3.3077116 * m + 0.23096994 * s,
        -1.268438 * l + 2.6097574 * m - 0.34131938 * s,
        -0.0041960863 * l - 0.7034186 * m + 1.7076147 * s,
      ];
    };
    let lin = linear();
    for (let t = 0; t < 24 && !lin.every((v) => v >= -0.0005 && v <= 1.0005); t++) {
      c *= 0.88;
      lin = linear();
    }
    return lin.map((v) => {
      const c = clamp(v, 0, 1);
      return Math.round((c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055) * 255);
    }) as RGB;
  });
}
export function paintRamp(paint: string, channel: keyof typeof KEYS): RGB[] {
  const len = channel === "skin" || channel === "hair" ? 4 : 5;
  if (/^#[a-f\d]{6}$/i.test(paint)) return deriveRamp(parseInt(paint.slice(1), 16), len);
  const swatches =
    channel === "skin" ? data.SKIN_TONES : channel === "hair" ? data.HAIR_COLORS : data.OUTFIT_COLORS;
  const swatch = swatches[Number(paint)] ?? swatches[0]!;
  return "ramp" in swatch && swatch.ramp ? swatch.ramp.slice(0, len).map(rgb) : deriveRamp(swatch.rgb!, len);
}
export function recolorCharacter(image: Img, code: string): Img {
  const parts = code.split("+")[0]!.split(".");
  const out = clone(image);
  const colors = new Map<number, RGB>();
  (["skin", "hair", "top", "bottom"] as const).forEach((channel, c) => {
    const ramp = paintRamp(parts[c + 1] ?? "0", channel);
    KEYS[channel].forEach((key, i) => colors.set(key, ramp[i]!));
  });
  for (let i = 0; i < out.data.length; i += 4) {
    if (!out.data[i + 3]) continue;
    const key = (out.data[i]! << 16) | (out.data[i + 1]! << 8) | out.data[i + 2]!;
    const c = colors.get(key);
    if (c) out.data.set(c, i);
  }
  return out;
}
