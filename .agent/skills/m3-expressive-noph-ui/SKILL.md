---
name: m3-expressive-noph-ui
description: Material 3 Expressive UI Element Selection & Noph UI (noph-ui) Implementation Guide for Alhalte Argenterie. Read this whenever modifying or adding UI components in web/alhalte-argenterie.
---

# Material 3 Expressive & Noph UI (`noph-ui`) Guide

`alhalte-argenterie` uses **Material 3 Expressive** via the **`noph-ui`** Svelte 5 component library (`import { ... } from 'noph-ui'`) and **Google Material Symbols Outlined** (`import { Icon } from 'noph-ui/icons'`).

**CRITICAL RULE**: Never build ad-hoc custom `<button>`, `<input type="range">`, `<input type="checkbox">` (for switches), custom tab bars, custom sheets/drawers, or hardcoded hex colors (`#333`, `#ff0000`, `red`, etc.) when an M3 Expressive `noph-ui` component and `--np-color-*` semantic token exist!

---

## 1. Theme & Material Color Tokens (`noph-ui/defaultTheme`)

1. Always import `noph-ui/defaultTheme` (in `main.ts` or root `App.svelte`) and ensure `<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Material+Symbols+Outlined:opsz,wght,FILL,GRAD@20..48,100..700,0..1,-50..200" />` is present in `index.html`.
2. **Never use hardcoded hex/RGB colors** in component styles. Use M3 semantic CSS custom properties:
   - Surfaces:
     - `--np-color-background`, `--np-color-on-background`
     - `--np-color-surface`, `--np-color-on-surface`, `--np-color-on-surface-variant`
     - `--np-color-surface-container-lowest`, `--np-color-surface-container-low`, `--np-color-surface-container`, `--np-color-surface-container-high`, `--np-color-surface-container-highest`
   - Primary / Secondary / Tertiary:
     - `--np-color-primary`, `--np-color-on-primary`, `--np-color-primary-container`, `--np-color-on-primary-container`
     - `--np-color-secondary`, `--np-color-on-secondary`, `--np-color-secondary-container`, `--np-color-on-secondary-container`
     - `--np-color-tertiary`, `--np-color-on-tertiary`, `--np-color-tertiary-container`, `--np-color-on-tertiary-container`
   - Error / Destructive (e.g., Delete buttons):
     - `--np-color-error`, `--np-color-on-error`, `--np-color-error-container`, `--np-color-on-error-container`
     - For a softer / expressive destructive tonal button (such as "シーンを削除" in Scene Settings), style the `Button` (`variant="tonal"`) with container color `var(--np-color-error-container)` or `var(--np-color-tertiary-container)` and text color `var(--np-color-on-error-container)` / `var(--np-color-on-tertiary-container)` via `--np-button-container-color` and `--np-button-label-text-color` instead of raw red!
   - Outlines & Elevation:
     - `--np-color-outline`, `--np-color-outline-variant`
     - `--np-elevation-1`, `--np-elevation-2`, `--np-elevation-3`, `--np-elevation-4`
   - Shapes & Expressive Motion:
     - `--np-shape-corner-extra-small` (4px), `--np-shape-corner-small` (8px), `--np-shape-corner-medium` (12px), `--np-shape-corner-large` (16px), `--np-shape-corner-extra-large` (28px), `--np-shape-corner-full` (pill)
     - `--np-motion-expressive-fast-spatial`, `--np-motion-expressive-default-spatial`, `--np-motion-expressive-fast-effects`
3. **Theme Selection Support**:
   - Support switching light/dark mode (`document.documentElement.dataset.theme = 'light' | 'dark'`) and dynamic color seed / theme presets (updating `--np-color-primary`, `--np-color-primary-container`, `--np-color-secondary-container`, `--np-color-tertiary-container`, etc. on `:root` or `data-theme-color`).

---

## 2. Icons (`noph-ui/icons` & Material Symbols Outlined)

Import `Icon` from `'noph-ui/icons'`:
```svelte
<script lang="ts">
  import { Icon } from 'noph-ui/icons';
</script>

<Icon>send</Icon>
<Icon>add</Icon>
<Icon>delete</Icon>
<Icon>person_pin_circle</Icon>
```
When passing an icon into a `noph-ui` component snippet (`icon`, `start`, `end`, etc.):
```svelte
{#snippet sendIcon()}<Icon>send</Icon>{/snippet}
<IconButton variant="filled" aria-label="送信" onclick={handleSend}>
  {@render sendIcon()}
</IconButton>
```
Note: `IconButton` renders its `children` snippet inside `.button-icon`, so you pass `<Icon>...</Icon>` directly as children of `<IconButton>`:
```svelte
<IconButton variant="tonal" aria-label="新規作成" onclick={createNew}>
  <Icon>add</Icon>
</IconButton>
```

---

## 3. UI Element Selection Guide (M3 Expressive Mapping)

### A. Buttons & Actions (`Button`, `IconButton`, `Fab`, `ExtendedFab`, `SegmentedButton`, `SplitButton`)
Import from `'noph-ui'`:
- **`Button`**:
  - Props: `variant?: 'filled' | 'tonal' | 'outlined' | 'elevated' | 'text'`, `size?: 'xs' | 's' | 'm' | 'l' | 'xl'`, `shape?: 'round' | 'square'`, `start?: Snippet`, `end?: Snippet`, `disabled?: boolean`, `onclick`.
  - Choose `variant` by visual hierarchy:
    - Primary action in a view/section: `variant="filled"`
    - Secondary action (medium emphasis): `variant="tonal"`
    - Tertiary / alternative action: `variant="outlined"` or `variant="text"`
- **`IconButton`**:
  - Props: `variant?: 'text' | 'filled' | 'outlined' | 'tonal'`, `size?: 'xs' | 's' | 'm' | 'l' | 'xl'`, `shape?: 'round' | 'square'`, `width?: 'narrow' | 'wide' | 'default'`, `toggle?: boolean`, `selected?: boolean`, `aria-label: string`.
  - Must be used for compact icon-only actions:
    - Message Send (`send` icon, `variant="filled"`)
    - Character Create (`person_add` or `add` icon, `variant="filled"` or `variant="tonal"`)
    - Character "駒として配置" (`person_pin_circle` / `place_item` icon, `variant="tonal"`)
    - Character "削除" (`delete` icon, `variant="text"` or `variant="tonal"`)
- **`SegmentedButton`**:
  - Props: `name: string`, `options: SegmentedButtonOption[]` (`{ value, label, disabled, selected, onclick }`), `bind:group`, `multiSelect?: boolean`.
  - Use for mutually exclusive view/mode switches (e.g., Asset Picker's `自分のアセット | 部屋のアセット` and `画像 | 音声`).

### B. Navigation & Tabs (`Tabs`, `Tab`, `Tooltip`)
Import `Tabs`, `Tab`, `Tooltip` from `'noph-ui'`:
- Control Panel navigation must use `<Tabs variant="primary" bind:value={activeTab}>` with `<Tab value={...} icon={...} aria-label={...} aria-describedby="tooltip-id">`.
- Attach `<Tooltip id="tooltip-id">ラベル名</Tooltip>` to each tab so hovering/focusing displays an M3 Tooltip explaining the icon tab!
- Tab icons (5 tabs after merging Scene Management into Scene Tab):
  1. `log` (`ログタブ`): `<Icon>forum</Icon>` (or `chat`)
  2. `scene` (`シーンタブ`): `<Icon>account_tree</Icon>`
  3. `scene-settings` (`シーン設定タブ`): `<Icon>tune</Icon>` (or `DisplaySettings` / `movie_edit`)
  4. `character` (`キャラクター管理タブ`): `<Icon>group</Icon>` (or `badge`)
  5. `extension` (`拡張機能管理タブ`): `<Icon>extension</Icon>`

### C. Control Panel Container (`Sheet`: Side Sheet & Bottom Sheet)
Import `Sheet` from `'noph-ui'`:
- Props: `bind:open`, `modal?: boolean`, `placement?: 'end' | 'bottom' | 'start' | 'top'`, `handle?: boolean`, `bind:expanded`, `detached?: boolean`, `headline?: string`, `action?: Snippet`.
- In **Landscape mode** (`orientation: landscape` / desktop):
  - Control Panel is a **Side Sheet** (`placement="end"`, `modal={false}`) that can be collapsed/expanded smoothly using a toggle `IconButton` (`right_panel_close` / `right_panel_open` or `chevron_right` / `chevron_left`) available across **all** tabs (not just LogTab!).
- In **Portrait mode** (`orientation: portrait` / mobile):
  - Control Panel is a **Bottom Sheet** (`placement="bottom"`, `handle={true}`) that supports collapsing/expanding.

### D. Switches & Sliders (`Switch`, `Slider`)
- **`Switch`**:
  - Import `{ Switch } from 'noph-ui'`.
  - Props: `bind:selected={boolean}`, `icons?: 'selected' | 'both'`, `disabled?: boolean`, `aria-label?: string`.
  - Use for boolean settings such as `"Juicesを継承"` in Scene Settings and extension enable/disable toggles.
- **`Slider`**:
  - Import `{ Slider } from 'noph-ui'`.
  - Props: `bind:value={number}`, `min={0}`, `max={100}`, `step={1}`, `size?: 'xs' | 's' | 'm' | 'l' | 'xl'`, `labeled?: boolean`.
  - Use for all BGM volume controls (`0`–`100`).

### E. Carousel (`Carousel`, `CarouselItem`)
- Import `{ Carousel, CarouselItem } from 'noph-ui'`.
- Props:
  - `<Carousel variant="multi-browse" label="...">` (or `variant="uncontained"`)
  - `<CarouselItem type="button" image={url} label={name} onclick={() => selectImage(id)} />`
- Use in Scene Settings (Background image selection) and Character Management (Standing picture / icon selection) to allow quick 1-click selection of available image assets (including `"NONE"`), with an `"画像選択"` `<Button variant="tonal">` directly below the Carousel to open the full Asset Picker overlay.

### F. Menus (`Menu`, `MenuItem`) & Outside-Click Dismissal
- Import `{ Menu, MenuItem } from 'noph-ui'`.
- Any popover/dropdown menu (Character selector menu in `CommonFooter`, Log tab gear filter menu in `LogTab`, Scene selector menu in `SceneSettingsTab`) **must close when clicking or tapping outside** (either by using `noph-ui`'s `<Menu bind:open anchor={btnEl}>` which uses HTML `popover="auto"`, or by attaching a `document` `pointerdown`/`click` listener that closes the menu when `!menuEl.contains(event.target)`).

### G. Dialogs (`Dialog`)
- Import `{ Dialog } from 'noph-ui'`.
- Props: `bind:open`, `variant?: 'basic' | 'full-screen'`, `headline?: string`, `supportingText?: string`, `icon?: Snippet`, `actions?: Snippet`.
- Use for confirmation warnings (such as deleting a scene node from the Scene Graph).
