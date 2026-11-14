import { flushSync } from "react-dom";
import { t as deferredText } from "../../i18n/dashboard";
import { translate as spaTranslate } from "../../i18n";
import { useRef } from "react";
import {
  type Canvas,
  type TextLayer,
  CARD,
  maxTextSize,
  textHeight,
  unknownPlaceholders,
} from "../../lib/card";
import { type Edit, Row, ColorField, Choice, Slider, Switch } from "./PropertyControls";

export function TextProperties({
  layer,
  canvas,
  onChange,
  simple,
}: {
  layer: TextLayer;
  canvas: Canvas;
  onChange: Edit<TextLayer>;
  simple: boolean;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  const unknown = unknownPlaceholders(layer.text);
  const setSize = (size: number, merge?: string) =>
    onChange({ ...layer, size, h: textHeight(size, layer.max_lines) }, merge);
  const insert = (token: string) => {
    const el = area.current;
    const [start, end] = el ? [el.selectionStart, el.selectionEnd] : [layer.text.length, layer.text.length];
    const text = (layer.text.slice(0, start) + token + layer.text.slice(end)).slice(0, CARD.text_max);
    // Commit before placing the caret; a deferred focus can steal typing from the next field.
    flushSync(() => onChange({ ...layer, text }));
    el?.focus();
    el?.setSelectionRange(start + token.length, start + token.length);
  };
  return (
    <>
      <Row label={spaTranslate("Text")} htmlFor="layer-text" wide>
        <textarea
          id="layer-text"
          ref={area}
          className="pixel-input pixel-textarea prop-text"
          value={layer.text}
          maxLength={CARD.text_max}
          rows={2}
          aria-invalid={unknown.length > 0 || layer.text.trim() === "" || undefined}
          onChange={(event) => onChange({ ...layer, text: event.target.value }, "text")}
        />
      </Row>
      <div className="chips" aria-label={spaTranslate("Placeholders")}>
        {CARD.placeholders.map((p) => (
          <button key={p} type="button" className="chip" onClick={() => insert(p)}>
            {p}
          </button>
        ))}
      </div>
      {unknown.length > 0 && (
        <p className="field-error" role="alert">
          {unknown[0]} {deferredText("isn't a placeholder. Use")} {CARD.placeholders.join(" ")}.
        </p>
      )}
      {layer.text.trim() === "" && (
        <p className="field-error" role="alert">
          {" "}
          {deferredText("Write something, or delete the layer.")}{" "}
        </p>
      )}
      <Choice
        label={spaTranslate("Font")}
        value={layer.font}
        options={[
          { value: "sans", label: "Noto Sans" },
          { value: "pixel", label: spaTranslate("Pixel") },
        ]}
        onChange={(font) => onChange({ ...layer, font })}
      />
      <Slider
        id="layer-size"
        label={spaTranslate("Size")}
        value={layer.size}
        range={[CARD.text_size[0] ?? 8, maxTextSize(layer.max_lines, canvas.height)]}
        unit=" px"
        onChange={(size) => setSize(size, "size")}
      />
      <ColorField
        id="layer-color"
        label={spaTranslate("Colour")}
        value={layer.color}
        onChange={(color) => onChange({ ...layer, color })}
      />
      <Choice
        label={spaTranslate("Align")}
        value={layer.align}
        options={[
          { value: "left", label: spaTranslate("Left") },
          { value: "center", label: spaTranslate("Centre") },
          { value: "right", label: spaTranslate("Right") },
        ]}
        onChange={(align) => onChange({ ...layer, align })}
      />
      {!simple && (
        <>
          <div className="prop-grid">
            <Switch
              label={deferredText("Bold")}
              checked={layer.bold}
              onChange={(bold) => onChange({ ...layer, bold })}
            />
            <Switch
              label={deferredText("Capitals")}
              checked={layer.uppercase}
              onChange={(uppercase) => onChange({ ...layer, uppercase })}
            />
            <Switch
              label={spaTranslate("Shadow")}
              checked={layer.shadow}
              onChange={(shadow) => onChange({ ...layer, shadow })}
            />
          </div>
          <Slider
            id="layer-lines"
            label={deferredText("Lines at most")}
            value={layer.max_lines}
            range={[1, CARD.max_lines]}
            onChange={(max_lines) =>
              onChange({ ...layer, max_lines, h: textHeight(layer.size, max_lines) }, "lines")
            }
          />
          <Slider
            id="layer-outline"
            label={spaTranslate("Outline")}
            value={layer.outline_width}
            range={[0, CARD.outline_max]}
            unit=" px"
            onChange={(outline_width) => onChange({ ...layer, outline_width }, "outline")}
          />
          {layer.outline_width > 0 && (
            <ColorField
              id="layer-outline-color"
              label={deferredText("Outline colour")}
              value={layer.outline_color}
              onChange={(outline_color) => onChange({ ...layer, outline_color })}
            />
          )}
        </>
      )}
    </>
  );
}
