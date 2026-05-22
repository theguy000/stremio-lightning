(function () {
  "use strict";

  function isYoutubeTrailerUrl(url) {
    if (!url) return false;
    var lower = url.toLowerCase();
    return (
      lower.indexOf("youtube.com/embed/") !== -1 ||
      lower.indexOf("youtube-nocookie.com/embed/") !== -1
    );
  }

  function interceptYoutubeIframe(iframe) {
    var src = iframe.src;
    if (isYoutubeTrailerUrl(src)) {
      console.log("[StremioLightning] Intercepted YouTube trailer embed iframe:", src);

      // Hide or remove the iframe so it doesn't render or execute broken script assets inside Servo
      iframe.style.display = "none";
      iframe.style.visibility = "hidden";
      iframe.src = "about:blank"; // Prevent loading inside the engine

      // Map the YouTube embed URL back to a standard YouTube play link
      // e.g. https://www.youtube.com/embed/dQw4w9WgXcQ -> https://www.youtube.com/watch?v=dQw4w9WgXcQ
      var videoId = "";
      var match = src.match(/(?:embed\/|v=)([a-zA-Z0-9_-]{11})/);
      if (match && match[1]) {
        videoId = match[1];
      }

      var playUrl = videoId
        ? "https://www.youtube.com/watch?v=" + videoId
        : src;

      // Trigger the native libmpv backend to play the video link using ytdl
      if (
        window.chrome &&
        window.chrome.webview &&
        typeof window.chrome.webview.postMessage === "function"
      ) {
        window.chrome.webview.postMessage({
          id: 9999, // Specific intercept message ID
          type: 6,  // Shell transport raw message type
          args: ["mpv-command", "loadfile", playUrl, "replace"]
        });
      } else {
        console.warn("[StremioLightning] Native shell bridge is not ready to route YouTube trailer");
      }
    }
  }

  var observer = new MutationObserver(function (mutations) {
    mutations.forEach(function (mutation) {
      if (mutation.addedNodes) {
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

  // Watch document body for dynamically added iframes
  var target = document.body || document.documentElement;
  if (target) {
    observer.observe(target, {
      childList: true,
      subtree: true
    });
    console.log("[StremioLightning] YouTube trailer interceptor loaded and observing DOM mutations.");
  } else {
    console.warn("[StremioLightning] YouTube trailer interceptor could not find DOM root.");
  }
})();
