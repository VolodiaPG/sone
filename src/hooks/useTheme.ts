import { useLayoutEffect } from "react";
import { useAtomValue } from "jotai";
import { themeAtom } from "../atoms/theme";
import { deriveTheme, isLightTheme, themeToCssVars } from "../lib/theme";
import { gradientDataUrl } from "../lib/gradientImage";

/**
 * Reads the current theme from the Jotai atom, derives all color tokens,
 * and injects them as CSS custom properties on :root.
 *
 * Call once near the top of the component tree (e.g. in App).
 */
export function useTheme() {
  const theme = useAtomValue(themeAtom);

  useLayoutEffect(() => {
    const derived = deriveTheme(theme.accent, theme.bgBase);
    const vars = themeToCssVars(derived);
    const root = document.documentElement;

    for (const [key, value] of Object.entries(vars)) {
      root.style.setProperty(key, value);
    }

    const pageGradient = gradientDataUrl(
      [
        { offset: 0, color: derived.bgSurface },
        { offset: 1, color: derived.bgBase },
      ],
      "vertical",
    );
    if (pageGradient) {
      root.style.setProperty("--page-gradient-image", `url(${pageGradient})`);
    } else {
      root.style.removeProperty("--page-gradient-image");
    }

    // Adapt native browser chrome (scrollbars, form controls) to theme
    root.style.colorScheme = isLightTheme(theme) ? "light" : "dark";
  }, [theme]);
}
