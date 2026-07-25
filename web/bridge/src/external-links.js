function initExternalLinks(ctx) {
  var log = window.StremioLightningLogger.bind("bridge.external-links");

  function openStremioUrl(value) {
    var raw = String(value || "").trim();
    if (
      !/^stremio:\/\//i.test(raw) ||
      /^stremio:\/\/\/?detail\//i.test(raw) ||
      /[\u0000-\u001f\u007f]/.test(raw)
    ) {
      return false;
    }

    try {
      var normalized = raw.replace(/^stremio:\/\//i, "");
      if (!/^https?:\/\//i.test(normalized)) {
        normalized = "https://" + normalized;
      }
      var addonUrl = new URL(normalized);
      if (!addonUrl.hostname || addonUrl.username || addonUrl.password) {
        return false;
      }
      window.location.assign(
        "#/addons?addon=" + encodeURIComponent(addonUrl.href),
      );
      return true;
    } catch (_) {
      return false;
    }
  }

  window.StremioLightningOpenStremioUrl = openStremioUrl;
  if (window.__stremioLightningExternalLinkHandler) {
    document.removeEventListener(
      "click",
      window.__stremioLightningExternalLinkHandler,
      true,
    );
  }
  window.__stremioLightningExternalLinkHandler = function (event) {
    var target = event.target;
    var link = target && target.closest ? target.closest("a[href]") : null;
    if (link && openStremioUrl(link.href)) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  };
  document.addEventListener(
    "click",
    window.__stremioLightningExternalLinkHandler,
    true,
  );

  window.open = function (url) {
    if (openStremioUrl(url)) {
      return null;
    }
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
