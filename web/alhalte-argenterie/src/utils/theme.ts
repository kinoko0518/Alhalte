/**
 * Material 3 Expressive Color System & Dynamic Theme Management.
 *
 * Strictly maps semantic roles (`--np-color-*`) from selected M3 tonal palettes
 * using `light-dark()` tokens on `:root` (`document.documentElement`), avoiding hardcoded
 * concrete colors inside UI components.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie` (Material 3 Expressive準拠)
 * - GitHub Issue #32: Material Color system & semantic theme selection
 */

export type ThemeMode = 'system' | 'light' | 'dark';
export type ThemePaletteId = 'azure' | 'violet' | 'emerald' | 'rose' | 'amber';

export interface ThemePaletteDefinition {
  id: ThemePaletteId;
  label: string;
  swatchColor: string;
  tokens: Record<string, string>;
}

export interface ThemeSettings {
  mode: ThemeMode;
  palette: ThemePaletteId;
}

export const LOCAL_STORAGE_THEME_KEY = 'alhalte:theme';

export const THEME_PALETTES: ThemePaletteDefinition[] = [
  {
    id: 'azure',
    label: 'Azure',
    swatchColor: '#00668c',
    tokens: {
      '--np-color-primary': 'light-dark(#00668c, #75ceff)',
      '--np-color-on-primary': 'light-dark(#f4f9ff, #003549)',
      '--np-color-primary-container': 'light-dark(#c1e8ff, #004d6a)',
      '--np-color-on-primary-container': 'light-dark(#001e2c, #c1e8ff)',
      '--np-color-secondary': 'light-dark(#4d616c, #b5cad6)',
      '--np-color-on-secondary': 'light-dark(#ffffff, #1f333d)',
      '--np-color-secondary-container': 'light-dark(#d0e6f3, #364954)',
      '--np-color-on-secondary-container': 'light-dark(#091e27, #d0e6f3)',
      '--np-color-tertiary': 'light-dark(#7c4b92, #efc2ff)',
      '--np-color-on-tertiary': 'light-dark(#fff7fc, #4a1c60)',
      '--np-color-tertiary-container': 'light-dark(#f8d8ff, #623378)',
      '--np-color-on-tertiary-container': 'light-dark(#310348, #f8d8ff)',
      '--np-color-error': 'light-dark(#ba1a1a, #ffb4ab)',
      '--np-color-on-error': 'light-dark(#ffffff, #690005)',
      '--np-color-error-container': 'light-dark(#ffdad6, #93000a)',
      '--np-color-on-error-container': 'light-dark(#410002, #ffdad6)',
      '--np-color-background': 'light-dark(#f7fafe, #0f1417)',
      '--np-color-on-background': 'light-dark(#181c1f, #dfe3e7)',
      '--np-color-surface': 'light-dark(#f7fafe, #0f1417)',
      '--np-color-on-surface': 'light-dark(#181c1f, #dfe3e7)',
      '--np-color-on-surface-variant': 'light-dark(#40484d, #c0c7cd)',
      '--np-color-surface-container-lowest': 'light-dark(#ffffff, #0a0f12)',
      '--np-color-surface-container-low': 'light-dark(#f1f4f8, #181c1f)',
      '--np-color-surface-container': 'light-dark(#ebeef2, #1c2024)',
      '--np-color-surface-container-high': 'light-dark(#e5e8ec, #262b2e)',
      '--np-color-surface-container-highest': 'light-dark(#dfe3e7, #313539)',
      '--np-color-outline': 'light-dark(#70787d, #8a9297)',
      '--np-color-outline-variant': 'light-dark(#c0c7cd, #40484d)',
      '--np-color-scrim': 'rgba(0, 0, 0, 0.32)',
    },
  },
  {
    id: 'violet',
    label: 'Violet',
    swatchColor: '#6750a4',
    tokens: {
      '--np-color-primary': 'light-dark(#6750a4, #d0bcff)',
      '--np-color-on-primary': 'light-dark(#ffffff, #381e72)',
      '--np-color-primary-container': 'light-dark(#eaddff, #4f378b)',
      '--np-color-on-primary-container': 'light-dark(#21005d, #eaddff)',
      '--np-color-secondary': 'light-dark(#625b71, #ccc2dc)',
      '--np-color-on-secondary': 'light-dark(#ffffff, #332d41)',
      '--np-color-secondary-container': 'light-dark(#e8def8, #4a4458)',
      '--np-color-on-secondary-container': 'light-dark(#1d192b, #e8def8)',
      '--np-color-tertiary': 'light-dark(#7d5260, #efb8c8)',
      '--np-color-on-tertiary': 'light-dark(#ffffff, #492532)',
      '--np-color-tertiary-container': 'light-dark(#ffd8e4, #633b48)',
      '--np-color-on-tertiary-container': 'light-dark(#31111d, #ffd8e4)',
      '--np-color-error': 'light-dark(#b3261e, #f2b8b5)',
      '--np-color-on-error': 'light-dark(#ffffff, #601410)',
      '--np-color-error-container': 'light-dark(#f9dedc, #8c1d18)',
      '--np-color-on-error-container': 'light-dark(#410e0b, #f9dedc)',
      '--np-color-background': 'light-dark(#fef7ff, #141218)',
      '--np-color-on-background': 'light-dark(#1d1b20, #e6e0e9)',
      '--np-color-surface': 'light-dark(#fef7ff, #141218)',
      '--np-color-on-surface': 'light-dark(#1d1b20, #e6e0e9)',
      '--np-color-on-surface-variant': 'light-dark(#49454f, #cac4d0)',
      '--np-color-surface-container-lowest': 'light-dark(#ffffff, #0f0d13)',
      '--np-color-surface-container-low': 'light-dark(#f7f2fa, #1d1b20)',
      '--np-color-surface-container': 'light-dark(#f3edf7, #211f26)',
      '--np-color-surface-container-high': 'light-dark(#ece6f0, #2b2930)',
      '--np-color-surface-container-highest': 'light-dark(#e6e0e9, #36343b)',
      '--np-color-outline': 'light-dark(#79747e, #938f99)',
      '--np-color-outline-variant': 'light-dark(#cac4d0, #49454f)',
      '--np-color-scrim': 'rgba(0, 0, 0, 0.32)',
    },
  },
  {
    id: 'emerald',
    label: 'Emerald',
    swatchColor: '#1b6d48',
    tokens: {
      '--np-color-primary': 'light-dark(#1b6d48, #8bd6af)',
      '--np-color-on-primary': 'light-dark(#ffffff, #003822)',
      '--np-color-primary-container': 'light-dark(#a6f2c9, #005234)',
      '--np-color-on-primary-container': 'light-dark(#002112, #a6f2c9)',
      '--np-color-secondary': 'light-dark(#4d6356, #b4ccbc)',
      '--np-color-on-secondary': 'light-dark(#ffffff, #1f3529)',
      '--np-color-secondary-container': 'light-dark(#d0e8d7, #364b3f)',
      '--np-color-on-secondary-container': 'light-dark(#0a1f15, #d0e8d7)',
      '--np-color-tertiary': 'light-dark(#894a65, #ffb0cf)',
      '--np-color-on-tertiary': 'light-dark(#ffffff, #531d36)',
      '--np-color-tertiary-container': 'light-dark(#ffd8e6, #6e334d)',
      '--np-color-on-tertiary-container': 'light-dark(#390720, #ffd8e6)',
      '--np-color-error': 'light-dark(#ba1a1a, #ffb4ab)',
      '--np-color-on-error': 'light-dark(#ffffff, #690005)',
      '--np-color-error-container': 'light-dark(#ffdad6, #93000a)',
      '--np-color-on-error-container': 'light-dark(#410002, #ffdad6)',
      '--np-color-background': 'light-dark(#f5fbf5, #0f1511)',
      '--np-color-on-background': 'light-dark(#171d1a, #dee4df)',
      '--np-color-surface': 'light-dark(#f5fbf5, #0f1511)',
      '--np-color-on-surface': 'light-dark(#171d1a, #dee4df)',
      '--np-color-on-surface-variant': 'light-dark(#404943, #bfc9c1)',
      '--np-color-surface-container-lowest': 'light-dark(#ffffff, #0a0f0c)',
      '--np-color-surface-container-low': 'light-dark(#eff5ef, #171d1a)',
      '--np-color-surface-container': 'light-dark(#e9efea, #1b211e)',
      '--np-color-surface-container-high': 'light-dark(#e4eae4, #252b28)',
      '--np-color-surface-container-highest': 'light-dark(#dee4df, #303632)',
      '--np-color-outline': 'light-dark(#707972, #8a938c)',
      '--np-color-outline-variant': 'light-dark(#bfc9c1, #404943)',
      '--np-color-scrim': 'rgba(0, 0, 0, 0.32)',
    },
  },
  {
    id: 'rose',
    label: 'Rose',
    swatchColor: '#8e455c',
    tokens: {
      '--np-color-primary': 'light-dark(#8e455c, #ffb1c5)',
      '--np-color-on-primary': 'light-dark(#ffffff, #55182f)',
      '--np-color-primary-container': 'light-dark(#ffd9e1, #712e45)',
      '--np-color-on-primary-container': 'light-dark(#3b031a, #ffd9e1)',
      '--np-color-secondary': 'light-dark(#75565e, #e4bdc5)',
      '--np-color-on-secondary': 'light-dark(#ffffff, #432930)',
      '--np-color-secondary-container': 'light-dark(#ffd9e1, #5b3f46)',
      '--np-color-on-secondary-container': 'light-dark(#2b151b, #ffd9e1)',
      '--np-color-tertiary': 'light-dark(#7b5635, #edbd94)',
      '--np-color-on-tertiary': 'light-dark(#ffffff, #462a0c)',
      '--np-color-tertiary-container': 'light-dark(#ffdcc0, #603f20)',
      '--np-color-on-tertiary-container': 'light-dark(#2d1600, #ffdcc0)',
      '--np-color-error': 'light-dark(#ba1a1a, #ffb4ab)',
      '--np-color-on-error': 'light-dark(#ffffff, #690005)',
      '--np-color-error-container': 'light-dark(#ffdad6, #93000a)',
      '--np-color-on-error-container': 'light-dark(#410002, #ffdad6)',
      '--np-color-background': 'light-dark(#fff8f8, #191113)',
      '--np-color-on-background': 'light-dark(#22191b, #efdfe1)',
      '--np-color-surface': 'light-dark(#fff8f8, #191113)',
      '--np-color-on-surface': 'light-dark(#22191b, #efdfe1)',
      '--np-color-on-surface-variant': 'light-dark(#514346, #d6c2c5)',
      '--np-color-surface-container-lowest': 'light-dark(#ffffff, #140c0e)',
      '--np-color-surface-container-low': 'light-dark(#fff0f2, #22191b)',
      '--np-color-surface-container': 'light-dark(#faeaed, #261d1f)',
      '--np-color-surface-container-high': 'light-dark(#f4e4e7, #31282a)',
      '--np-color-surface-container-highest': 'light-dark(#efdfe1, #3c3234)',
      '--np-color-outline': 'light-dark(#847376, #9f8c8f)',
      '--np-color-outline-variant': 'light-dark(#d6c2c5, #514346)',
      '--np-color-scrim': 'rgba(0, 0, 0, 0.32)',
    },
  },
  {
    id: 'amber',
    label: 'Amber',
    swatchColor: '#805610',
    tokens: {
      '--np-color-primary': 'light-dark(#805610, #f5be6f)',
      '--np-color-on-primary': 'light-dark(#ffffff, #452b00)',
      '--np-color-primary-container': 'light-dark(#ffddb2, #633f00)',
      '--np-color-on-primary-container': 'light-dark(#291800, #ffddb2)',
      '--np-color-secondary': 'light-dark(#6f5b40, #dec2a2)',
      '--np-color-on-secondary': 'light-dark(#ffffff, #3d2d16)',
      '--np-color-secondary-container': 'light-dark(#fbdebc, #56432b)',
      '--np-color-on-secondary-container': 'light-dark(#271904, #fbdebc)',
      '--np-color-tertiary': 'light-dark(#824c76, #f4b2e2)',
      '--np-color-on-tertiary': 'light-dark(#ffffff, #4e1f45)',
      '--np-color-tertiary-container': 'light-dark(#ffd7f3, #68355d)',
      '--np-color-on-tertiary-container': 'light-dark(#35082e, #ffd7f3)',
      '--np-color-error': 'light-dark(#ba1a1a, #ffb4ab)',
      '--np-color-on-error': 'light-dark(#ffffff, #690005)',
      '--np-color-error-container': 'light-dark(#ffdad6, #93000a)',
      '--np-color-on-error-container': 'light-dark(#410002, #ffdad6)',
      '--np-color-background': 'light-dark(#fff8f4, #18120b)',
      '--np-color-on-background': 'light-dark(#201b13, #ede0d4)',
      '--np-color-surface': 'light-dark(#fff8f4, #18120b)',
      '--np-color-on-surface': 'light-dark(#201b13, #ede0d4)',
      '--np-color-on-surface-variant': 'light-dark(#4f4539, #d3c4b4)',
      '--np-color-surface-container-lowest': 'light-dark(#ffffff, #130d07)',
      '--np-color-surface-container-low': 'light-dark(#fff1e5, #201b13)',
      '--np-color-surface-container': 'light-dark(#f9ebe0, #251f17)',
      '--np-color-surface-container-high': 'light-dark(#f3e5da, #2f2921)',
      '--np-color-surface-container-highest': 'light-dark(#ede0d4, #3b342b)',
      '--np-color-outline': 'light-dark(#817567, #9c8f80)',
      '--np-color-outline-variant': 'light-dark(#d3c4b4, #4f4539)',
      '--np-color-scrim': 'rgba(0, 0, 0, 0.32)',
    },
  },
];

const DEFAULT_SETTINGS: ThemeSettings = {
  mode: 'system',
  palette: 'azure',
};

/**
 * Retrieve persisted Material 3 theme settings from localStorage.
 */
export function getThemeSettings(): ThemeSettings {
  if (typeof localStorage === 'undefined') {
    return { ...DEFAULT_SETTINGS };
  }
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_THEME_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw) as Partial<ThemeSettings>;
    const mode: ThemeMode =
      parsed.mode === 'light' || parsed.mode === 'dark' || parsed.mode === 'system'
        ? parsed.mode
        : DEFAULT_SETTINGS.mode;
    const palette: ThemePaletteId = THEME_PALETTES.some((p) => p.id === parsed.palette)
      ? (parsed.palette as ThemePaletteId)
      : DEFAULT_SETTINGS.palette;
    return { mode, palette };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

/**
 * Persist and apply the selected Material 3 color palette.
 */
export function setThemePalette(paletteId: ThemePaletteId): ThemeSettings {
  const current = getThemeSettings();
  const next: ThemeSettings = { ...current, palette: paletteId };
  saveThemeSettings(next);
  applyThemeToDocument(next);
  return next;
}

/**
 * Persist and apply the selected color mode ('system' | 'light' | 'dark').
 */
export function setThemeMode(mode: ThemeMode): ThemeSettings {
  const current = getThemeSettings();
  const next: ThemeSettings = { ...current, mode };
  saveThemeSettings(next);
  applyThemeToDocument(next);
  return next;
}

function saveThemeSettings(settings: ThemeSettings): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(LOCAL_STORAGE_THEME_KEY, JSON.stringify(settings));
  } catch {
    // Ignore storage write errors
  }
}

/**
 * Apply Material 3 semantic color tokens (`--np-color-*`) and theme mode to `document.documentElement`.
 */
export function applyThemeToDocument(settings: ThemeSettings = getThemeSettings()): void {
  if (typeof document === 'undefined' || !document.documentElement) return;
  const root = document.documentElement;

  if (settings.mode === 'light' || settings.mode === 'dark') {
    root.dataset.theme = settings.mode;
  } else {
    delete root.dataset.theme;
  }
  root.dataset.themePalette = settings.palette;

  const paletteDef =
    THEME_PALETTES.find((p) => p.id === settings.palette) ?? THEME_PALETTES[0];

  for (const [token, value] of Object.entries(paletteDef.tokens)) {
    root.style.setProperty(token, value);
  }
}
