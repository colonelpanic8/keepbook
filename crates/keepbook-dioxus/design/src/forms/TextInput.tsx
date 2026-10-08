import type { ChangeEventHandler } from "react";
import { cx } from "../cx";

export interface TextInputProps {
  /** Field label above the input; omit for inline inputs such as search boxes. */
  label?: string;
  value?: string;
  placeholder?: string;
  /** `text`, `number`, `date`, … */
  type?: string;
  /** A multi-line text area (e.g. prompts and notes). */
  multiline?: boolean;
  /** Compact height for dense editors. */
  small?: boolean;
  disabled?: boolean;
  onChange?: ChangeEventHandler<HTMLInputElement | HTMLTextAreaElement>;
}

/**
 * A text, number, date, or multi-line input, optionally labeled above.
 *
 * With a `label` it renders as a labeled field that stacks the label above the
 * control, the way every keepbook form field does.
 */
export function TextInput({ label, value, placeholder, type = "text", multiline, small, disabled, onChange }: TextInputProps) {
  const control = multiline ? (
    <textarea
      className={cx("control-input", "ai-rule-prompt")}
      defaultValue={value}
      placeholder={placeholder}
      disabled={disabled}
      onChange={onChange}
    />
  ) : (
    <input
      className={cx("control-input", small && "small")}
      type={type}
      defaultValue={value}
      placeholder={placeholder}
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
