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
      message: truncate(formattedValues.join(" "), maxMessageLength),
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
})();
