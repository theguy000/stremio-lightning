function initDiscordRpcTracker(ctx) {
  var host = ctx.host;
  var shellTransport = ctx.shellTransport;
  var mpvState = shellTransport.mpvState;
  var trackerActive = false;

  function normalizePlaybackState(fullState) {
    var streamState =
      fullState && fullState.streamState ? fullState.streamState : {};
    var currentTime = toFiniteNumber(streamState.time);
    if (!(currentTime > 0)) currentTime = toFiniteNumber(streamState["time-pos"]);
    if (!(currentTime > 0)) currentTime = toFiniteNumber(fullState && fullState.time);
    if (!(currentTime > 0)) currentTime = toFiniteNumber(fullState && fullState["time-pos"]);
    if (!(currentTime > 0)) currentTime = toFiniteNumber(mpvState.timePos);

    var duration = toFiniteNumber(streamState.duration);
    if (!(duration > 0)) duration = toFiniteNumber(fullState && fullState.duration);
    if (!(duration > 0)) duration = toFiniteNumber(fullState && fullState["duration"]);
    if (!(duration > 0)) duration = toFiniteNumber(mpvState.duration);

    var pausedValue = streamState.paused;
    if (typeof pausedValue !== "boolean") pausedValue = streamState.pause;
    if (typeof pausedValue !== "boolean") pausedValue = streamState["paused-for-cache"];
    if (typeof pausedValue !== "boolean") pausedValue = fullState && fullState.paused;
    if (typeof pausedValue !== "boolean") pausedValue = fullState && fullState.pause;
    if (typeof pausedValue !== "boolean") pausedValue = mpvState.pause;
    if (!pausedValue && mpvState.pausedForCache) pausedValue = true;

    return {
      currentTime: Math.max(0, currentTime),
      duration: Math.max(0, duration),
      isPaused: !!pausedValue,
    };
  }

  function buildWatchingLabels(meta, seriesInfo) {
    var details = meta && meta.name ? meta.name : "Unknown title";
    var state = "Watching";

    if (meta && meta.type === "series" && seriesInfo) {
      var parts = [];
      var isKitsu = meta.id && meta.id.indexOf("kitsu:") === 0;

      if (!isKitsu && seriesInfo.season != null) {
        parts.push("Season " + seriesInfo.season);
      }
      if (seriesInfo.episode != null) {
        parts.push("Episode " + seriesInfo.episode);
      }
      if (parts.length) state = parts.join(" - ");
    }

    return { details: details, state: state };
  }

  function buildWatchingActivity(meta, seriesInfo, currentTime, duration, isPaused) {
    var labels = buildWatchingLabels(meta, seriesInfo);
    var activity = {
      details: labels.details,
      state: isPaused ? "Paused" : labels.state,
      largeImageKey: meta && meta.poster ? meta.poster : "stremio",
      largeImageText: meta && meta.name ? meta.name : "Stremio Lightning",
      activityType: 3,
    };

    if (!isPaused && duration > 0) {
      var now = Math.floor(Date.now() / 1000);
      var safeCurrentTime = Math.max(0, Math.floor(currentTime));
      var safeDuration = Math.max(safeCurrentTime, Math.ceil(duration));
      activity.startTimestamp = now - safeCurrentTime;
      activity.endTimestamp = activity.startTimestamp + safeDuration;
    }

    return activity;
  }

  var CORESTATE_MAX_RETRIES = 30;
  var CORESTATE_RETRY_INTERVAL = 1000;

  function waitForPlayerState() {
    var attempt = 0;
    return new Promise(function (resolve) {
      function tryOnce() {
        if (attempt >= CORESTATE_MAX_RETRIES) return resolve(null);
        attempt++;
        evalInPageContext('core.transport.getState("player")')
          .then(function (state) {
            if (state && state.metaItem && state.metaItem.content) {
              resolve(true);
            } else {
              setTimeout(tryOnce, CORESTATE_RETRY_INTERVAL);
            }
          })
          .catch(function () {
            setTimeout(tryOnce, CORESTATE_RETRY_INTERVAL);
          });
      }
      tryOnce();
    });
  }

  function getMetaDetails() {
    var attempt = 0;
    return new Promise(function (resolve) {
      function tryOnce() {
        if (attempt >= CORESTATE_MAX_RETRIES) return resolve(null);
        attempt++;
        evalInPageContext('core.transport.getState("meta_details")')
          .then(function (state) {
            if (
              state &&
              state.metaItem &&
              state.metaItem.content &&
              state.metaItem.content.content
            ) {
              resolve(state.metaItem.content.content);
            } else {
              setTimeout(tryOnce, CORESTATE_RETRY_INTERVAL);
            }
          })
          .catch(function () {
            setTimeout(tryOnce, CORESTATE_RETRY_INTERVAL);
          });
      }
      tryOnce();
    });
  }

  var discordTracker = {
    _mpvPollInterval: null,

    init: function () {
      if (trackerActive) return;
      trackerActive = true;
      shellTransport.observeMpvProperties();
      console.info("[DiscordRPC] Tracker initialized, current hash:", location.hash);
      window.addEventListener("hashchange", discordTracker.handleNavigation);
      discordTracker.handleNavigation();
    },

    stop: function () {
      console.info("[DiscordRPC] Tracker stopped");
      trackerActive = false;
      discordTracker._stopMpvPoll();
      window.removeEventListener("hashchange", discordTracker.handleNavigation);
    },

    handleNavigation: function () {
      if (!trackerActive) return;
      var hash = location.hash;
      if (discordTracker._mpvPollInterval && (hash === "" || hash === "#/")) return;
      discordTracker._checkWatching();
      discordTracker._checkExploring();
      discordTracker._checkMainMenu();
    },

    _stopMpvPoll: function () {
      if (discordTracker._mpvPollInterval) {
        clearInterval(discordTracker._mpvPollInterval);
        discordTracker._mpvPollInterval = null;
      }
    },

    _checkWatching: function () {
      if (location.href.indexOf("#/player") === -1) {
        discordTracker._stopMpvPoll();
        return;
      }

      shellTransport.observeMpvProperties();
      discordTracker._stopMpvPoll();

      waitForPlayerState().then(function (playerReady) {
        if (!playerReady) {
          console.warn("[DiscordRPC] Could not get player state");
          return;
        }

        function pollAndUpdate() {
          if (location.href.indexOf("#/player") === -1) {
            discordTracker._stopMpvPoll();
            return;
          }

          evalInPageContext('core.transport.getState("player")')
            .then(function (fullState) {
              if (!fullState) return;

              var playback = normalizePlaybackState(fullState);
              var meta =
                fullState.metaItem && fullState.metaItem.content
                  ? fullState.metaItem.content
                  : null;
              var seriesInfo = fullState.seriesInfo || null;

              if (!meta) return;

              var activity = buildWatchingActivity(
                meta,
                seriesInfo,
                playback.currentTime,
                playback.duration,
                playback.isPaused,
              );

              host.invoke("update_discord_activity", { activity: activity }).catch(
                function (e) {
                  console.error("[DiscordRPC] update failed:", e);
                },
              );
            })
            .catch(function (e) {
              console.error("[DiscordRPC] pollAndUpdate error:", e);
            });
        }

        pollAndUpdate();
        discordTracker._mpvPollInterval = setInterval(pollAndUpdate, 5000);
      });
    },

    _checkExploring: function () {
      if (location.href.indexOf("#/detail") === -1) return;

      getMetaDetails().then(function (meta) {
        if (!meta) {
          console.warn("[DiscordRPC] Could not get meta details");
          return;
        }
        host.invoke("update_discord_activity", {
          activity: {
            details: meta.name,
            state: "Exploring",
            largeImageKey: meta.poster || "stremio",
            largeImageText: "Stremio Lightning",
            smallImageKey: "hamburger",
            smallImageText: "Main Menu",
            activityType: 3,
          },
        }).catch(function (e) {
          console.error("[DiscordRPC] update failed:", e);
        });
      });
    },

    _checkMainMenu: function () {
      var hashMap = {
        "": "Home",
        "#/": "Home",
        "#/board": "Home",
        "#/discover": "Discover",
        "#/library": "Library",
        "#/calendar": "Calendar",
        "#/addons": "Addons",
        "#/settings": "Settings",
        "#/search": "Search",
      };

      var activity = hashMap[location.hash];
      if (!activity) return;

      host.invoke("update_discord_activity", {
        activity: {
          details: activity,
          largeImageKey: "stremio",
          largeImageText: "Stremio Lightning",
          smallImageKey: "hamburger",
          smallImageText: "Main Menu",
          activityType: 3,
        },
      }).catch(function (e) {
        console.error("[DiscordRPC] update failed:", e);
      });
    },
  };

  window.StremioEnhancedAPI._discordTrackerInit = function () {
    discordTracker.init();
  };
  window.StremioEnhancedAPI._discordTrackerStop = function () {
    discordTracker.stop();
  };

  window.addEventListener("sl-discord-rpc-enable", function () {
    discordTracker.init();
  });
  window.addEventListener("sl-discord-rpc-disable", function () {
    discordTracker.stop();
  });
  window.addEventListener("sl-mods-panel", function (e) {
    if (!trackerActive) return;
    if (e.detail) {
      host.invoke("update_discord_activity", {
        activity: {
          details: "Mods",
          state: "Browsing mods",
          largeImageKey: "stremio",
          largeImageText: "Stremio Lightning",
          smallImageKey: "hamburger",
          smallImageText: "Main Menu",
          activityType: 3,
        },
      }).catch(function (error) {
        console.error("[DiscordRPC] update failed:", error);
      });
    } else {
      discordTracker.handleNavigation();
    }
  });

  ctx.initDiscordRpc = function () {
    var enabled = localStorage.getItem("discordrichpresence");
    if (enabled === "true") {
      host.invoke("start_discord_rpc")
        .then(function () {
          discordTracker.init();
          console.info("[StremioLightning] Discord RPC started");
        })
        .catch(function (e) {
          console.error("[StremioLightning] Failed to start Discord RPC:", e);
        });
    }
  };
}
