import type { ChangeEventHandler } from "react";
import { cx } from "../cx";

export interface TextInputProps {
  /** Field label above the input; omit for inline inputs such as search boxes. */
  label?: string;
  value?: string;
  /** `text` (default), `number`, `date`, `search`, or `color` (a swatch that opens the platform picker). */
  kind?: "text" | "number" | "date" | "search" | "color";
  placeholder?: string;
  /** Bounds for date inputs. */
  min?: string;
  max?: string;
  /** Compact height for dense editors. */
  small?: boolean;
  /** A multi-line text area, e.g. for prompts. */
  multiline?: boolean;
  disabled?: boolean;
  className?: string;
  onChange?: ChangeEventHandler<HTMLInputElement | HTMLTextAreaElement>;
}

/**
 * A text, number, date, search, color, or multi-line input, optionally labeled above.
 *
 * With a `label` it renders as a labeled field that stacks the label above the
 * control, the way every keepbook form field does. Mirrors
 * `components/forms/text_input.rs`.
 */
export function TextInput({
  label,
  value,
  kind = "text",
  placeholder,
  min,
  max,
  small,
  multiline,
  disabled,
  className,
  onChange,
}: TextInputProps) {
  const classes = cx("control-input", small && "small", multiline && "ai-rule-prompt", className);
  const control = multiline ? (
    <textarea className={classes} defaultValue={value} placeholder={placeholder} disabled={disabled} onChange={onChange} />
  ) : (
    <input
      className={classes}
      type={kind}
      defaultValue={value}
      placeholder={placeholder}
      min={min}
      max={max}
      step={kind === "number" ? "0.01" : undefined}
      disabled={disabled}
      onChange={onChange}
    />
  );
  if (!label) return control;
  return (
    <label className="control-field">
      <span>{label}</span>
      {control}
    </label>
  );
}
