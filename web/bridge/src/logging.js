// This bootstrap is injected before every bridge module so early diagnostics remain available.
(function () {
  "use strict";

  var originalConsole = {};
  ["debug", "info", "warn", "error"].forEach(function (level) {
    originalConsole[level] = console[level];
  });
  window.__stremioLightningOriginalConsole = originalConsole;

  var entries = [];
  var listeners = [];
  var nextId = 1;
  var capacity = 2000;
  var maxCollectionItems = 100;
  var maxDepth = 6;
  var maxMessageLength = 16384;
  var maxSourceLength = 256;
  var extendedDiagnostics = false;
  var nativeHttpCapture = false;
  var nativeNetworkFailureCapture = false;
  var pendingEntries = [];
  var pendingBytes = 0;
  var pendingLimit = 500;
  var pendingBytesLimit = 1024 * 1024;
  var batchLimit = 50;
  var batchBytesLimit = 128 * 1024;
  var flushTimer = null;
  var retryTimer = null;
  var flushing = false;
  var inFlightFlush = null;
  var flushingGeneration = null;
  var queueGeneration = 0;
  var flushAttempts = 0;
  var droppedEntries = 0;
  var recentFingerprints = Object.create(null);
  var originIds = Object.create(null);
  var nextOriginId = 1;
  var nextNetworkRequestId = 1;
  var streamRequestStallThresholdMs = 15000;

  function truncate(value, maxLength) {
    var suffix = "... [truncated]";
    return value.length <= maxLength
      ? value
      : value.slice(0, maxLength - suffix.length) + suffix;
  }

  function redactSensitiveDetails(value) {
    return value
      .replace(/\b((?:https?|ftp|rtsp):\/\/)[^/\s:@]+:[^@\s/]+@/gi, "$1[redacted]@")
      .replace(
        /\b(authorization|proxy-authorization|cookie|set-cookie|token|access[_-]?token|refresh[_-]?token|api[_-]?key|password|passwd|secret|session[_-]?id)\b(\\?["']?\s*[:=]\s*\\?["']?)(?:"[^"]*"|'[^']*'|(?:Bearer\s+)?[^\s}\]\r\n,;)]+)/gi,
        "$1$2[redacted]",
      )
      .replace(/\b[A-Z]:\\[^\r\n\t,;)\]]+/gi, "[redacted local path]")
      .replace(/\/(?:home|Users)\/[^/\s]+\/[^\s)\]]*/g, "[redacted local path]");
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

  function sanitizeSource(source) {
    try {
      source = String(source);
      if (/:\/\/|^(?:magnet|data|file):/i.test(source)) return "browser.external";
      return truncate(redactSensitiveDetails(source), maxSourceLength);
    } catch (_) {
      return "unknown";
    }
  }

  function diagnosticsHost() {
    var host = window.StremioLightningHost;
    return host && typeof host.invoke === "function" ? host : null;
  }

  function queueDropWarning() {
    if (!droppedEntries || pendingEntries.length >= pendingLimit) return;
    var count = droppedEntries;
    droppedEntries = 0;
    enqueuePersisted({
      timestamp: Date.now(),
      level: "warn",
      source: "bridge.diagnostics",
      message: "Dropped " + count + " diagnostic record" + (count === 1 ? "" : "s") + " while the native diagnostics sink was unavailable",
    }, false);
  }

  function scheduleFlush(delay) {
    if (flushTimer !== null || retryTimer !== null) return;
    flushTimer = window.setTimeout(function () {
      flushTimer = null;
      flushPersisted();
    }, delay);
  }

  function enqueuePersisted(entry, immediate) {
    var queued = {
      timestamp: entry.timestamp,
      level: entry.level,
      source: entry.source,
      message: entry.message,
    };
    // Four bytes per UTF-16 code unit also safely covers JSON escaping overhead.
    var queuedBytes = (queued.source.length + queued.message.length) * 4 + 64;
    while (
      pendingEntries.length >= pendingLimit ||
      (pendingEntries.length && pendingBytes + queuedBytes > pendingBytesLimit)
    ) {
      var removed = pendingEntries.shift();
      pendingBytes = Math.max(0, pendingBytes - removed.__diagnosticBytes);
      droppedEntries++;
      if (droppedEntries === 1) {
        emit("warn", "bridge.diagnostics", [
          "Browser diagnostic records were dropped because the native diagnostics queue is full",
        ], { mirror: false });
      }
    }
    queued.__diagnosticBytes = queuedBytes;
    pendingEntries.push(queued);
    pendingBytes += queuedBytes;
    if (immediate) flushPersisted();
    else scheduleFlush(500);
  }

  function fingerprint(entry) {
    return entry.level + "\n" + entry.source + "\n" + entry.message;
  }

  function queuePersisted(entry) {
    var key = fingerprint(entry);
    var now = Date.now();
    var previous = recentFingerprints[key];
    if (previous && now - previous.lastAt < 10000) {
      previous.count++;
      return;
    }
    if (previous && previous.count) {
      enqueuePersisted({
        timestamp: now,
        level: "warn",
        source: "bridge.diagnostics",
        message: "Suppressed " + previous.count + " duplicate diagnostic record" + (previous.count === 1 ? "" : "s") + " from " + entry.source,
      }, false);
    }
    recentFingerprints[key] = { lastAt: now, count: 0 };
    enqueuePersisted(entry, entry.level === "error");
  }

  function flushPersisted() {
    if (flushing || pendingEntries.length === 0) {
      return inFlightFlush || Promise.resolve();
    }
    var host = diagnosticsHost();
    if (!host) {
      scheduleFlush(1000);
      return Promise.resolve();
    }
    var batch = [];
    var batchBytes = 0;
    while (pendingEntries.length && batch.length < batchLimit) {
      var candidate = pendingEntries[0];
      if (batch.length && batchBytes + candidate.__diagnosticBytes > batchBytesLimit) break;
      pendingEntries.shift();
      pendingBytes = Math.max(0, pendingBytes - candidate.__diagnosticBytes);
      batchBytes += candidate.__diagnosticBytes;
      batch.push(candidate);
    }
    if (droppedEntries) queueDropWarning();
    var generation = queueGeneration;
    flushing = true;
    flushingGeneration = generation;
    var hostBatch = batch.map(function (entry) {
      return {
        timestamp: entry.timestamp,
        level: entry.level,
        source: entry.source,
        message: entry.message,
      };
    });
    var invocation;
    try {
      invocation = host.invoke("submit_diagnostic_logs", { entries: hostBatch });
    } catch (error) {
      invocation = Promise.reject(error);
    }
    var submission = Promise.resolve(invocation).then(
      function () {
        if (generation !== queueGeneration) {
          if (flushingGeneration === generation) {
            flushing = false;
            flushingGeneration = null;
            inFlightFlush = null;
          }
          return;
        }
        flushing = false;
        flushingGeneration = null;
        flushAttempts = 0;
        inFlightFlush = null;
        if (pendingEntries.length) return flushPersisted();
      },
      function () {
        if (generation !== queueGeneration) {
          if (flushingGeneration === generation) {
            flushing = false;
            flushingGeneration = null;
            inFlightFlush = null;
          }
          return;
        }
        flushing = false;
        flushingGeneration = null;
        inFlightFlush = null;
        flushAttempts++;
        pendingEntries = batch.concat(pendingEntries);
        pendingBytes += batchBytes;
        while (pendingEntries.length > pendingLimit || pendingBytes > pendingBytesLimit) {
          var removed = pendingEntries.pop();
          pendingBytes = Math.max(0, pendingBytes - removed.__diagnosticBytes);
          droppedEntries++;
        }
        var delay = Math.min(4000, 250 * Math.pow(2, flushAttempts));
        retryTimer = window.setTimeout(function () {
          retryTimer = null;
          flushPersisted();
        }, delay);
      },
    );
    inFlightFlush = submission;
    return submission;
  }

  function flushWithRetry(attempt) {
    return flushPersisted().then(function () {
      if (!pendingEntries.length || attempt >= 2 || !diagnosticsHost()) return;
      return new Promise(function (resolve) {
        window.setTimeout(resolve, 250 * Math.pow(2, attempt));
      }).then(function () {
        if (retryTimer !== null) {
          window.clearTimeout(retryTimer);
          retryTimer = null;
        }
        return flushWithRetry(attempt + 1);
      });
    });
  }

  function emit(level, source, values, options) {
    options = options || {};
    var formattedValues = values.slice(0, maxCollectionItems).map(function (value) {
      return format(value, null, 0);
    });
    if (values.length > maxCollectionItems) formattedValues.push("... [truncated]");
    var entry = {
      id: nextId++,
      timestamp: Date.now(),
      level: level,
      source: sanitizeSource(source),
      message: truncate(redactSensitiveDetails(formattedValues.join(" ")), maxMessageLength),
    };
    entries.push(entry);
    if (entries.length > capacity) entries.shift();
    listeners.slice().forEach(function (listener) {
      try {
        listener(entry);
      } catch (_) {}
    });
    if (options.persist) queuePersisted(entry);
    if (options.mirror !== false) {
      var original = originalConsole[level];
      if (typeof original === "function") original.apply(console, values);
    }
    return entry;
  }

  function explicitLog(level, source, values) {
    return emit(level, source, values, {
      persist: level !== "debug" || extendedDiagnostics,
      mirror: true,
    });
  }

  var logger = {
    debug: function (source) { return explicitLog("debug", source, Array.prototype.slice.call(arguments, 1)); },
    info: function (source) { return explicitLog("info", source, Array.prototype.slice.call(arguments, 1)); },
    warn: function (source) { return explicitLog("warn", source, Array.prototype.slice.call(arguments, 1)); },
    error: function (source) { return explicitLog("error", source, Array.prototype.slice.call(arguments, 1)); },
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
    setExtendedDiagnostics: function (enabled) {
      extendedDiagnostics = !!enabled;
    },
    configure: function (options) {
      options = options || {};
      if (typeof options.extended === "boolean") extendedDiagnostics = options.extended;
      if (typeof options.nativeHttpCapture === "boolean") {
        nativeHttpCapture = options.nativeHttpCapture;
      }
      if (typeof options.nativeNetworkFailureCapture === "boolean") {
        nativeNetworkFailureCapture = options.nativeNetworkFailureCapture;
      }
      if (options.capabilities) {
        if (typeof options.capabilities.nativeHttpCapture === "boolean") {
          nativeHttpCapture = options.capabilities.nativeHttpCapture;
        } else if (typeof options.capabilities.nativeHttpResponseCapture === "boolean") {
          nativeHttpCapture = options.capabilities.nativeHttpResponseCapture;
        }
        if (typeof options.capabilities.nativeNetworkFailureCapture === "boolean") {
          nativeNetworkFailureCapture = options.capabilities.nativeNetworkFailureCapture;
        }
      }
    },
    clearDiagnostics: function () {
      queueGeneration++;
      entries = [];
      pendingEntries = [];
      pendingBytes = 0;
      recentFingerprints = Object.create(null);
      droppedEntries = 0;
      if (flushTimer !== null) window.clearTimeout(flushTimer);
      if (retryTimer !== null) window.clearTimeout(retryTimer);
      flushTimer = null;
      retryTimer = null;
      flushing = false;
      inFlightFlush = null;
      flushingGeneration = null;
      flushAttempts = 0;
    },
    clear: function () { logger.clearDiagnostics(); },
    flush: function () {
      return flushWithRetry(0);
    },
  };

  window.StremioLightningLogger = logger;

  // Direct console calls are page diagnostics. Use the saved methods so logger output never re-enters this hook.
  ["debug", "info", "warn", "error"].forEach(function (level) {
    if (typeof originalConsole[level] !== "function") return;
    var captureConsole = function () {
      var values = Array.prototype.slice.call(arguments);
      if (
        window.StremioLightningLogger === logger &&
        (level === "error" || (level === "warn" && extendedDiagnostics))
      ) {
        emit(level, "web.console", values, { persist: true, mirror: false });
      }
      return originalConsole[level].apply(console, values);
    };
    captureConsole.__stremioLightningConsoleWrapper = true;
    console[level] = captureConsole;
  });

  function refreshDiagnosticsCapabilities() {
    var host = diagnosticsHost();
    if (!host) return;
    var reset;
    try {
      reset = host.invoke("set_extended_diagnostics", { enabled: false });
    } catch (_) {
      reset = Promise.resolve();
    }
    Promise.resolve(reset).catch(function () {}).then(function () {
      return host.invoke("init");
    }).then(function (init) {
      nativeHttpCapture = !!(init && init.diagnostics && init.diagnostics.nativeHttpCapture);
      nativeNetworkFailureCapture = !!(
        init && init.diagnostics && init.diagnostics.nativeNetworkFailureCapture
      );
    }, function () {});
  }
  window.setTimeout(refreshDiagnosticsCapabilities, 0);

  function requestUrl(input) {
    try {
      if (typeof input === "string") return input;
      if (input && typeof input.url === "string") return input.url;
      if (input && typeof input.href === "string") return input.href;
    } catch (_) {}
    return null;
  }

  function descriptorForRequest(input) {
    var rawUrl = requestUrl(input);
    if (!rawUrl) return { kind: "unknown request", originId: 0 };
    var parsedUrl;
    try {
      parsedUrl = new URL(rawUrl, window.location.href);
    } catch (_) {
      return { kind: "unknown request", originId: 0 };
    }
    var path = parsedUrl.pathname.toLowerCase();
    var match = path.match(/\/(manifest|catalog|meta|stream|subtitles)(?:\/|\.json|$)/);
    var kind = match ? {
      manifest: "addon manifest",
      catalog: "catalog",
      meta: "metadata",
      stream: "stream discovery",
      subtitles: "subtitles",
    }[match[1]] : /opensubhash/.test(path) ? "subtitle hash" : "generic request";
    var origin = parsedUrl.protocol + "//" + parsedUrl.host;
    if (!originIds[origin]) originIds[origin] = nextOriginId++;
    return { kind: kind, originId: originIds[origin] };
  }

  function sanitizeMethod(method) {
    method = typeof method === "string" ? method.toUpperCase() : "GET";
    return /^[A-Z]{1,16}$/.test(method) ? method : "UNKNOWN";
  }

  function networkStatus(status, statusText) {
    return status ? "HTTP " + String(status) : "network error";
  }

  function beginNetworkRequest(input, method, transport) {
    var request = {
      id: nextNetworkRequestId++,
      descriptor: descriptorForRequest(input),
      method: sanitizeMethod(method),
      startedAt: Date.now(),
      transport: transport,
      stallTimer: null,
    };
    if (request.descriptor.kind === "stream discovery") {
      request.stallTimer = window.setTimeout(function () {
        logNetworkStalled(request);
      }, streamRequestStallThresholdMs);
    }
    if (extendedDiagnostics) logNetworkStarted(request);
    return request;
  }

  function networkSummary(request) {
    return request.method + " " + request.descriptor.kind + " via " + request.transport + " from origin #" + request.descriptor.originId;
  }

  function finishNetworkRequest(request) {
    if (request && request.stallTimer !== null) {
      window.clearTimeout(request.stallTimer);
      request.stallTimer = null;
    }
  }

  function logNetworkStarted(request) {
    logger.debug("bridge.network", "Request #" + request.id + " started: " + networkSummary(request));
  }

  function logNetworkStalled(request) {
    logger.warn(
      "bridge.network",
      "Stream request #" + request.id + " is still pending after " + streamRequestStallThresholdMs + " ms: " + networkSummary(request),
    );
  }

  function logNetworkCompleted(request, status, statusText) {
    finishNetworkRequest(request);
    if (extendedDiagnostics) {
      logger.debug(
        "bridge.network",
        "Request #" + request.id + " completed: " + networkSummary(request) + " -> " + networkStatus(status, statusText) + " in " + (Date.now() - request.startedAt) + " ms",
      );
    }
  }

  function logNetworkFailure(request, status, statusText, error) {
    finishNetworkRequest(request);
    if (nativeHttpCapture && status) return;
    if (nativeNetworkFailureCapture && !status) return;
    logger.error(
      "bridge.network",
      "Browser request failed: " + networkSummary(request) + " -> " + networkStatus(status, statusText) + " after " + (Date.now() - request.startedAt) + " ms",
      error ? redactSensitiveDetails(format(error, null, 0)) : "",
    );
  }

  if (!window.__stremioLightningNetworkDiagnosticsInstalled) {
    window.__stremioLightningNetworkDiagnosticsInstalled = true;
    if (typeof window.fetch === "function") {
      var originalFetch = window.fetch;
      window.fetch = function () {
        var input = arguments[0];
        var options = arguments[1];
        var request = beginNetworkRequest(input, options && options.method || input && input.method, "fetch");
        try {
          return originalFetch.apply(this, arguments).then(function (response) {
            if (response && (response.ok === false || response.status >= 400)) {
              logNetworkFailure(request, response.status, response.statusText);
            } else if (response) {
              logNetworkCompleted(request, response.status, response.statusText);
            }
            return response;
          }, function (error) {
            logNetworkFailure(request, 0, "", error);
            throw error;
          });
        } catch (error) {
          logNetworkFailure(request, 0, "", error);
          throw error;
        }
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
        var request = beginNetworkRequest(this.__stremioLightningRequestInput, this.__stremioLightningRequestMethod, "XHR");
        this.__stremioLightningNetworkRequest = request;
        if (!this.__stremioLightningDiagnosticsAttached) {
          this.__stremioLightningDiagnosticsAttached = true;
          this.addEventListener("load", function () {
            var active = this.__stremioLightningNetworkRequest;
            if (this.status >= 400) logNetworkFailure(active, this.status, this.statusText);
            else if (this.status > 0) logNetworkCompleted(active, this.status, this.statusText);
          });
          ["error", "timeout", "abort"].forEach(function (eventName) {
            this.addEventListener(eventName, function () {
              logNetworkFailure(this.__stremioLightningNetworkRequest, this.status, this.statusText, "request " + eventName + "ed");
            });
          }, this);
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

  function resourceKind(target) {
    if (target === document || target === window) return "document/navigation resource";
    var tagName = target && target.tagName;
    if (tagName === "LINK") {
      var rel = String(target.rel || "").toLowerCase();
      var as = String(target.as || "").toLowerCase();
      if (rel === "stylesheet") return "stylesheet";
      if (as === "font") return "font";
      return "linked resource";
    }
    return {
      SCRIPT: "script",
      IMG: "image",
      AUDIO: "media audio",
      VIDEO: "media video",
      SOURCE: "media source",
      TRACK: "media track",
      IFRAME: "document/navigation resource",
      HTML: "document/navigation resource",
    }[tagName] || null;
  }

  if (window.__stremioLightningErrorHandlersLogger !== logger) {
    window.__stremioLightningErrorHandlersInstalled = true;
    window.__stremioLightningErrorHandlersLogger = logger;
    window.addEventListener("error", function (event) {
      if (window.StremioLightningLogger !== logger) return;
      if (event.error || event.message) {
        logger.error("bridge.browser", "Uncaught browser error:", event.error || event.message);
        return;
      }
      var kind = resourceKind(event.target);
      if (!kind) return;
      if (kind === "image" && !extendedDiagnostics) return;
      logger.error("bridge.browser", "Browser resource failed to load:", kind, "(resource address redacted)");
    }, true);
    window.addEventListener("unhandledrejection", function (event) {
      if (window.StremioLightningLogger !== logger) return;
      logger.error("bridge.browser", "Unhandled promise rejection:", event.reason);
    });
    window.addEventListener("pagehide", flushPersisted);
    document.addEventListener("visibilitychange", function () {
      if (document.visibilityState === "hidden") flushPersisted();
    });
  }
})();
