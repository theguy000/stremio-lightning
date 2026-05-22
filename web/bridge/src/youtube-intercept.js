(function () {
  "use strict";

  // Prevent SyntaxError from YouTube's www-widgetapi when postMessage is called with targetOrigin "about:blank"
  (function () {
    try {
      var originalPostMessage = Window.prototype.postMessage;
      var customPostMessage = function (message, targetOrigin, transfer) {
        var context = this || window;
        var args = Array.prototype.slice.call(arguments);
        if (args.length >= 2 && typeof args[1] === "string") {
          var lower = args[1].toLowerCase();
          if (lower === "about:blank" || lower.indexOf("about:") === 0 || lower === "") {
            args[1] = "*";
          }
        }
        try {
          return originalPostMessage.apply(context, args);
        } catch (e) {
          if (e.name === "SyntaxError" || (e.message && e.message.indexOf("expected pattern") !== -1)) {
            try {
              if (args.length < 2) {
                args[1] = "*";
              } else {
                args[1] = "*";
              }
              return originalPostMessage.apply(context, args);
            } catch (innerErr) {
              console.warn("[StremioLightning] Suppressed postMessage SyntaxError:", e);
              return;
            }
          }
          throw e;
        }
      };

      // Try direct assignment first
      Window.prototype.postMessage = customPostMessage;
    } catch (err) {
      try {
        Object.defineProperty(Window.prototype, "postMessage", {
          value: customPostMessage,
          writable: true,
          configurable: true
        });
      } catch (defErr) {
        console.warn("[StremioLightning] Failed to override Window.prototype.postMessage:", defErr);
      }
    }
  })();

  function isYoutubeTrailerUrl(url) {
    if (!url) return false;
    var lower = url.toLowerCase();
    return (
      lower.indexOf("youtube.com/embed/") !== -1 ||
      lower.indexOf("youtube-nocookie.com/embed/") !== -1
    );
  }

  function safeAtob(str) {
    try {
      str = str.replace(/=+$/, "");
      while (str.length % 4) {
        str += "=";
      }
      return atob(str);
    } catch (e) {
      return atob(str);
    }
  }

  function extractVideoId(url) {
    if (!url) return "";

    // 1. Check if it has a standard path like youtube.com/embed/<id>
    var pathMatch = url.match(/(?:embed\/|v=)([a-zA-Z0-9_-]{11})/);
    if (pathMatch && pathMatch[1]) {
      return pathMatch[1];
    }

    // 2. Extract and decode player state from forigin parameter
    var stateStr = "";
    var foriginMatch = url.match(/forigin=([^&]+)/);
    if (foriginMatch && foriginMatch[1]) {
      try {
        var decodedForigin = decodeURIComponent(decodeURIComponent(foriginMatch[1]));
        var hashIndex = decodedForigin.indexOf("#/player/");
        if (hashIndex !== -1) {
          stateStr = decodedForigin.substring(hashIndex + 9);
        }
      } catch (e) {
        console.warn("[StremioLightning] Failed to URL-decode forigin parameter:", e);
      }
    }

    // 3. Fallback to location hash
    if (!stateStr && window.location.hash && window.location.hash.indexOf("#/player/") === 0) {
      stateStr = window.location.hash.substring(9);
    }

    if (stateStr) {
      try {
        var binary = safeAtob(stateStr);
        if (binary.length > 7) {
          var jsonStr = binary.substring(7);
          var lastBrace = jsonStr.lastIndexOf("}");
          if (lastBrace !== -1) {
            jsonStr = jsonStr.substring(0, lastBrace + 1);
          }
          var stateObj = JSON.parse(jsonStr);
          if (stateObj && stateObj.ytId) {
            return stateObj.ytId;
          }
        }
      } catch (e) {
        console.warn("[StremioLightning] Failed to decode video ID from serialized state:", e);
      }
    }

    return "";
  }

  var trailerStyleId = "stremio-lightning-trailer-style";
  var activeTrailerInterval = null;

  function enableTrailerTransparency() {
    if (document.getElementById(trailerStyleId)) return;

    var style = document.createElement("style");
    style.id = trailerStyleId;
    style.textContent =
      "html, body, #app, [class*=\"app-\"], [class*=\"detail-\"], [class*=\"modal-\"], [class*=\"route-\"], [class*=\"layout-\"], [class*=\"container-\"], [class*=\"popup-\"], [class*=\"dialog-\"] {\n" +
      "  background: transparent !important;\n" +
      "  background-color: transparent !important;\n" +
      "  background-image: none !important;\n" +
      "}\n" +
      "[class*=\"detail-content\"], [class*=\"detail-meta\"], [class*=\"meta-details\"], [class*=\"detail-container\"] {\n" +
      "  opacity: 0.15 !important;\n" +
      "  transition: opacity 0.3s ease-in-out !important;\n" +
      "}\n" +
      "[class*=\"detail-content\"]:hover, [class*=\"detail-meta\"]:hover, [class*=\"meta-details\"]:hover, [class*=\"detail-container\"]:hover {\n" +
      "  opacity: 0.95 !important;\n" +
      "}\n" +
      "iframe[src=\"about:blank\"] {\n" +
      "  display: none !important;\n" +
      "}\n";
    document.head.appendChild(style);
  }

  function disableTrailerTransparency() {
    var style = document.getElementById(trailerStyleId);
    if (style) {
      style.parentElement.removeChild(style);
    }
  }

  var isTrailerPaused = false;

  function sendMpvProperty(name, value) {
    if (
      window.chrome &&
      window.chrome.webview &&
      typeof window.chrome.webview.postMessage === "function"
    ) {
      window.chrome.webview.postMessage({
        id: 9997,
        type: 6,
        args: ["mpv-set-prop", [name, value]]
      });
    }
  }

  function sendMpvCommand(name, args) {
    if (
      window.chrome &&
      window.chrome.webview &&
      typeof window.chrome.webview.postMessage === "function"
    ) {
      window.chrome.webview.postMessage({
        id: 9999,
        type: 6,
        args: ["mpv-command", [name].concat(args)]
      });
    }
  }

  function handleTrailerKeyDown(e) {
    var activeTag = document.activeElement ? document.activeElement.tagName : "";
    if (activeTag === "INPUT" || activeTag === "TEXTAREA" || activeTag === "SELECT") {
      return;
    }

    if (e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      isTrailerPaused = !isTrailerPaused;
      sendMpvProperty("pause", isTrailerPaused);
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      e.stopPropagation();
      sendMpvCommand("seek", ["-10"]);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      sendMpvCommand("seek", ["10"]);
    }
  }

  function handleTrailerClick(e) {
    // Avoid toggling play/pause if clicking on a button, anchor, or anything inside a button
    var el = e.target;
    while (el && el !== document.body) {
      var tag = el.tagName;
      if (tag === "BUTTON" || tag === "A" || el.onclick || el.getAttribute("role") === "button" || (el.className && el.className.indexOf("close") !== -1)) {
        return;
      }
      el = el.parentElement;
    }

    e.preventDefault();
    e.stopPropagation();
    isTrailerPaused = !isTrailerPaused;
    sendMpvProperty("pause", isTrailerPaused);
  }

  function interceptYoutubeIframe(iframe) {
    if (iframe.__STREMIO_LIGHTNING_INTERCEPTED__) return;

    var src = iframe.src;
    if (isYoutubeTrailerUrl(src)) {
      iframe.__STREMIO_LIGHTNING_INTERCEPTED__ = true;
      console.log("[StremioLightning] Intercepted YouTube trailer embed iframe:", src);

      // Hide or remove the iframe so it doesn't render or execute broken script assets inside Servo
      iframe.style.display = "none";
      iframe.style.visibility = "hidden";
      iframe.src = "about:blank"; // Prevent loading inside the engine

      // Enable transparency so the background MPV video is visible through the webview
      enableTrailerTransparency();

      var videoId = extractVideoId(src);
      var playUrl = videoId
        ? "https://www.youtube.com/watch?v=" + videoId
        : src;

      console.log("[StremioLightning] Routing YouTube trailer to MPV:", playUrl);

      // Trigger the native libmpv backend to play the video link using ytdl
      if (
        window.chrome &&
        window.chrome.webview &&
        typeof window.chrome.webview.postMessage === "function"
      ) {
        window.chrome.webview.postMessage({
          id: 9999, // Specific intercept message ID
          type: 6,  // Shell transport raw message type
          args: ["mpv-command", ["loadfile", playUrl, "replace"]]
        });
      } else {
        console.warn("[StremioLightning] Native shell bridge is not ready to route YouTube trailer");
      }

      // Bind trailer controls key/click listeners
      isTrailerPaused = false;
      window.addEventListener("keydown", handleTrailerKeyDown, true);
      window.addEventListener("click", handleTrailerClick, true);

      // Start polling to detect when the user closes the trailer modal (iframe is removed from DOM)
      if (activeTrailerInterval) {
        clearInterval(activeTrailerInterval);
      }
      activeTrailerInterval = setInterval(function () {
        if (!document.body.contains(iframe)) {
          console.log("[StremioLightning] YouTube trailer iframe removed, stopping playback and restoring UI");
          clearInterval(activeTrailerInterval);
          activeTrailerInterval = null;

          disableTrailerTransparency();

          // Unbind control listeners
          window.removeEventListener("keydown", handleTrailerKeyDown, true);
          window.removeEventListener("click", handleTrailerClick, true);

          // Send stop command to native MPV player
          if (
            window.chrome &&
            window.chrome.webview &&
            typeof window.chrome.webview.postMessage === "function"
          ) {
            window.chrome.webview.postMessage({
              id: 9998,
              type: 6,
              args: ["native-player-stop"]
            });
          }
        }
      }, 250);
    }
  }

  var observer = new MutationObserver(function (mutations) {
    mutations.forEach(function (mutation) {
      if (mutation.type === "attributes" && mutation.target.nodeName === "IFRAME") {
        interceptYoutubeIframe(mutation.target);
      } else if (mutation.addedNodes) {
        for (var i = 0; i < mutation.addedNodes.length; i++) {
          var node = mutation.addedNodes[i];
          if (node.nodeName === "IFRAME") {
            interceptYoutubeIframe(node);
          } else if (node.querySelectorAll) {
            var iframes = node.querySelectorAll("iframe");
            for (var j = 0; j < iframes.length; j++) {
              interceptYoutubeIframe(iframes[j]);
            }
          }
        }
      }
    });
  });

  // Watch document body for dynamically added iframes and attribute changes
  var target = document.body || document.documentElement;
  if (target) {
    observer.observe(target, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["src"]
    });
    console.log("[StremioLightning] YouTube trailer interceptor loaded and observing DOM mutations.");
  } else {
    console.warn("[StremioLightning] YouTube trailer interceptor could not find DOM root.");
  }
})();
