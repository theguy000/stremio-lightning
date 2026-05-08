function initUpdateBanner(ctx) {
  var host = ctx.host;

  function injectUpdateBannerStyles() {
    if (document.getElementById("sl-update-banner-styles")) return;
    var style = document.createElement("style");
    style.id = "sl-update-banner-styles";
    style.textContent =
      "@keyframes sl-banner-slide-down { from { transform:translateY(-100%); opacity:0; } to { transform:translateY(0); opacity:1; } }" +
      ".sl-update-banner { position:fixed; top:0; left:0; right:0; z-index:200000; display:flex; align-items:center; justify-content:center; padding:0; background:linear-gradient(180deg, rgba(12,11,17,0.95) 0%, rgba(12,11,17,0.88) 100%); border-bottom:1px solid rgba(255,255,255,0.06); backdrop-filter:blur(30px) saturate(140%); -webkit-backdrop-filter:blur(30px) saturate(140%); box-shadow:0 8px 32px rgba(0,0,0,0.4), 0 2px 8px rgba(0,0,0,0.2); animation:sl-banner-slide-down 0.4s cubic-bezier(0.16,1,0.3,1); font-family:inherit; box-sizing:border-box; }" +
      ".sl-update-banner-content { display:flex; align-items:center; gap:1rem; width:100%; padding:0.75rem 1.25rem; box-sizing:border-box; }" +
      ".sl-update-banner-icon { flex:none; display:flex; align-items:center; justify-content:center; width:2rem; height:2rem; border-radius:50%; background:rgba(123,91,245,0.12); color:var(--primary-accent-color, #7b5bf5); }" +
      ".sl-update-banner-icon svg { width:1rem; height:1rem; }" +
      ".sl-update-banner-text { flex:1; font-size:inherit; font-weight:400; color:var(--primary-foreground-color, rgba(255,255,255,0.8)); line-height:1.4; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }" +
      ".sl-update-banner-version { font-weight:600; color:var(--primary-accent-color, #7b5bf5); }" +
      ".sl-update-banner-current { opacity:0.5; }" +
      ".sl-update-banner-actions { flex:none; display:flex; align-items:center; gap:0.5rem; }" +
      ".sl-update-banner-download { flex:none; padding:0.45rem 1.1rem; border:none; border-radius:var(--border-radius, 0.5rem); background:var(--primary-accent-color, #7b5bf5); color:#fff; font-size:inherit; font-weight:600; cursor:pointer; transition:background 0.15s, transform 0.1s, box-shadow 0.15s; box-shadow:0 2px 8px rgba(123,91,245,0.3); }" +
      ".sl-update-banner-download:hover { background:color-mix(in srgb, var(--primary-accent-color, #7b5bf5) 85%, white); box-shadow:0 4px 16px rgba(123,91,245,0.4); }" +
      ".sl-update-banner-download:active { transform:scale(0.97); }" +
      ".sl-update-banner-close { flex:none; display:flex; align-items:center; justify-content:center; width:2rem; height:2rem; border:none; border-radius:var(--border-radius, 0.5rem); background:transparent; color:rgba(255,255,255,0.35); cursor:pointer; transition:background 0.15s, color 0.15s; padding:0; }" +
      ".sl-update-banner-close:hover { background:var(--overlay-color, rgba(255,255,255,0.08)); color:rgba(255,255,255,0.8); }" +
      ".sl-update-banner-close svg { width:0.9rem; height:0.9rem; }" +
      "@media only screen and (max-width: 600px) { .sl-update-banner-content { padding:0.6rem 0.75rem; gap:0.6rem; } .sl-update-banner-current { display:none; } }";
    document.head.appendChild(style);
  }

  function showUpdateBanner(info) {
    if (document.getElementById("sl-update-banner")) return;

    injectUpdateBannerStyles();

    var banner = document.createElement("div");
    banner.id = "sl-update-banner";
    banner.className = "sl-update-banner";

    var content = document.createElement("div");
    content.className = "sl-update-banner-content";

    var iconDiv = document.createElement("div");
    iconDiv.className = "sl-update-banner-icon";
    iconDiv.innerHTML =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>';

    var textSpan = document.createElement("span");
    textSpan.className = "sl-update-banner-text";
    textSpan.appendChild(
      document.createTextNode(
        "A new version of Stremio Lightning is available: ",
      ),
    );

    var versionSpan = document.createElement("span");
    versionSpan.className = "sl-update-banner-version";
    versionSpan.textContent = info.newVersion;
    textSpan.appendChild(versionSpan);
    textSpan.appendChild(document.createTextNode(" "));

    var currentSpan = document.createElement("span");
    currentSpan.className = "sl-update-banner-current";
    currentSpan.textContent = "(you have v" + info.currentVersion + ")";
    textSpan.appendChild(currentSpan);

    var actionsDiv = document.createElement("div");
    actionsDiv.className = "sl-update-banner-actions";

    var downloadBtn = document.createElement("button");
    downloadBtn.className = "sl-update-banner-download";
    downloadBtn.textContent = "Download Update";
    downloadBtn.addEventListener("click", function () {
      host.invoke("open_external_url", { url: info.releaseUrl }).catch(function (e) {
        console.error("[AppUpdater] Failed to open release URL:", e);
      });
    });

    var closeBtn = document.createElement("button");
    closeBtn.className = "sl-update-banner-close";
    closeBtn.title = "Dismiss";
    closeBtn.innerHTML =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>';
    closeBtn.addEventListener("click", function () {
      banner.style.animation = "none";
      banner.style.transition =
        "transform 0.25s ease-in, opacity 0.25s ease-in";
      banner.style.transform = "translateY(-100%)";
      banner.style.opacity = "0";
      setTimeout(function () {
        banner.remove();
      }, 260);
      try {
        localStorage.setItem("sl-dismissed-update", info.newVersion);
      } catch (_) {}
    });

    actionsDiv.appendChild(downloadBtn);
    actionsDiv.appendChild(closeBtn);
    content.appendChild(iconDiv);
    content.appendChild(textSpan);
    content.appendChild(actionsDiv);
    banner.appendChild(content);
    document.body.insertBefore(banner, document.body.firstChild);
  }

  ctx.initUpdateChecker = function () {
    setTimeout(function () {
      if (
        !window.StremioEnhancedAPI ||
        typeof window.StremioEnhancedAPI.checkAppUpdate !== "function"
      ) {
        return;
      }

      window.StremioEnhancedAPI.checkAppUpdate()
        .then(function (info) {
          if (!info || !info.hasUpdate) return;
          try {
            var dismissed = localStorage.getItem("sl-dismissed-update");
            if (dismissed === info.newVersion) return;
          } catch (_) {}
          showUpdateBanner(info);
        })
        .catch(function (e) {
          console.error("[AppUpdater] Failed to check for updates:", e);
        });
    }, 5000);
  };
}
