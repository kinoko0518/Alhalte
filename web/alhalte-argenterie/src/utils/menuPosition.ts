/**
 * menuPosition.ts - Menu positioning utility & actions for noph-ui Menu in Alhalte Argenterie.
 *
 * Solves Issue #124:
 * HTML Popover API with CSS Anchor Positioning defaults to top-center of the screen
 * when anchor positioning is unsupported, or centers via `@position-try --np-menu-over-anchor`
 * (`align-self: center`, `position-area: span-all`) when Chrome/Blink anchor positioning
 * detects collision or unsupported anchor features.
 *
 * This utility calculates the anchor's viewport bounding rect and applies
 * explicit inline styles with `!important` to completely override Anchor Positioning
 * auto-centering fallbacks and interference:
 *   position: fixed !important;
 *   inset: unset !important;
 *   margin: 0 !important;
 *   position-area: none !important;
 *   position-try-fallbacks: none !important;
 *   align-self: unset !important;
 *   justify-self: unset !important;
 *
 * It binds directly to anchor boundaries:
 *   - bottom placement: `top: ${anchorRect.bottom + offset}px !important;`
 *   - top placement: `bottom: ${(vh - anchorRect.top + offset)}px !important;`
 *   - end placement: `right: ${(vw - anchorRect.right)}px !important;`
 *   - start placement: `left: ${anchorRect.left}px !important;`
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
    const fallbackStyles = [
      'position: fixed !important',
      'inset: unset !important',
      'margin: 0 !important',
      'position-area: none !important',
      'position-try-fallbacks: none !important',
      'align-self: unset !important',
      'justify-self: unset !important',
      `top: ${margin}px !important`,
      `left: ${margin}px !important`,
    ];
    if (options.anchorName) {
      fallbackStyles.push(`position-anchor: ${options.anchorName}`);
    }
    return {
      top: margin,
      left: margin,
      isTop: false,
      styleString: fallbackStyles.join('; ') + ';',
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

  const styleParts: string[] = [
    'position: fixed !important',
    'inset: unset !important',
    'margin: 0 !important',
    'position-area: none !important',
    'position-try-fallbacks: none !important',
    'align-self: unset !important',
    'justify-self: unset !important',
  ];

  if (bottom !== undefined) {
    styleParts.push(`bottom: ${bottom}px !important`);
  }
  if (top !== undefined) {
    styleParts.push(`top: ${top}px !important`);
  }
  if (left !== undefined) {
    styleParts.push(`left: ${left}px !important`);
  }
  if (right !== undefined) {
    styleParts.push(`right: ${right}px !important`);
  }
  if (maxHeight !== undefined) {
    styleParts.push(`max-height: ${maxHeight}px !important`);
    styleParts.push('overflow-y: auto !important');
  }

  if (options.anchorName) {
    styleParts.push(`position-anchor: ${options.anchorName}`);
    styleParts.push(
      `--np-menu-position-area: ${isTop ? 'top span-right' : 'bottom span-right'}`
    );
  }

  return {
    top,
    bottom,
    left,
    right,
    maxHeight,
    isTop,
    styleString: styleParts.join('; ') + ';',
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

    menu.style.setProperty('position', 'fixed', 'important');
    menu.style.setProperty('inset', 'unset', 'important');
    menu.style.setProperty('margin', '0', 'important');
    menu.style.setProperty('position-area', 'none', 'important');
    menu.style.setProperty('position-try-fallbacks', 'none', 'important');
    menu.style.setProperty('align-self', 'unset', 'important');
    menu.style.setProperty('justify-self', 'unset', 'important');

    if (result.top !== undefined) {
      menu.style.setProperty('top', `${result.top}px`, 'important');
      menu.style.removeProperty('bottom');
    }
    if (result.bottom !== undefined) {
      menu.style.setProperty('bottom', `${result.bottom}px`, 'important');
      menu.style.removeProperty('top');
    }
    if (result.left !== undefined) {
      menu.style.setProperty('left', `${result.left}px`, 'important');
      menu.style.removeProperty('right');
    }
    if (result.right !== undefined) {
      menu.style.setProperty('right', `${result.right}px`, 'important');
      menu.style.removeProperty('left');
    }
    if (result.maxHeight !== undefined) {
      menu.style.setProperty('max-height', `${result.maxHeight}px`, 'important');
      menu.style.setProperty('overflow-y', 'auto', 'important');
    }
    if (options.anchorName) {
      menu.style.setProperty('position-anchor', options.anchorName);
      menu.style.setProperty(
        '--np-menu-position-area',
        result.isTop ? 'top span-right' : 'bottom span-right'
      );
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
