(function () {
  "use strict";

  /*
   * Servo Engine Polyfills — Stremio Lightning
   *
   * Lightweight W3C-compatible polyfills for modern DOM APIs that Stremio Web's
   * Svelte components depend on but Servo's SpiderMonkey engine may not yet support.
   *
   * Injected at document_start before Stremio Web scripts execute, only when
   * the rendering engine is Servo.
   */

  // ── IntersectionObserver Polyfill ───────────────────────────────────
  // Stremio Web uses IntersectionObserver for lazy-loading poster thumbnail
  // images during scrolling. Without it, Svelte scripts throw and halt.
  // This stub immediately fires the callback marking all targets as
  // intersecting, which bypasses lazy-loading and ensures posters render.
  if (!("IntersectionObserver" in window)) {
    window.IntersectionObserver = (function () {
      function IntersectionObserver(callback, options) {
        this._callback = callback;
        this._options = options || {};
        this._targets = [];
      }

      IntersectionObserver.prototype.observe = function (element) {
        if (!element) return;
        this._targets.push(element);
        var self = this;
        // Defer to allow the element to be fully attached to the DOM
        Promise.resolve().then(function () {
          self._callback(
            [
              {
                isIntersecting: true,
                intersectionRatio: 1.0,
                target: element,
                boundingClientRect: element.getBoundingClientRect
                  ? element.getBoundingClientRect()
                  : { top: 0, left: 0, bottom: 0, right: 0, width: 0, height: 0 },
                intersectionRect: element.getBoundingClientRect
                  ? element.getBoundingClientRect()
                  : { top: 0, left: 0, bottom: 0, right: 0, width: 0, height: 0 },
                rootBounds: null,
                time: Date.now(),
              },
            ],
            self
          );
        });
      };

      IntersectionObserver.prototype.unobserve = function (element) {
        this._targets = this._targets.filter(function (t) {
          return t !== element;
        });
      };

      IntersectionObserver.prototype.disconnect = function () {
        this._targets = [];
      };

      IntersectionObserver.prototype.takeRecords = function () {
        return [];
      };

      return IntersectionObserver;
    })();

    console.log("[StremioLightning] IntersectionObserver polyfill installed for Servo engine.");
  }

  // ── ResizeObserver Polyfill ─────────────────────────────────────────
  // Some Svelte components use ResizeObserver for responsive layout.
  // This stub fires the callback once on observe with the element's current size.
  if (!("ResizeObserver" in window)) {
    window.ResizeObserver = (function () {
      function ResizeObserver(callback) {
        this._callback = callback;
        this._targets = [];
      }

      ResizeObserver.prototype.observe = function (element) {
        if (!element) return;
        this._targets.push(element);
        var self = this;
        Promise.resolve().then(function () {
          var rect = element.getBoundingClientRect
            ? element.getBoundingClientRect()
            : { width: 0, height: 0 };
          self._callback(
            [
              {
                target: element,
                contentRect: rect,
                borderBoxSize: [{ inlineSize: rect.width, blockSize: rect.height }],
                contentBoxSize: [{ inlineSize: rect.width, blockSize: rect.height }],
              },
            ],
            self
          );
        });
      };

      ResizeObserver.prototype.unobserve = function (element) {
        this._targets = this._targets.filter(function (t) {
          return t !== element;
        });
      };

      ResizeObserver.prototype.disconnect = function () {
        this._targets = [];
      };

      return ResizeObserver;
    })();

    console.log("[StremioLightning] ResizeObserver polyfill installed for Servo engine.");
  }

  // ── requestIdleCallback Polyfill ───────────────────────────────────
  // Fallback using setTimeout for engines that don't support idle callbacks.
  if (!("requestIdleCallback" in window)) {
    window.requestIdleCallback = function (callback, options) {
      var timeout = (options && options.timeout) || 50;
      var start = Date.now();
      return setTimeout(function () {
        callback({
          didTimeout: false,
          timeRemaining: function () {
            return Math.max(0, timeout - (Date.now() - start));
          },
        });
      }, 1);
    };

    window.cancelIdleCallback = function (id) {
      clearTimeout(id);
    };

    console.log("[StremioLightning] requestIdleCallback polyfill installed for Servo engine.");
  }

  // ── CSS.supports Polyfill ──────────────────────────────────────────
  // Minimal stub that returns false for unknown features, preventing
  // runtime errors in feature-detection code.
  if (!window.CSS || !window.CSS.supports) {
    if (!window.CSS) {
      window.CSS = {};
    }
    window.CSS.supports = function () {
      return false;
    };

    console.log("[StremioLightning] CSS.supports polyfill installed for Servo engine.");
  }
})();
