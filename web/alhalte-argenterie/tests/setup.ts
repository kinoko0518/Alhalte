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
}
