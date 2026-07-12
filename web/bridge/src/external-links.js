function initExternalLinks(ctx) {
  var log = window.StremioLightningLogger.bind("bridge.external-links");
  window.open = function (url) {
    if (url) {
      ctx.host.invoke("open_external_url", { url: String(url) }).catch(function (e) {
        log.error(
          "[StremioLightning] Failed to open external URL:",
          e,
        );
      });
    }
    return null;
  };
}
