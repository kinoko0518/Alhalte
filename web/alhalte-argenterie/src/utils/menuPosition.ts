/**
 * menuPosition.ts - Menu positioning utility & actions for noph-ui Menu in Alhalte Argenterie.
 *
 * Solves Issue #124 and Issue #241:
 * HTML Popover API with CSS Anchor Positioning defaults to top-center of the screen
 * when anchor positioning is unsupported, or centers via `@position-try --np-menu-over-anchor`
 * (`align-self: center`, `position-area: span-all`) when Chrome/Blink anchor positioning
 * detects collision or unsupported anchor features.
 *
 * This utility calculates the anchor's viewport bounding rect and applies
 * explicit inline styles with `!important` to override Anchor Positioning
 * auto-centering fallbacks and interference:
 *   position: fixed !important;
 *   inset: unset !important;
 *   margin: 0 !important;
 *   position-area: none !important;
 *   position-try-fallbacks: none !important;
 *   align-self: unset !important;
 *   justify-self: unset !important;
 *
 * `!important` alone does not neutralize the HTML UA stylesheet, though. The UA
 * sheet gives every `[popover]` element `position: fixed; inset: 0`, and that
 * `0` still applies to any side that has no author declaration of its own.
 * For a fixed, `fit-content` box the over-constrained inset resolution then
 * prefers `left` / `top` in LTR, which collapses the menu to x=0 and/or y=0
 * (Issue #241). `inset` is a shorthand, and `removeProperty()` on one of its
 * longhands also drops the author declaration for that side, which lets the UA
 * `0` leak back in. This utility therefore never removes an inset longhand and
 * always pins all four sides in every update:
 *   - bottom placement: `top: <px> !important;` plus `bottom: auto !important;`
 *   - top placement: `bottom: <px> !important;` plus `top: auto !important;`
 *   - start placement: `left: <px> !important;` plus `right: auto !important;`
 *   - end placement: `right: <px> !important;` plus `left: auto !important;`
 *
 * Both the string path (`getMenuPositionStyle` -> `computeMenuPosition().styleString`,
 * rendered into the `style` attribute) and the DOM path (`applyMenuPosition`, run
 * from a `$effect`) are built from the same declaration list, so a later
 * re-render cannot reintroduce the missing-side bug (Issue #241).
 *
 * This eliminates size estimation discrepancies (e.g. small delete menus appearing
 * far below the 3-dots button) and prevents right-center or top-center drift.
 */

export type MenuPlacement =
  | 'bottom'
  | 'bottom-start'
  | 'bottom-end'
  | 'top'
  | 'top-start'
  | 'top-end'
  | 'auto';

export interface MenuPositionOptions {
  /** Desired placement relative to anchor. Default: 'auto' */
  placement?: MenuPlacement;
  /** Distance in pixels between anchor and menu. Default: 4 */
  offset?: number;
  /** Minimum distance in pixels from viewport edges. Default: 8 */
  margin?: number;
  /** Optional CSS Anchor Positioning name (e.g. '--gear-menu-anchor') */
  anchorName?: string;
  /** Estimated menu width in px if menu element size is not yet measured. Default: 180 */
  estimatedWidth?: number;
  /** Estimated menu height in px if menu element size is not yet measured. Default: 200 */
  estimatedHeight?: number;
  /** Whether to use bottom property instead of top when placed above anchor. Default: true */
  useBottomForTopPlacement?: boolean;
  /** Whether to use right property instead of left when end-aligned. Default: true */
  useRightForEndPlacement?: boolean;
}

export interface ComputedMenuPosition {
  top?: number;
  bottom?: number;
  left?: number;
  right?: number;
  maxHeight?: number;
  isTop: boolean;
  styleString: string;
}

/**
 * One inline declaration, shared by the string path and the DOM path.
 */
interface MenuPositionDeclaration {
  property: string;
  value: string;
  important: boolean;
}

/**
 * Builds the inline declarations shared by the string path (the `style`
 * attribute) and the DOM path (`applyMenuPosition`). Using one list guarantees
 * that both paths pin the same four inset longhands, so a re-render cannot
 * reintroduce an unpinned side (Issue #241).
 *
 * The unused side of each axis is still declared as `auto !important`: without
 * an author declaration the UA `[popover] { inset: 0 }` rule wins for that side
 * and Chrome collapses the fixed, `fit-content` menu to x=0 / y=0 (Issue #241).
 */
function buildMenuPositionDeclarations(
  result: ComputedMenuPosition,
  options: MenuPositionOptions,
): MenuPositionDeclaration[] {
  const pin = (value: number | undefined): string =>
    value === undefined ? 'auto' : `${value}px`;

  const declarations: MenuPositionDeclaration[] = [
    { property: 'position', value: 'fixed', important: true },
    // Must come before the longhands: setting the `inset` shorthand resets all
    // four of them, and the explicit pins below have to win afterwards.
    { property: 'inset', value: 'unset', important: true },
    { property: 'top', value: pin(result.top), important: true },
    { property: 'bottom', value: pin(result.bottom), important: true },
    { property: 'left', value: pin(result.left), important: true },
    { property: 'right', value: pin(result.right), important: true },
    { property: 'margin', value: '0', important: true },
    { property: 'position-area', value: 'none', important: true },
    { property: 'position-try-fallbacks', value: 'none', important: true },
    { property: 'align-self', value: 'unset', important: true },
    { property: 'justify-self', value: 'unset', important: true },
  ];

  if (result.maxHeight !== undefined) {
    declarations.push({
      property: 'max-height',
      value: `${result.maxHeight}px`,
      important: true,
    });
    declarations.push({ property: 'overflow-y', value: 'auto', important: true });
  }

  if (options.anchorName) {
    declarations.push({
      property: 'position-anchor',
      value: options.anchorName,
      important: false,
    });
    declarations.push({
      property: '--np-menu-position-area',
      value: result.isTop ? 'top span-right' : 'bottom span-right',
      important: false,
    });
  }

  return declarations;
}

/**
 * Serializes {@link buildMenuPositionDeclarations} into the `style` attribute
 * value used by `getMenuPositionStyle` (Issue #241).
 */
function menuPositionStyleString(
  result: ComputedMenuPosition,
  options: MenuPositionOptions,
): string {
  return (
    buildMenuPositionDeclarations(result, options)
      .map(({ property, value, important }) =>
        `${property}: ${value}${important ? ' !important' : ''}`,
      )
      .join('; ') + ';'
  );
}

/**
 * Calculates menu position based on anchor element bounds and viewport dimensions.
 */
export function computeMenuPosition(
  anchor: HTMLElement | null | undefined,
  menu?: HTMLElement | null,
  options: MenuPositionOptions = {}
): ComputedMenuPosition {
  const offset = options.offset ?? 4;
  const margin = options.margin ?? 8;
  const estimatedWidth = options.estimatedWidth ?? 180;
  const estimatedHeight = options.estimatedHeight ?? 200;
  const useBottomForTop = options.useBottomForTopPlacement ?? true;
  const useRightForEnd = options.useRightForEndPlacement ?? true;

  const vw = typeof window !== 'undefined'
    ? (window.innerWidth || document.documentElement?.clientWidth || 1024)
    : 1024;
  const vh = typeof window !== 'undefined'
    ? (window.innerHeight || document.documentElement?.clientHeight || 768)
    : 768;

  if (!anchor) {
    const fallback = { top: margin, left: margin, isTop: false, styleString: '' };
    return {
      ...fallback,
      styleString: menuPositionStyleString(fallback, options),
    };
  }

  const anchorRect = anchor.getBoundingClientRect();
  const menuRect = menu?.getBoundingClientRect();
  const menuWidth = (menuRect && menuRect.width > 0)
    ? menuRect.width
    : (menu?.offsetWidth || estimatedWidth);
  const menuHeight = (menuRect && menuRect.height > 0)
    ? menuRect.height
    : (menu?.offsetHeight || estimatedHeight);

  // Determine vertical placement
  const placement = options.placement ?? 'auto';
  let isTop = false;

  const spaceBelow = vh - anchorRect.bottom - offset - margin;
  const spaceAbove = anchorRect.top - offset - margin;

  if (placement.startsWith('top')) {
    isTop = true;
    // If not enough space above but plenty below, flip to bottom
    if (spaceAbove < menuHeight && spaceBelow >= menuHeight) {
      isTop = false;
    }
  } else if (placement.startsWith('bottom')) {
    isTop = false;
    // If not enough space below but plenty above, flip to top
    if (spaceBelow < menuHeight && spaceAbove >= menuHeight) {
      isTop = true;
    }
  } else {
    // 'auto' placement:
    // If space below is insufficient and space above has more room, flip to top
    if (spaceBelow < menuHeight && spaceAbove > spaceBelow) {
      isTop = true;
    } else {
      isTop = false;
    }
  }

  let top: number | undefined;
  let bottom: number | undefined;

  if (isTop) {
    if (useBottomForTop) {
      bottom = Math.round(vh - anchorRect.top + offset);
    } else {
      top = Math.round(Math.max(margin, anchorRect.top - offset - menuHeight));
    }
  } else {
    top = Math.round(anchorRect.bottom + offset);
  }

  const availableHeight = isTop ? spaceAbove : spaceBelow;
  const maxHeight = Math.max(0, Math.floor(availableHeight));

  // Determine horizontal alignment
  let left: number | undefined;
  let right: number | undefined;
  const isEndAligned = placement.endsWith('-end');

  if (useRightForEnd && isEndAligned) {
    let r = Math.round(vw - anchorRect.right);
    r = Math.max(margin, r);
    if (vw >= menuWidth + margin * 2) {
      r = Math.min(r, vw - menuWidth - margin);
    }
    right = r;
  } else if (isEndAligned) {
    left = anchorRect.right - menuWidth;
    left = Math.round(Math.max(margin, Math.min(left, vw - menuWidth - margin)));
  } else if (placement.endsWith('-start')) {
    let l = Math.round(anchorRect.left);
    l = Math.max(margin, l);
    if (vw >= menuWidth + margin * 2) {
      l = Math.min(l, vw - menuWidth - margin);
    }
    left = l;
  } else {
    // Auto horizontal alignment:
    if (anchorRect.left + menuWidth > vw - margin) {
      let r = Math.round(vw - anchorRect.right);
      r = Math.max(margin, r);
      if (vw >= menuWidth + margin * 2) {
        r = Math.min(r, vw - menuWidth - margin);
      }
      right = r;
    } else {
      let l = Math.round(anchorRect.left);
      l = Math.max(margin, l);
      if (vw >= menuWidth + margin * 2) {
        l = Math.min(l, vw - menuWidth - margin);
      }
      left = l;
    }
  }

  const result = { top, bottom, left, right, maxHeight, isTop, styleString: '' };

  return {
    ...result,
    styleString: menuPositionStyleString(result, options),
  };
}

/**
 * Helper returning inline style string for a Menu component.
 * Also configures `anchor-name` on the anchor element if `anchorName` option is provided.
 */
export function getMenuPositionStyle(
  anchor: HTMLElement | null | undefined,
  options: MenuPositionOptions = {}
): string {
  if (anchor && options.anchorName) {
    if (anchor.style.getPropertyValue('anchor-name') !== options.anchorName) {
      anchor.style.setProperty('anchor-name', options.anchorName);
    }
  }
  return computeMenuPosition(anchor, undefined, options).styleString;
}

/**
 * Applies calculated menu position directly to menu element, and handles
 * window resize and scroll events to keep it positioned adjacent to the anchor.
 * Returns a cleanup function.
 */
export function applyMenuPosition(
  menu: HTMLElement | null | undefined,
  anchor: HTMLElement | null | undefined,
  options: MenuPositionOptions = {}
): () => void {
  if (!menu) return () => {};

  if (anchor && options.anchorName) {
    anchor.style.setProperty('anchor-name', options.anchorName);
  }

  const update = () => {
    if (!menu || !anchor) return;
    const result = computeMenuPosition(anchor, menu, options);

    // Same declaration list as `getMenuPositionStyle` -> `styleString`, so the
    // `$effect` and the `style` attribute never disagree. In particular, do not
    // `removeProperty()` any inset longhand: that drops the author declaration
    // for the side and lets the UA popover `inset: 0` collapse the menu (#241).
    for (const { property, value, important } of buildMenuPositionDeclarations(result, options)) {
      menu.style.setProperty(property, value, important ? 'important' : '');
    }
  };

  update();

  if (typeof window !== 'undefined') {
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    return () => {
      window.removeEventListener('resize', update);
      window.removeEventListener('scroll', update, true);
    };
  }

  return () => {};
}

/**
 * Svelte action to apply adjacent position to a Menu element.
 */
export function menuPosition(
  node: HTMLElement,
  params: { anchor?: HTMLElement | null; options?: MenuPositionOptions } | HTMLElement | null
) {
  let anchor = params && 'anchor' in params ? params.anchor : (params as HTMLElement | null);
  let options = params && 'options' in params ? params.options : undefined;

  let cleanup = applyMenuPosition(node, anchor, options);

  return {
    update(newParams: { anchor?: HTMLElement | null; options?: MenuPositionOptions } | HTMLElement | null) {
      cleanup();
      anchor = newParams && 'anchor' in newParams ? newParams.anchor : (newParams as HTMLElement | null);
      options = newParams && 'options' in newParams ? newParams.options : undefined;
      cleanup = applyMenuPosition(node, anchor, options);
    },
    destroy() {
      cleanup();
    },
  };
}
