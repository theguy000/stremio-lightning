function initShortcuts(ctx) {
  var host = ctx.host;
  var appWindow = ctx.appWindow;
  var webview = ctx.webview;
  var zoomLevel = 1.0;

  function toggleFullscreen() {
    appWindow.isFullscreen().then(function (fs) {
      appWindow.setFullscreen(!fs);
    });
  }

  document.addEventListener(
    "click",
    function (e) {
      var el = e.target;
      for (var i = 0; i < 5 && el && el !== document; i++) {
        var title = el.getAttribute && el.getAttribute("title");
        if (
          title &&
          (title.indexOf("fullscreen") !== -1 ||
            title.indexOf("Fullscreen") !== -1)
        ) {
          e.preventDefault();
          e.stopPropagation();
          toggleFullscreen();
          return;
        }
        el = el.parentElement;
      }
    },
    true,
  );

  document.addEventListener("keydown", function (e) {
    if (e.key === "F11") {
      e.preventDefault();
      toggleFullscreen();
      return;
    }

    if (e.key === "f" && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
      var tag = document.activeElement ? document.activeElement.tagName : "";
      var isInput =
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        (document.activeElement && document.activeElement.isContentEditable);
      if (!isInput) {
        e.preventDefault();
        toggleFullscreen();
        return;
      }
    }

    if (!e.ctrlKey) return;

    if (e.shiftKey && (e.key === "I" || e.key === "i")) {
      e.preventDefault();
      host.invoke("toggle_devtools");
      return;
    }

    if (e.shiftKey && (e.key === "P" || e.key === "p")) {
      if (isPlayerRoute() && ctx.pipFeatureOn) {
        e.preventDefault();
        host.invoke("toggle_pip").catch(function (err) {
          console.error("[StremioLightning] PiP toggle failed:", err);
        });
      }
      return;
    }

    if (!e.shiftKey && (e.key === "r" || e.key === "R")) {
      e.preventDefault();
      window.location.reload();
      return;
    }

    if (e.key === "+" || e.key === "=") {
      e.preventDefault();
      zoomLevel = Math.min(zoomLevel + 0.1, 3.0);
      webview.setZoom(zoomLevel);
      return;
    }

    if (e.key === "-") {
      e.preventDefault();
      zoomLevel = Math.max(zoomLevel - 0.1, 0.5);
      webview.setZoom(zoomLevel);
    }
  });
}
