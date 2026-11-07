import { atom } from "jotai";
import {
  atomWithStorage,
  createJSONStorage,
  unstable_withStorageValidator,
} from "jotai/utils";
import {
  type Appearance,
  type AutoTheme,
  type Theme,
  AUTO_THEME_STORAGE_KEY,
  DEFAULT_AUTO_THEME,
  PRESET_THEMES,
  THEME_STORAGE_KEY,
  isAutoTheme,
  isTheme,
} from "../lib/theme";

// getOnInit reads localStorage at module-eval so the first painted frame uses
// the real theme rather than the default preset. The validator is required,
// not defensive: without it a valid-JSON-wrong-shape value (`null`, `42`)
// would reach the atom and make deriveTheme throw before render() ever runs.
const themeStorage =
  unstable_withStorageValidator(isTheme)(createJSONStorage<unknown>());

export const themeAtom = atomWithStorage<Theme>(
  THEME_STORAGE_KEY,
  PRESET_THEMES[0],
  themeStorage,
  { getOnInit: true },
);

const autoThemeStorage =
  unstable_withStorageValidator(isAutoTheme)(createJSONStorage<unknown>());

/** The light and dark themes used when the theme follows the OS. */
export const autoThemeAtom = atomWithStorage<AutoTheme>(
  AUTO_THEME_STORAGE_KEY,
  DEFAULT_AUTO_THEME,
  autoThemeStorage,
  { getOnInit: true },
);

const DARK_QUERY = "(prefers-color-scheme: dark)";

function readAppearance(): Appearance {
  return window.matchMedia?.(DARK_QUERY).matches ? "dark" : "light";
}

/** The current OS appearance. Tracks changes while mounted. */
export const systemAppearanceAtom = atom<Appearance>(readAppearance());

systemAppearanceAtom.onMount = (set) => {
  const mq = window.matchMedia?.(DARK_QUERY);
  if (!mq) return;
  const update = () => set(mq.matches ? "dark" : "light");
  update();
  mq.addEventListener("change", update);
  return () => mq.removeEventListener("change", update);
};
