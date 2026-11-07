import { useState, useEffect, useCallback, useRef } from "react";
import { useAtom, useAtomValue } from "jotai";
import {
  themeAtom,
  autoThemeAtom,
  systemAppearanceAtom,
} from "../../atoms/theme";
import {
  PRESET_THEMES,
  DEFAULT_AUTO_THEME,
  deriveTheme,
  type Appearance,
  type Theme,
} from "../../lib/theme";
import { Check, RotateCcw } from "lucide-react";
import ColorPicker from "./ColorPicker";
import ThemeMiniPreview from "./ThemeMiniPreview";
import Toggle from "../Toggle";
import SettingRow from "./SettingRow";

const SECTION_LABEL =
  "text-[10.5px] font-bold tracking-[1.4px] uppercase text-th-text-faint mb-3";

type PickerTarget = "accent" | "bg";

export default function ThemesTab() {
  const [theme, setTheme] = useAtom(themeAtom);
  const [auto, setAuto] = useAtom(autoThemeAtom);
  const systemAppearance = useAtomValue(systemAppearanceAtom);
  const [editing, setEditing] = useState<Appearance>(systemAppearance);

  // Follow the OS when it changes
  useEffect(() => {
    setEditing(systemAppearance);
  }, [systemAppearance]);

  // Theme the rest of this tab edits
  const edited = auto.enabled ? auto[editing] : theme;
  const setEdited = useCallback(
    (t: Theme) => {
      if (auto.enabled) setAuto((a) => ({ ...a, [editing]: t }));
      else setTheme(t);
    },
    [auto.enabled, editing, setAuto, setTheme],
  );

  const [localAccent, setLocalAccent] = useState(edited.accent);
  const [localBg, setLocalBg] = useState(edited.bgBase);
  const [activePreset, setActivePreset] = useState<string | null>(
    PRESET_THEMES.find(
      (p) => p.accent === edited.accent && p.bgBase === edited.bgBase,
    )?.name ?? null,
  );

  // Color-picker popover state
  const [picker, setPicker] = useState<PickerTarget | null>(null);
  const [pickerPos, setPickerPos] = useState<{ top: number; left: number }>({
    top: 0,
    left: 0,
  });
  const sectionRef = useRef<HTMLElement>(null);
  const accentSwatchRef = useRef<HTMLButtonElement>(null);
  const bgSwatchRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  // Sync local state when the edited theme changes externally
  useEffect(() => {
    setLocalAccent(edited.accent);
    setLocalBg(edited.bgBase);
    setActivePreset(
      PRESET_THEMES.find(
        (p) => p.accent === edited.accent && p.bgBase === edited.bgBase,
      )?.name ?? null,
    );
  }, [edited]);

  // Close popover on outside click
  useEffect(() => {
    if (!picker) return;
    const onDocClick = (e: MouseEvent) => {
      const target = e.target as Node;
      if (popoverRef.current?.contains(target)) return;
      if (accentSwatchRef.current?.contains(target)) return;
      if (bgSwatchRef.current?.contains(target)) return;
      setPicker(null);
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [picker]);

  // Apply live preview as user edits
  const applyPreview = useCallback(
    (accent: string, bgBase: string) => {
      setEdited({ name: "Custom", accent, bgBase });
    },
    [setEdited],
  );

  const handlePresetClick = (preset: Theme) => {
    setLocalAccent(preset.accent);
    setLocalBg(preset.bgBase);
    setActivePreset(preset.name);
    setEdited(preset);
  };

  const handleAccentChange = (hex: string) => {
    setLocalAccent(hex);
    setActivePreset(null);
    applyPreview(hex, localBg);
  };

  const handleBgChange = (hex: string) => {
    setLocalBg(hex);
    setActivePreset(null);
    applyPreview(localAccent, hex);
  };

  const handleReset = () => {
    const defaultTheme = auto.enabled
      ? DEFAULT_AUTO_THEME[editing]
      : PRESET_THEMES[0];
    setLocalAccent(defaultTheme.accent);
    setLocalBg(defaultTheme.bgBase);
    setActivePreset(defaultTheme.name);
    setEdited(defaultTheme);
  };

  const togglePicker = (
    target: PickerTarget,
    anchor: HTMLButtonElement | null,
  ) => {
    if (picker === target) {
      setPicker(null);
      return;
    }
    const section = sectionRef.current;
    if (anchor && section) {
      const a = anchor.getBoundingClientRect();
      const s = section.getBoundingClientRect();
      setPickerPos({
        top: a.bottom - s.top + 8,
        left: a.left - s.left,
      });
    }
    setPicker(target);
  };

  const toggleAuto = () => {
    if (auto.enabled) {
      setAuto((a) => ({ ...a, enabled: false }));
    } else {
      // Keep the active theme: store it in the slot for the current appearance
      setAuto((a) => ({ ...a, enabled: true, [systemAppearance]: theme }));
    }
  };

  return (
    <div>
      {/* Appearance */}
      <section className="mb-[30px]">
        <h3 className={SECTION_LABEL}>Appearance</h3>
        <div className="rounded-[14px] bg-th-surface border border-th-border-subtle overflow-hidden">
          <SettingRow
            title="Match system appearance"
            subtitle="Use a light theme in light mode and a dark theme in dark mode."
          >
            <button
              type="button"
              role="switch"
              aria-checked={auto.enabled}
              aria-label="Match system appearance"
              onClick={toggleAuto}
              className="cursor-pointer"
            >
              <Toggle on={auto.enabled} />
            </button>
          </SettingRow>
        </div>
        {auto.enabled && (
          <div
            className="mt-3 inline-flex rounded-[10px] bg-th-inset border border-th-border-subtle p-0.5"
            role="group"
            aria-label="Theme to edit"
          >
            {(["light", "dark"] as const).map((a) => {
              const selected = editing === a;
              return (
                <button
                  key={a}
                  type="button"
                  aria-pressed={selected}
                  onClick={() => setEditing(a)}
                  className={`cursor-pointer rounded-[8px] px-3.5 py-1.5 text-[12.5px] font-semibold transition-colors ${
                    selected
                      ? "bg-th-accent text-th-on-accent"
                      : "text-th-text-secondary hover:text-th-text-primary"
                  }`}
                >
                  {a === "light" ? "Light" : "Dark"}
                  {systemAppearance === a && (
                    <span className="ml-1.5 text-[11px] font-normal opacity-70">
                      (current)
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        )}
      </section>

      {/* Presets */}
      <section>
        <h3 className={SECTION_LABEL}>Presets</h3>
        <div className="grid grid-cols-5 gap-2.5">
          {PRESET_THEMES.map((p) => {
            const active = activePreset === p.name;
            const d = deriveTheme(p.accent, p.bgBase);
            return (
              <button
                key={p.name}
                onClick={() => handlePresetClick(p)}
                className="cursor-pointer text-left"
              >
                <span
                  className="relative flex h-[52px] flex-col justify-between rounded-[10px] px-[11px] py-[9px] transition"
                  style={{
                    background: d.bgSurface,
                    boxShadow: `inset 0 0 0 1px ${d.borderSubtle}`,
                    outline: active
                      ? `1.5px solid ${p.accent}`
                      : "1.5px solid transparent",
                  }}
                >
                  <span className="flex gap-1.5">
                    <span
                      className="block h-[15px] w-[15px] rounded-full"
                      style={{ background: p.accent }}
                    />
                    <span
                      className="block h-[15px] w-[15px] rounded-full"
                      style={{
                        background: p.bgBase,
                        boxShadow: `inset 0 0 0 1px ${d.borderSubtle}`,
                      }}
                    />
                  </span>
                  <span
                    className="block truncate text-[11px] font-medium leading-none"
                    style={{ color: d.textPrimary }}
                  >
                    {p.name}
                  </span>
                  {active && (
                    <span
                      className="absolute right-1.5 top-1.5 grid h-[15px] w-[15px] place-items-center rounded-full"
                      style={{ background: p.accent, color: d.onAccent }}
                    >
                      <Check size={9} />
                    </span>
                  )}
                </span>
              </button>
            );
          })}
        </div>
      </section>

      {/* Custom + picker + preview */}
      <section ref={sectionRef} className="relative mt-[30px]">
        <h3 className={SECTION_LABEL}>Custom</h3>

        <div className="flex items-center gap-[11px]">
          <span className="text-[12.5px] text-th-text-secondary w-[92px] shrink-0">
            Accent
          </span>
          <button
            ref={accentSwatchRef}
            type="button"
            onClick={() => togglePicker("accent", accentSwatchRef.current)}
            className="w-[34px] h-[34px] rounded-[9px] border border-th-border-subtle cursor-pointer p-0 shrink-0"
            style={{ background: localAccent }}
            aria-label="Pick accent color"
          />
          <input
            type="text"
            value={localAccent}
            onChange={(e) => {
              const v = e.target.value;
              if (/^#[0-9A-Fa-f]{0,6}$/.test(v)) {
                setLocalAccent(v);
                if (/^#[0-9A-Fa-f]{6}$/.test(v)) {
                  setActivePreset(null);
                  applyPreview(v, localBg);
                }
              }
            }}
            className="w-[112px] px-[11px] py-2 text-[12.5px] font-mono uppercase bg-th-inset border border-th-border-subtle rounded-[9px] text-th-text-primary outline-none focus:border-th-accent transition-colors"
            maxLength={7}
            spellCheck={false}
          />
        </div>

        <div className="flex items-center gap-[11px] mt-[11px]">
          <span className="text-[12.5px] text-th-text-secondary w-[92px] shrink-0">
            Background
          </span>
          <button
            ref={bgSwatchRef}
            type="button"
            onClick={() => togglePicker("bg", bgSwatchRef.current)}
            className="w-[34px] h-[34px] rounded-[9px] border border-th-border-subtle cursor-pointer p-0 shrink-0"
            style={{ background: localBg }}
            aria-label="Pick background color"
          />
          <input
            type="text"
            value={localBg}
            onChange={(e) => {
              const v = e.target.value;
              if (/^#[0-9A-Fa-f]{0,6}$/.test(v)) {
                setLocalBg(v);
                if (/^#[0-9A-Fa-f]{6}$/.test(v)) {
                  setActivePreset(null);
                  applyPreview(localAccent, v);
                }
              }
            }}
            className="w-[112px] px-[11px] py-2 text-[12.5px] font-mono uppercase bg-th-inset border border-th-border-subtle rounded-[9px] text-th-text-primary outline-none focus:border-th-accent transition-colors"
            maxLength={7}
            spellCheck={false}
          />
        </div>

        {/* Preview */}
        <h3 className={`${SECTION_LABEL} mt-[30px]`}>Preview</h3>
        <ThemeMiniPreview accent={localAccent} bg={localBg} />

        <button
          onClick={handleReset}
          className="mt-[18px] inline-flex items-center gap-2 px-4 py-2 text-[12.5px] font-semibold text-th-text-secondary border border-th-border-subtle rounded-[10px] hover:text-th-text-primary hover:border-th-text-faint transition-colors"
        >
          <RotateCcw size={15} /> Reset to default
        </button>

        {/* Color-picker popover */}
        {picker && (
          <div
            ref={popoverRef}
            className="absolute z-50"
            style={{ top: pickerPos.top, left: pickerPos.left }}
          >
            <ColorPicker
              value={picker === "accent" ? localAccent : localBg}
              onChange={
                picker === "accent" ? handleAccentChange : handleBgChange
              }
            />
          </div>
        )}
      </section>
    </div>
  );
}
