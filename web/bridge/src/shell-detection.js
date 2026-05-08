function initShellDetection() {
  try {
    var originalUA = navigator.userAgent;
    Object.defineProperty(Navigator.prototype, "userAgent", {
      get: function () {
        return originalUA + " StremioShell/4.4";
      },
      configurable: true,
    });
  } catch (e) {
    console.warn("[StremioLightning] Could not override userAgent:", e);
  }
}
