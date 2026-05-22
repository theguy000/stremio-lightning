(function () {
  "use strict";

  var lastNonPlayerHash = "";
  if (window.location.hash && window.location.hash.indexOf("#/player/") === -1) {
    lastNonPlayerHash = window.location.hash;
  }
  window.addEventListener("hashchange", function () {
    if (window.location.hash && window.location.hash.indexOf("#/player/") === -1) {
      lastNonPlayerHash = window.location.hash;
    }
  });

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

  function interceptYoutubeIframe(iframe) {
    if (iframe.__STREMIO_LIGHTNING_INTERCEPTED__) return;

    var src = iframe.src;
    if (isYoutubeTrailerUrl(src)) {
      iframe.__STREMIO_LIGHTNING_INTERCEPTED__ = true;
      console.log("[StremioLightning] Intercepted YouTube trailer embed iframe, opening in external browser:", src);

      // Hide the iframe immediately and navigate it to about:blank to prevent resource/script execution in the engine
      iframe.style.display = "none";
      iframe.style.visibility = "hidden";
      iframe.src = "about:blank";

      // 1. Dismiss Stremio's player/trailer modal UI overlay/route by reverting hash or navigating back in history
      try {
        if (lastNonPlayerHash) {
          window.location.hash = lastNonPlayerHash;
        } else if (window.history && typeof window.history.back === "function") {
          window.history.back();
        } else {
          var closeBtn = document.querySelector(".close, [class*=\"close\"]");
          if (closeBtn) {
            closeBtn.click();
          }
        }
      } catch (err) {
        console.warn("[StremioLightning] Failed to dismiss player overlay:", err);
      }


      // 2. Determine play URL (standard watch page instead of embed link)
      var videoId = extractVideoId(src);
      var playUrl = videoId
        ? "https://www.youtube.com/watch?v=" + videoId
        : src;



      // 3. Request the host to open the URL in the default system browser
      var host = window.StremioLightningHost;
      if (host && typeof host.invoke === "function") {
        host.invoke("open_external_url", { url: playUrl }).catch(function (err) {
          console.error("[StremioLightning] Failed to open YouTube trailer URL in external browser:", err);
        });
      } else {
        console.warn("[StremioLightning] Native host bridge not ready to open trailer URL:", playUrl);
      }
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

  var target = document.body || document.documentElement;
  if (target) {
    observer.observe(target, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["src"]
    });
    console.log("[StremioLightning] YouTube trailer browser interceptor loaded.");
  }
})();
