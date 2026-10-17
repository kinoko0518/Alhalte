/**
 * Vitest setup file for jsdom environment.
 */

if (typeof window !== 'undefined') {
  // Mock window.matchMedia
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });

  // Mock Element.prototype.animate for Web Animations API (used by Noph UI ripple)
  if (typeof Element !== 'undefined' && !Element.prototype.animate) {
    Element.prototype.animate = () =>
      ({
        finished: Promise.resolve(),
        cancel: () => {},
        play: () => {},
        pause: () => {},
        reverse: () => {},
        finish: () => {},
        addEventListener: () => {},
        removeEventListener: () => {},
      }) as unknown as Animation;
  }

  // Mock HTMLDialogElement methods (used by Noph UI Sheet / Dialog)
  if (typeof HTMLDialogElement !== 'undefined') {
    if (!HTMLDialogElement.prototype.show) {
      HTMLDialogElement.prototype.show = function () {
        this.open = true;
      };
    }
    if (!HTMLDialogElement.prototype.showModal) {
      HTMLDialogElement.prototype.showModal = function () {
        this.open = true;
      };
    }
    if (!HTMLDialogElement.prototype.close) {
      HTMLDialogElement.prototype.close = function () {
        this.open = false;
      };
    }
  }

  // Mock Element pointer capture methods
  if (typeof Element !== 'undefined') {
    if (!Element.prototype.setPointerCapture) {
      Element.prototype.setPointerCapture = () => {};
    }
    if (!Element.prototype.releasePointerCapture) {
      Element.prototype.releasePointerCapture = () => {};
    }
    if (!Element.prototype.hasPointerCapture) {
      Element.prototype.hasPointerCapture = () => false;
    }
  }

  // Polyfill PointerEvent for jsdom
  if (typeof globalThis !== 'undefined' && !globalThis.PointerEvent) {
    // @ts-expect-error fallback PointerEvent for jsdom
    const PolyfilledPointerEvent = class PointerEvent extends (globalThis.MouseEvent || Object) {
      pointerId: number;
      width: number;
      height: number;
      pressure: number;
      tangentialPressure: number;
      tiltX: number;
      tiltY: number;
      twist: number;
      pointerType: string;
      isPrimary: boolean;

      constructor(type: string, params: PointerEventInit = {}) {
        super(type, params);
        this.pointerId = params.pointerId ?? 0;
        this.width = params.width ?? 1;
        this.height = params.height ?? 1;
        this.pressure = params.pressure ?? 0;
        this.tangentialPressure = params.tangentialPressure ?? 0;
        this.tiltX = params.tiltX ?? 0;
        this.tiltY = params.tiltY ?? 0;
        this.twist = params.twist ?? 0;
        this.pointerType = params.pointerType ?? 'mouse';
        this.isPrimary = params.isPrimary ?? false;
      }
    };
    // @ts-expect-error fallback PointerEvent for jsdom
    globalThis.PointerEvent = PolyfilledPointerEvent;
    if (typeof window !== 'undefined') {
      // @ts-expect-error fallback PointerEvent for jsdom
      window.PointerEvent = PolyfilledPointerEvent;
    }
  }

  // Polyfill DragEvent for jsdom
  if (typeof globalThis !== 'undefined' && !globalThis.DragEvent) {
    // @ts-expect-error fallback DragEvent for jsdom
    const PolyfilledDragEvent = class DragEvent extends (globalThis.MouseEvent || Object) {
      dataTransfer: DataTransfer | null;
      constructor(type: string, params: MouseEventInit = {}) {
        super(type, params);
        this.dataTransfer = null;
      }
    };
    // @ts-expect-error fallback DragEvent for jsdom
    globalThis.DragEvent = PolyfilledDragEvent;
    if (typeof window !== 'undefined') {
      // @ts-expect-error fallback DragEvent for jsdom
      window.DragEvent = PolyfilledDragEvent;
    }
  }

  // Mock ResizeObserver for jsdom (used by Svelte bind:clientWidth / bind:clientHeight in TextField)
  if (typeof globalThis !== 'undefined' && !globalThis.ResizeObserver) {
    class PolyfilledResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    // @ts-expect-error fallback ResizeObserver for jsdom
    globalThis.ResizeObserver = PolyfilledResizeObserver;
    if (typeof window !== 'undefined') {
      // @ts-expect-error fallback ResizeObserver for jsdom
      window.ResizeObserver = PolyfilledResizeObserver;
    }
  }
}

