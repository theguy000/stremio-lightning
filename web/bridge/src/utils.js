// Shared helpers for the injected bridge modules.

function onDomReady(callback) {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", callback, { once: true });
  } else {
    callback();
  }
}

function onWindowLoad(callback) {
  if (document.readyState === "complete") {
    callback();
  } else {
    window.addEventListener("load", callback, { once: true });
  }
}

function isPlayerRoute() {
  var hash = window.location.hash || "";
  return hash.indexOf("/player") !== -1;
}

function toFiniteNumber(value) {
  if (typeof value === "number" && isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    var parsed = Number(value);
    if (isFinite(parsed)) return parsed;
  }
  return 0;
}

var _evalCounter = 0;

function evalInPageContext(js) {
  return new Promise(function (resolve, reject) {
    try {
      var eventName = "sl-eval-" + ++_evalCounter + "-" + Date.now();
      var script = document.createElement("script");

      window.addEventListener(
        eventName,
        function handler(e) {
          script.remove();
          resolve(e.detail);
        },
        { once: true },
      );

      script.textContent =
        "(function() {" +
        "  try {" +
        "    var core = window.services && window.services.core;" +
        '    if (!core) { window.dispatchEvent(new CustomEvent("' +
        eventName +
        '", { detail: null })); return; }' +
        "    var result = " +
        js +
        ";" +
        '    if (result && typeof result.then === "function") {' +
        '      result.then(function(r) { window.dispatchEvent(new CustomEvent("' +
        eventName +
        '", { detail: r })); })' +
        '            .catch(function() { window.dispatchEvent(new CustomEvent("' +
        eventName +
        '", { detail: null })); });' +
        "    } else {" +
        '      window.dispatchEvent(new CustomEvent("' +
        eventName +
        '", { detail: result }));' +
        "    }" +
        "  } catch(err) {" +
        '    window.dispatchEvent(new CustomEvent("' +
        eventName +
        '", { detail: null }));' +
        "  }" +
        "})();";

      document.head.appendChild(script);
      setTimeout(function () {
        if (script.parentElement) {
          script.remove();
          resolve(null);
        }
      }, 10000);
    } catch (err) {
      reject(err);
    }
  });
}
