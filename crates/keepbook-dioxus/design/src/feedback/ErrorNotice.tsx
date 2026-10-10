import { IconButton } from "../actions/IconButton";
import { renderIcon } from "../actions/icons";

export interface ErrorNoticeProps {
  /** The full failure, naming what failed: "Price refresh failed: quote source unavailable." */
  message: string;
  onDismiss?: () => void;
}

/**
 * A failure of work the user started, in full, beneath the control that started it.
 *
 * Unlike other feedback it stays until dismissed or the work is retried, so the
 * message can't be missed. Mirrors `components/feedback/error_notice.rs`.
 */
export function ErrorNotice({ message, onDismiss }: ErrorNoticeProps) {
  return (
    <div className="notice error" role="alert">
      {renderIcon("circle-alert")}
      <span>{message}</span>
      <IconButton label="Dismiss" glyph="×" className="notice-dismiss" onClick={() => onDismiss?.()} />
    </div>
  );
}
