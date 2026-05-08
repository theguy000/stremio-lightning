// Stremio Lightning - Frontend Bridge & Keyboard Shortcuts
// Injected by the native Rust shell crates on every page load.

var host = window.StremioLightningHost || null;

if (!host) {
  console.error(
    "[StremioLightning] host adapter not available - bridge not loaded",
  );
} else {
  var ctx = {
    host: host,
    appWindow: host.window,
    webview: host.webview,
    pipFeatureOn: localStorage.getItem("sl-pip-feature") !== "false",
    shellTransport: null,
    initDiscordRpc: function () {},
    initUpdateChecker: function () {},
  };

  console.info("[StremioLightning] Native player mode enabled (libmpv transport)");

  initCastFallback();
  ctx.shellTransport = initShellTransport(ctx);

  window.StremioEnhancedAPI = window.StremioEnhancedAPI || {};

  initExternalLinks(ctx);
  initShellDetection();
  initBackButton();
  initShortcuts(ctx);
  initPictureInPicture(ctx);
  initDiscordRpcTracker(ctx);
  initUpdateBanner(ctx);

  onWindowLoad(function () {
    ctx.initDiscordRpc();
    ctx.initUpdateChecker();
  });
}
