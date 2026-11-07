import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { createStore } from "jotai";
import {
  AUTO_THEME_STORAGE_KEY,
  THEME_STORAGE_KEY,
  type AutoTheme,
  type Theme,
} from "./theme";

const DARK_A: Theme = { name: "DarkA", accent: "#3B82F6", bgBase: "#0E1118" };
const DARK_B: Theme = { name: "DarkB", accent: "#F43F5E", bgBase: "#140E0F" };
const LIGHT_A: Theme = { name: "LightA", accent: "#2563EB", bgBase: "#F5F3EE" };
const LIGHT_B: Theme = { name: "LightB", accent: "#16A34A", bgBase: "#F2F6EE" };

const pair = (enabled: boolean): AutoTheme => ({
  enabled,
  light: LIGHT_A,
  dark: DARK_A,
});

let mq: {
  matches: boolean;
  listeners: Set<() => void>;
  addEventListener: (t: string, fn: () => void) => void;
  removeEventListener: (t: string, fn: () => void) => void;
};

function flipOs(dark: boolean) {
  mq.matches = dark;
  [...mq.listeners].forEach((fn) => fn());
}

// The atoms read localStorage and matchMedia at module-eval, so seed and stub
// first, then import a fresh module instance.
async function setup(opts: {
  osDark: boolean;
  auto?: AutoTheme;
  theme?: Theme;
}) {
  mq = {
    matches: opts.osDark,
    listeners: new Set(),
    addEventListener: (_t, fn) => void mq.listeners.add(fn),
    removeEventListener: (_t, fn) => void mq.listeners.delete(fn),
  };
  vi.stubGlobal("matchMedia", () => mq);
  if (opts.auto)
    localStorage.setItem(AUTO_THEME_STORAGE_KEY, JSON.stringify(opts.auto));
  if (opts.theme)
    localStorage.setItem(THEME_STORAGE_KEY, JSON.stringify(opts.theme));
  vi.resetModules();
  const atoms = await import("../atoms/theme");
  const { installAutoTheme } = await import("./autoTheme");
  return { ...atoms, installAutoTheme, store: createStore() };
}

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("installAutoTheme", () => {
  it("does nothing while disabled", async () => {
    const { store, installAutoTheme, themeAtom, autoThemeAtom } = await setup({
      osDark: true,
      auto: pair(false),
      theme: DARK_B,
    });
    installAutoTheme(store);
    expect(store.get(themeAtom)).toEqual(DARK_B);
    flipOs(false);
    expect(store.get(themeAtom)).toEqual(DARK_B);
    store.set(themeAtom, LIGHT_B);
    expect(store.get(autoThemeAtom)).toEqual(pair(false));
  });

  it("applies the dark slot on install when OS is dark", async () => {
    const { store, installAutoTheme, themeAtom } = await setup({
      osDark: true,
      auto: pair(true),
      theme: LIGHT_B,
    });
    installAutoTheme(store);
    expect(store.get(themeAtom)).toEqual(DARK_A);
  });

  it("switches theme with the OS and leaves the pair unchanged", async () => {
    const { store, installAutoTheme, themeAtom, autoThemeAtom } = await setup({
      osDark: true,
      auto: pair(true),
    });
    installAutoTheme(store);
    expect(store.get(themeAtom)).toEqual(DARK_A);
    flipOs(false);
    expect(store.get(themeAtom)).toEqual(LIGHT_A);
    expect(store.get(autoThemeAtom)).toEqual(pair(true));
    flipOs(true);
    expect(store.get(themeAtom)).toEqual(DARK_A);
    expect(store.get(autoThemeAtom)).toEqual(pair(true));
  });

  it("saves an external theme change into the current slot only", async () => {
    const { store, installAutoTheme, themeAtom, autoThemeAtom } = await setup({
      osDark: true,
      auto: pair(true),
    });
    installAutoTheme(store);
    store.set(themeAtom, DARK_B);
    expect(store.get(autoThemeAtom)).toEqual({ ...pair(true), dark: DARK_B });
    expect(store.get(autoThemeAtom).light).toEqual(LIGHT_A);
    flipOs(false);
    expect(store.get(themeAtom)).toEqual(LIGHT_A);
    store.set(themeAtom, LIGHT_B);
    expect(store.get(autoThemeAtom)).toEqual({
      enabled: true,
      light: LIGHT_B,
      dark: DARK_B,
    });
  });

  it("ignores writes to the other slot, follows writes to the current slot", async () => {
    const { store, installAutoTheme, themeAtom, autoThemeAtom } = await setup({
      osDark: true,
      auto: pair(true),
    });
    installAutoTheme(store);
    store.set(autoThemeAtom, { ...pair(true), light: LIGHT_B });
    expect(store.get(themeAtom)).toEqual(DARK_A);
    store.set(autoThemeAtom, { ...pair(true), light: LIGHT_B, dark: DARK_B });
    expect(store.get(themeAtom)).toEqual(DARK_B);
  });

  it("keeps the theme when enabling with the current slot already active", async () => {
    const { store, installAutoTheme, themeAtom, autoThemeAtom } = await setup({
      osDark: true,
      auto: pair(false),
      theme: DARK_B,
    });
    installAutoTheme(store);
    store.set(autoThemeAtom, { ...pair(true), dark: DARK_B });
    expect(store.get(themeAtom)).toEqual(DARK_B);
    expect(store.get(autoThemeAtom)).toEqual({ ...pair(true), dark: DARK_B });
  });

  it("stops syncing after unsubscribe", async () => {
    const { store, installAutoTheme, themeAtom, autoThemeAtom } = await setup({
      osDark: true,
      auto: pair(true),
    });
    const stop = installAutoTheme(store);
    expect(store.get(themeAtom)).toEqual(DARK_A);
    stop();
    flipOs(false);
    expect(store.get(themeAtom)).toEqual(DARK_A);
    store.set(themeAtom, DARK_B);
    expect(store.get(autoThemeAtom)).toEqual(pair(true));
    store.set(autoThemeAtom, { ...pair(true), dark: LIGHT_B });
    expect(store.get(themeAtom)).toEqual(DARK_B);
  });
});
