function initCastFallback() {
  var castApiAvailabilityCallback = window.__onGCastApiAvailable;
  var castApiUnavailableTimer = null;

  Object.defineProperty(window, "__onGCastApiAvailable", {
    configurable: true,
    enumerable: true,
    get: function () {
      return castApiAvailabilityCallback;
    },
    set: function (callback) {
      castApiAvailabilityCallback = function () {
        if (castApiUnavailableTimer !== null) {
          clearTimeout(castApiUnavailableTimer);
          castApiUnavailableTimer = null;
        }
        return callback.apply(this, arguments);
      };

      castApiUnavailableTimer = setTimeout(function () {
        if (castApiAvailabilityCallback) {
          castApiAvailabilityCallback(false);
        }
      }, 0);
    },
  });
}
