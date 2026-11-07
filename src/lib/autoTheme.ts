import type { createStore } from "jotai";
import { autoThemeAtom, systemAppearanceAtom, themeAtom } from "../atoms/theme";
import { type Theme, themesEqual } from "./theme";

type Store = ReturnType<typeof createStore>;

/**
 * Keep `themeAtom` in step with the OS appearance while auto theme is on.
 *
 * Two rules, and each one is a no-op when the other just ran:
 * - When the OS appearance or the stored light/dark pair changes, the theme
 *   for the current appearance becomes the active theme.
 * - When the active theme changes from anywhere else (the Themes tab, an
 *   edited theme.json), it is saved as the theme for the current appearance.
 *
 * Call before the first render so the first frame already uses the right
 * theme. Returns a function that stops the sync.
 */
const sameTheme = (a: Theme, b: Theme) =>
  a.name === b.name && themesEqual(a, b);

export function installAutoTheme(store: Store): () => void {
  const applyFromPair = () => {
    const auto = store.get(autoThemeAtom);
    if (!auto.enabled) return;
    const next = auto[store.get(systemAppearanceAtom)];
    if (!sameTheme(next, store.get(themeAtom))) store.set(themeAtom, next);
  };

  const saveToPair = () => {
    const auto = store.get(autoThemeAtom);
    if (!auto.enabled) return;
    const appearance = store.get(systemAppearanceAtom);
    const current = store.get(themeAtom);
    if (!sameTheme(auto[appearance], current)) {
      store.set(autoThemeAtom, { ...auto, [appearance]: current });
    }
  };

  // store.sub mounts systemAppearanceAtom, which starts the OS listener.
  const unsubs = [
    store.sub(systemAppearanceAtom, applyFromPair),
    store.sub(autoThemeAtom, applyFromPair),
    store.sub(themeAtom, saveToPair),
  ];
  applyFromPair();
  return () => unsubs.forEach((u) => u());
}
