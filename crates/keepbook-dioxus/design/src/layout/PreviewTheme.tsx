import { useLayoutEffect, type ReactNode } from "react";
import { applyTheme, storedTheme } from "./Theme";

/**
 * Preview-card wrapper: renders the card in the selected theme and follows changes made in other cards.
 *
 * Not for designs; it is the sync's preview provider.
 */
export function PreviewTheme({ children }: { children: ReactNode }) {
  useLayoutEffect(() => {
    document.body.style.background = "var(--color-bg)";
    const sync = () => applyTheme(storedTheme());
    sync();
    window.addEventListener("storage", sync);
    return () => window.removeEventListener("storage", sync);
  }, []);
  return <>{children}</>;
}
