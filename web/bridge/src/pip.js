function initPictureInPicture(ctx) {
  var log = window.StremioLightningLogger.bind("bridge.pip");
  var host = ctx.host;
  var appWindow = ctx.appWindow;
  var pipObserver = null;
  var pipObserverTimeout = null;
  var pipDragActive = false;
  var interactiveTags = ["A", "BUTTON", "INPUT", "LABEL", "SELECT", "TEXTAREA"];
  var interactiveRoles = ["button", "menuitem", "option", "slider", "tab", "textbox"];
  var interactiveClassNames = [
    "control-bar",
    "button",
    "slider",
    "seek",
    "volume",
  ];

  function setPipButtonTitle(title) {
    var btn = document.getElementById("sl-pip-btn");
    if (btn) btn.title = title;
  }

  function injectPipButton() {
    if (document.getElementById("sl-pip-btn")) return "present";

    var containers = document.querySelectorAll(
      '[class*="control-bar-buttons-container"]',
    );
    if (!containers.length) return "controls-missing";

    var btnContainer = containers[containers.length - 1];
    var btn = document.createElement("button");
    btn.id = "sl-pip-btn";
    btn.title = "Picture in Picture";
    btn.setAttribute("tabindex", "-1");
    btn.innerHTML =
      '<svg viewBox="0 0 24 24" style="width:3rem;height:2rem;fill:rgba(255,255,255,0.85);">' +
      '<path d="M19 11h-8v6h8v-6zm4 8V4.98C23 3.88 22.1 3 21 3H3c-1.1 0-2 .88-2 1.98V19c0 1.1.9 2 2 2h18c1.1 0 2-.9 2-2zm-2 .02H3V4.97h18v14.05z"/>' +
      "</svg>";

    btn.style.cssText =
      "flex:none;width:4rem;height:4rem;display:flex;justify-content:center;align-items:center;" +
      "background:transparent;border:none;cursor:pointer;padding:0;outline:none;";

    btn.addEventListener("mouseenter", function () {
      btn.querySelector("svg").style.fill = "rgba(255,255,255,1)";
    });
    btn.addEventListener("mouseleave", function () {
      btn.querySelector("svg").style.fill = "rgba(255,255,255,0.85)";
    });
    btn.addEventListener("click", function () {
      host.invoke("toggle_pip").catch(function (err) {
        log.error("[StremioLightning] PiP toggle failed:", err);
      });
      btn.blur();
    });

    var volumeSlider = btnContainer.querySelector('[class*="volume-slider"]');
    var spacing = btnContainer.querySelector('[class*="spacing"]');
    if (volumeSlider) {
      btnContainer.insertBefore(btn, volumeSlider.nextSibling);
    } else if (spacing) {
      btnContainer.insertBefore(btn, spacing);
    } else {
      btnContainer.appendChild(btn);
    }
    return "injected";
  }

  function removePipButton() {
    var btn = document.getElementById("sl-pip-btn");
    if (btn) btn.remove();
  }

  function stopPipObserver() {
    if (pipObserver) {
      pipObserver.disconnect();
      pipObserver = null;
    }
    if (pipObserverTimeout) {
      clearTimeout(pipObserverTimeout);
      pipObserverTimeout = null;
    }
  }

  function observePipButton() {
    if (!document.body) return;

    stopPipObserver();

    pipObserver = new MutationObserver(function () {
      if (!ctx.pipFeatureOn) return;
      if (!isPlayerRoute()) {
        stopPipObserver();
        removePipButton();
        return;
      }

      var result = injectPipButton();
      if (result === "injected") {
        log.info("[StremioLightning] PiP button restored after controls changed");
      }
      observePipButton();
    });

    var btn = document.getElementById("sl-pip-btn");
    var controls = btn && btn.parentElement;
    if (!controls) {
      pipObserver.observe(document.body, { childList: true, subtree: true });
      pipObserverTimeout = setTimeout(stopPipObserver, 30000);
      return;
    }

    var node = controls;
    while (node) {
      pipObserver.observe(node, { childList: true });
      if (node === document.body) break;
      node = node.parentElement;
    }
  }

  function updatePipButton() {
    if (!ctx.pipFeatureOn) {
      stopPipObserver();
      removePipButton();
      return;
    }

    if (!isPlayerRoute()) {
      stopPipObserver();
      removePipButton();
      return;
    }

    var result = injectPipButton();
    observePipButton();
    log.info("[StremioLightning] PiP button update: " + result);
  }

  function isInteractiveNode(el) {
    if (interactiveTags.indexOf(el.tagName) !== -1) {
      return true;
    }
    if (el.isContentEditable) return true;

    var role = el.getAttribute && el.getAttribute("role");
    if (interactiveRoles.indexOf(role) !== -1) {
      return true;
    }

    if (el.className && typeof el.className === "string") {
      var cls = el.className;
      for (var i = 0; i < interactiveClassNames.length; i++) {
        if (cls.indexOf(interactiveClassNames[i]) !== -1) return true;
      }
    }
    return false;
  }

  function isInsideInteractive(el) {
    while (el && el !== document.body && el !== document.documentElement) {
      if (isInteractiveNode(el)) return true;
      el = el.parentElement;
    }
    return false;
  }

  window.addEventListener("hashchange", updatePipButton);
  onDomReady(updatePipButton);

  document.addEventListener("sl-pip-feature-changed", function (e) {
    ctx.pipFeatureOn = e.detail !== false;
    updatePipButton();
  });

  document.addEventListener("sl-pip-enabled", function () {
    setPipButtonTitle("Exit Picture in Picture");
    pipDragActive = true;
  });
  document.addEventListener("sl-pip-disabled", function () {
    setPipButtonTitle("Picture in Picture");
    pipDragActive = false;
  });

  document.addEventListener(
    "mousedown",
    function (e) {
      if (!pipDragActive) return;
      if (e.button !== 0) return;
      if (isInsideInteractive(e.target)) return;
      e.stopImmediatePropagation();
      e.preventDefault();
      appWindow.startDragging();
    },
    true,
  );
}
