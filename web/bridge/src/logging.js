// This bootstrap is injected before every bridge module so early diagnostics remain available.
(function () {
  "use strict";

  var originalConsole = {};
  ["debug", "info", "warn", "error"].forEach(function (level) {
    originalConsole[level] = console[level];
  });

  var entries = [];
  var listeners = [];
  var nextId = 1;
  var capacity = 2000;
  var maxCollectionItems = 100;
  var maxDepth = 6;
  var maxMessageLength = 16384;
  var maxSourceLength = 256;

  function truncate(value, maxLength) {
    var suffix = "... [truncated]";
    return value.length <= maxLength
      ? value
      : value.slice(0, maxLength - suffix.length) + suffix;
  }

  function redactSensitiveDetails(value) {
    return value
      .replace(/\b(?:https?|file):\/\/[^\s"'<>]+/gi, "[redacted URL]")
      .replace(/\bmagnet:\?[^\s"'<>]+/gi, "[redacted URL]")
      .replace(
        /\b(authorization|cookie|set-cookie|token|access[_-]?token|refresh[_-]?token|api[_-]?key|password|secret)\b(\s*[:=]\s*)[^}\]\r\n]+/gi,
        "$1$2[redacted]",
      );
  }

  function format(value, seen, depth) {
    if (value instanceof Error) {
      try {
        return value.stack || value.name + ": " + value.message;
      } catch (_) {
        return "[Error]";
      }
    }
    if (typeof value === "string") return value;
    if (value === null || value === undefined) return String(value);
    if (typeof value === "bigint") return String(value) + "n";
    if (typeof value !== "object") {
      try {
        return String(value);
      } catch (_) {
        return "[Unserializable]";
      }
    }
    if (depth >= maxDepth) return "[Max depth]";

    seen = seen || [];
    if (seen.indexOf(value) !== -1) return "[Circular]";
    seen.push(value);
    try {
      if (Array.isArray(value)) {
        var items = value.slice(0, maxCollectionItems).map(function (item) {
          return format(item, seen, depth + 1);
        });
        if (value.length > maxCollectionItems) items.push("... [truncated]");
        return "[" + items.join(", ") + "]";
      }
      var keys = Object.keys(value);
      var properties = keys.slice(0, maxCollectionItems).map(function (key) {
        var item;
        try {
          item = format(value[key], seen, depth + 1);
        } catch (_) {
          item = "[Unserializable]";
        }
        return key + ": " + item;
      });
      if (keys.length > maxCollectionItems) properties.push("... [truncated]");
      return "{" + properties.join(", ") + "}";
    } catch (_) {
      try {
        return String(value);
      } catch (_) {
        return "[Unserializable]";
      }
    } finally {
      seen.pop();
    }
  }

  function emit(level, source, values) {
    var formattedValues = values.slice(0, maxCollectionItems).map(function (value) {
      return format(value, null, 0);
    });
    if (values.length > maxCollectionItems) formattedValues.push("... [truncated]");
    var entry = {
      id: nextId++,
      timestamp: Date.now(),
      level: level,
      source: truncate(String(source), maxSourceLength),
      message: truncate(
        redactSensitiveDetails(formattedValues.join(" ")),
        maxMessageLength,
      ),
    };
    entries.push(entry);
    if (entries.length > capacity) entries.shift();
    listeners.slice().forEach(function (listener) {
      try {
        listener(entry);
      } catch (_) {}
    });
    var original = originalConsole[level];
    if (typeof original === "function") original.apply(console, values);
    return entry;
  }

  var logger = {
    debug: function (source) { return emit("debug", source, Array.prototype.slice.call(arguments, 1)); },
    info: function (source) { return emit("info", source, Array.prototype.slice.call(arguments, 1)); },
    warn: function (source) { return emit("warn", source, Array.prototype.slice.call(arguments, 1)); },
    error: function (source) { return emit("error", source, Array.prototype.slice.call(arguments, 1)); },
    bind: function (source) {
      return {
        debug: function () { return logger.debug.apply(logger, [source].concat(Array.prototype.slice.call(arguments))); },
        info: function () { return logger.info.apply(logger, [source].concat(Array.prototype.slice.call(arguments))); },
        warn: function () { return logger.warn.apply(logger, [source].concat(Array.prototype.slice.call(arguments))); },
        error: function () { return logger.error.apply(logger, [source].concat(Array.prototype.slice.call(arguments))); },
      };
    },
    entries: function () { return entries.slice(); },
    subscribe: function (listener) {
      listeners.push(listener);
      try {
        listener(null, entries.slice());
      } catch (_) {}
      return function () {
        listeners = listeners.filter(function (item) { return item !== listener; });
      };
    },
  };

  window.StremioLightningLogger = logger;

  var nextNetworkRequestId = 1;
  var nextAddonSourceId = 1;
  var addonSourceIds = Object.create(null);
  var streamRequestStallThresholdMs = 15000;

  function requestUrl(input) {
    if (typeof input === "string") return input;
    if (input && typeof input.url === "string") return input.url;
    if (input && typeof input.href === "string") return input.href;
    return null;
  }

  function classifyRequest(input) {
    var rawUrl = requestUrl(input);
    if (!rawUrl) return null;

    var parsedUrl;
    try {
      parsedUrl = new URL(rawUrl, window.location.href);
    } catch (_) {
      return null;
    }
    var match = parsedUrl.pathname.toLowerCase().match(
      /\/(manifest|catalog|meta|stream|subtitles)(?:\/([^/.?#]+))?(?:\/|\.json|$)/,
    );
    if (!match) return null;
    var kind = {
      manifest: "addon manifest",
      catalog: "catalog",
      meta: "metadata",
      stream: "stream discovery",
      subtitles: "subtitles",
    }[match[1]];
    var mediaTypes = {
      anime: "anime",
      channel: "channel",
      movie: "movie",
      series: "series",
      tv: "TV",
    };
    var source = parsedUrl.protocol + "//" + parsedUrl.host;
    if (!addonSourceIds[source]) addonSourceIds[source] = nextAddonSourceId++;
    return {
      kind: kind,
      mediaType: mediaTypes[match[2]] || null,
      sourceId: addonSourceIds[source],
    };
  }

  function classifyStreamRequest(input) {
    var rawUrl = requestUrl(input);
    if (!rawUrl || !/(?:^|\/)stream(?:\/|\.json|[?#]|$)/i.test(rawUrl)) {
      return null;
    }
    var classification = classifyRequest(rawUrl);
    return classification && classification.kind === "stream discovery"
      ? classification
      : null;
  }

  function networkStatus(status, statusText) {
    return status
      ? String(status) + (statusText ? " " + statusText : "")
      : "network error";
  }

  function sanitizeMethod(method) {
    method = typeof method === "string" ? method.toUpperCase() : "GET";
    return /^[A-Z]{1,16}$/.test(method) ? method : "UNKNOWN";
  }

  function createNetworkRequest(input, method, transport) {
    var classification = classifyStreamRequest(input);
    var request = {
      id: classification ? nextNetworkRequestId++ : null,
      input: input,
      classification: classification,
      method: method,
      startedAt: Date.now(),
      stallTimer: null,
      transport: transport,
    };
    if (classification) {
      logNetworkStarted(request);
      request.stallTimer = window.setTimeout(function () {
        logNetworkStalled(request);
      }, streamRequestStallThresholdMs);
    }
    return request;
  }

  function networkRequestSummary(request, classification) {
    return sanitizeMethod(request.method) + " " + classification.kind +
      (classification.mediaType ? " (" + classification.mediaType + ")" : "") +
      " via " + request.transport + " from addon #" + classification.sourceId;
  }

  function logNetworkStarted(request) {
    var currentLogger = window.StremioLightningLogger;
    if (!currentLogger || !request.classification) return;
    currentLogger.info(
      "bridge.network",
      "[StremioLightning] Stream request #" + request.id + " started:",
      networkRequestSummary(request, request.classification),
      "(request details redacted)",
    );
  }

  function logNetworkStalled(request) {
    var currentLogger = window.StremioLightningLogger;
    if (!currentLogger || !request.classification) return;
    currentLogger.warn(
      "bridge.network",
      "[StremioLightning] Stream request #" + request.id +
        " is still pending after " + streamRequestStallThresholdMs + " ms:",
      networkRequestSummary(request, request.classification),
      "(request details redacted)",
    );
  }

  function finishNetworkRequest(request) {
    if (request.stallTimer !== null) {
      window.clearTimeout(request.stallTimer);
      request.stallTimer = null;
    }
  }

  function logNetworkCompleted(request, status, statusText) {
    finishNetworkRequest(request);
    var currentLogger = window.StremioLightningLogger;
    if (!currentLogger || !request.classification) return;
    currentLogger.info(
      "bridge.network",
      "[StremioLightning] Stream request #" + request.id + " completed:",
      networkRequestSummary(request, request.classification),
      "-> HTTP " + networkStatus(status, statusText),
      "in " + (Date.now() - request.startedAt) + " ms",
    );
  }

  function logNetworkFailure(request, status, statusText, error) {
    finishNetworkRequest(request);
    var currentLogger = window.StremioLightningLogger;
    var classification = request.classification || classifyRequest(request.input);
    if (!currentLogger || !classification) return;
    var requestId = request.id || nextNetworkRequestId++;
    currentLogger.error(
      "bridge.network",
      "[StremioLightning] Browser request #" + requestId + " failed:",
      networkRequestSummary(request, classification),
      "-> " + (status ? "HTTP " : "") + networkStatus(status, statusText),
      "after " + (Date.now() - request.startedAt) + " ms",
      error || "",
    );
  }

  if (!window.__stremioLightningNetworkDiagnosticsInstalled) {
    window.__stremioLightningNetworkDiagnosticsInstalled = true;

    if (typeof window.fetch === "function") {
      var originalFetch = window.fetch;
      window.fetch = function () {
        var input = arguments[0];
        var options = arguments[1];
        var method = options && options.method || input && input.method;
        var request = createNetworkRequest(input, method, "fetch");
        return originalFetch.apply(this, arguments).then(
          function (response) {
            if (response && response.ok === false) {
              logNetworkFailure(request, response.status, response.statusText);
            } else if (response) {
              logNetworkCompleted(request, response.status, response.statusText);
            }
            return response;
          },
          function (error) {
            logNetworkFailure(request, 0, "", error);
            throw error;
          },
        );
      };
    }

    if (typeof window.XMLHttpRequest === "function") {
      var originalXhrOpen = window.XMLHttpRequest.prototype.open;
      var originalXhrSend = window.XMLHttpRequest.prototype.send;
      window.XMLHttpRequest.prototype.open = function () {
        this.__stremioLightningRequestInput = arguments[1];
        this.__stremioLightningRequestMethod = arguments[0];
        return originalXhrOpen.apply(this, arguments);
      };
      window.XMLHttpRequest.prototype.send = function () {
        var request = createNetworkRequest(
          this.__stremioLightningRequestInput,
          this.__stremioLightningRequestMethod,
          "XHR",
        );
        this.__stremioLightningNetworkRequest = request;
        if (!this.__stremioLightningDiagnosticsAttached) {
          this.__stremioLightningDiagnosticsAttached = true;
          this.addEventListener("load", function () {
            var activeRequest = this.__stremioLightningNetworkRequest;
            if (this.status >= 400) {
              logNetworkFailure(activeRequest, this.status, this.statusText);
            } else if (this.status > 0) {
              logNetworkCompleted(activeRequest, this.status, this.statusText);
            }
          });
          this.addEventListener("error", function () {
            logNetworkFailure(
              this.__stremioLightningNetworkRequest,
              this.status,
              this.statusText,
            );
          });
          this.addEventListener("timeout", function () {
            logNetworkFailure(
              this.__stremioLightningNetworkRequest,
              this.status,
              this.statusText,
              "request timed out",
            );
          });
          this.addEventListener("abort", function () {
            logNetworkFailure(
              this.__stremioLightningNetworkRequest,
              this.status,
              this.statusText,
              "request aborted",
            );
          });
        }
        try {
          return originalXhrSend.apply(this, arguments);
        } catch (error) {
          logNetworkFailure(request, 0, "", error);
          throw error;
        }
      };
    }
  }

  if (!window.__stremioLightningErrorHandlersInstalled) {
    window.__stremioLightningErrorHandlersInstalled = true;
    window.addEventListener("error", function (event) {
      var currentLogger = window.StremioLightningLogger;
      if (!currentLogger) return;

      if (event.error || event.message) {
        currentLogger.error(
          "bridge.browser",
          "[StremioLightning] Uncaught browser error:",
          event.error || event.message,
        );
        return;
      }

      var target = event.target;
      var tagName = target && target.tagName;
      var isStylesheet = tagName === "LINK" &&
        String(target.rel || "").toLowerCase() === "stylesheet";
      if (tagName !== "SCRIPT" && !isStylesheet) return;
      currentLogger.error(
        "bridge.browser",
        "[StremioLightning] Browser resource failed to load:",
        isStylesheet ? "stylesheet" : "script",
      );
    }, true);
    window.addEventListener("unhandledrejection", function (event) {
      var currentLogger = window.StremioLightningLogger;
      if (!currentLogger) return;
      currentLogger.error(
        "bridge.browser",
        "[StremioLightning] Unhandled promise rejection:",
        event.reason,
      );
    });
  }
})();
