import { translate as spaTranslate } from "../../i18n";
import { type ReactNode } from "react";
import { clamp } from "../../lib/card";
import { PixelToggle } from "../PixelToggle";
import { ATLAS_SIZE, ATLAS_URL, type AtlasIndex } from "./CardSvg";

/** Applies an edit; edits with the same `merge` key in a row make one undo step (typing, sliding). */
export type Edit<T> = (next: T, merge?: string) => void;

// /Small controls

export function Row({
  label,
  htmlFor,
  children,
  wide,
}: {
  label: string;
  htmlFor?: string;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <div className={`prop ${wide ? "prop-wide" : ""}`}>
      <label className="prop-label" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
    </div>
  );
}

export function NumberField({
  id,
  label,
  value,
  range,
  onChange,
}: {
  id: string;
  label: string;
  value: number;
  range: readonly number[];
  onChange: (n: number) => void;
}) {
  return (
    <Row label={label} htmlFor={id}>
      <input
        id={id}
        className="pixel-input prop-number"
        type="number"
        inputMode="numeric"
        value={value}
        min={range[0]}
        max={range[1]}
        onChange={(event) => {
          const n = Number(event.target.value);
          if (event.target.value !== "" && Number.isFinite(n)) onChange(clamp(Math.round(n), range));
        }}
      />
    </Row>
  );
}

export function Slider({
  id,
  label,
  value,
  range,
  unit = "",
  onChange,
}: {
  id: string;
  label: string;
  value: number;
  range: readonly number[];
  unit?: string;
  onChange: (n: number) => void;
}) {
  return (
    <Row label={`${label}: ${value}${unit}`} htmlFor={id} wide>
      <input
        id={id}
        className="prop-slider"
        type="range"
        min={range[0]}
        max={range[1]}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </Row>
  );
}

export function ColorField({
  id,
  label,
  value,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (hex: string) => void;
}) {
  return (
    <Row label={label} htmlFor={id}>
      <span className="prop-color">
        <input
          type="color"
          aria-label={`${label} picker`}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
        <input
          id={id}
          className="pixel-input"
          value={value}
          maxLength={7}
          spellCheck={false}
          onChange={(event) => {
            const hex = event.target.value.trim();
            if (/^#[0-9a-fA-F]{6}$/.test(hex)) onChange(hex.toLowerCase());
          }}
        />
      </span>
    </Row>
  );
}

export function Choice<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="prop prop-wide" role="radiogroup" aria-label={label}>
      <span className="prop-label">{label}</span>
      <span className="segmented">
        {options.map((o) => (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={value === o.value}
            className={`segment ${value === o.value ? "is-checked" : ""}`}
            onClick={() => onChange(o.value)}
          >
            {spaTranslate(o.label)}
          </button>
        ))}
      </span>
    </div>
  );
}

export function Switch({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <div className="prop prop-switch">
      <span className="prop-label">{label}</span>
      <PixelToggle label={label} checked={checked} onChange={onChange} />
    </div>
  );
}

/** A piece of the atlas as a small picture, pixels kept square. */
export function AtlasThumb({ atlas, id, size }: { atlas: AtlasIndex | undefined; id: string; size: number }) {
  const region = atlas?.[id];
  if (!region) return <span className="atlas-thumb" style={{ width: size, height: size }} />;
  const k = Math.min(size / region.w, size / region.h);
  return (
    <svg
      className="atlas-thumb"
      width={region.w * k}
      height={region.h * k}
      viewBox={`${region.x} ${region.y} ${region.w} ${region.h}`}
      aria-hidden="true"
    >
      <image
        href={ATLAS_URL}
        width={ATLAS_SIZE.w}
        height={ATLAS_SIZE.h}
        style={{ imageRendering: "pixelated" }}
      />
    </svg>
  );
}
