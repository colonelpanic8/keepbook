import { useLayoutEffect, type ReactNode } from "react";

/**
 * Preview-card wrapper: renders the card in the stored theme on its page background.
 *
 * Not for designs; it is the sync's preview provider. `assets/theme.js`
 * applies the theme and follows changes made in other cards.
 */
export function PreviewTheme({ children }: { children: ReactNode }) {
  useLayoutEffect(() => {
    document.body.style.background = "var(--color-bg)";
    window.keepbookTheme?.apply();
  }, []);
  return <>{children}</>;
}
