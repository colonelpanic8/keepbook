import type { ReactNode } from "react";
import { cx } from "../cx";

export interface ModalProps {
  title: string;
  /** Controls beside the title, typically a close `IconButton`. */
  headerActions?: ReactNode;
  /** Footer buttons, right-aligned. */
  actions?: ReactNode;
  /** Widens the dialog from 440px to 620px for editors. */
  wide?: boolean;
  children?: ReactNode;
}

/**
 * A centered dialog over a dimmed backdrop; render it only while open.
 *
 * Use it for focused edits and long-running operations, and prefer inline
 * feedback for everything else. Mirrors `components/layout/modal.rs`.
 */
export function Modal({ title, headerActions, actions, wide, children }: ModalProps) {
  return (
    <div className="modal-backdrop">
      <div className={cx("modal-dialog", wide && "wide")} role="dialog" aria-label={title}>
        <div className="modal-header">
          <h3>{title}</h3>
          {headerActions}
        </div>
        {children}
        {actions && <div className="modal-actions">{actions}</div>}
      </div>
    </div>
  );
}
