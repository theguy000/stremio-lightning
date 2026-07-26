function initShortcuts(ctx) {
  var log = window.StremioLightningLogger.bind("bridge.shortcuts");
  var host = ctx.host;
  var appWindow = ctx.appWindow;
  var webview = ctx.webview;
  var zoomLevel = 1.0;
  var spaceHoldTimer = null;
  var spaceHeld = false;
  var fastForwarding = false;
  var speedBeforeHold = 1;
  var pauseBeforeHold = false;

  function isEditableTarget(target) {
    var tag = target && target.tagName;
    return (
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      (target && target.isContentEditable)
    );
  }

  function speedHint() {
    var hint = document.getElementById("sl-speed-hint");
    if (hint) return hint;

    hint = document.createElement("div");
    hint.id = "sl-speed-hint";
    hint.hidden = true;
    hint.setAttribute("role", "status");
    hint.setAttribute("aria-live", "polite");
    hint.innerHTML =
      '<span class="sl-speed-hint-value">2x</span>' +
      '<svg class="sl-speed-hint-icon" viewBox="0 0 15 8" aria-hidden="true">' +
      '<path fill="currentColor" d="M0 0v8l6-4zm5 0v8l6-4z"/></svg>';
    (document.body || document.documentElement).appendChild(hint);
    return hint;
  }

  function finishSpaceHold(togglePause) {
    if (!spaceHeld) return;

    clearTimeout(spaceHoldTimer);
    spaceHoldTimer = null;
    spaceHeld = false;

    if (fastForwarding) {
      ctx.shellTransport.setMpvProperty("speed", speedBeforeHold);
      if (pauseBeforeHold) {
        ctx.shellTransport.setMpvProperty("pause", true);
      }
      var hint = document.getElementById("sl-speed-hint");
      if (hint) hint.hidden = true;
      fastForwarding = false;
    } else if (togglePause) {
      ctx.shellTransport.setMpvProperty("pause", !pauseBeforeHold);
    }
  }

  window.addEventListener(
    "keydown",
    function (e) {
      if (
        e.code !== "Space" ||
        e.ctrlKey ||
        e.altKey ||
        e.metaKey ||
        e.shiftKey ||
        !isPlayerRoute() ||
        isEditableTarget(e.target)
      ) {
        return;
      }

      e.preventDefault();
      e.stopImmediatePropagation();
      if (spaceHeld) return;

      spaceHeld = true;
      pauseBeforeHold = ctx.shellTransport.mpvState.pause;
      spaceHoldTimer = setTimeout(function () {
        if (!spaceHeld) return;
        speedBeforeHold = ctx.shellTransport.mpvState.speed || 1;
        fastForwarding = true;
        if (pauseBeforeHold) {
          ctx.shellTransport.setMpvProperty("pause", false);
        }
        ctx.shellTransport.setMpvProperty("speed", 2);
        speedHint().hidden = false;
      }, 350);
    },
    true,
  );

  window.addEventListener(
    "keyup",
    function (e) {
      if (e.code !== "Space" || !spaceHeld) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      finishSpaceHold(true);
    },
    true,
  );

  window.addEventListener("blur", function () {
    finishSpaceHold(false);
  });

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
      if (!isEditableTarget(document.activeElement)) {
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
          log.error("[StremioLightning] PiP toggle failed:", err);
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
