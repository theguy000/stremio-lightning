function initExternalLinks(ctx) {
  window.open = function (url) {
    if (url) {
      ctx.host.invoke("open_external_url", { url: String(url) }).catch(function (e) {
        console.error(
          "[StremioLightning] Failed to open external URL:",
          url,
          e,
        );
      });
    }
    return null;
  };
}
