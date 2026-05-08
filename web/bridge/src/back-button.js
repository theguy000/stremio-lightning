function initBackButton() {
  function isIntroPage() {
    return window.location.hash.indexOf("/intro") !== -1;
  }

  function updateBackButton() {
    var btn = document.getElementById("sl-back-btn");
    if (isIntroPage()) {
      if (!btn) injectBackButton();
    } else if (btn) {
      btn.remove();
    }
  }

  function injectBackButton() {
    if (document.getElementById("sl-back-btn")) return;

    var btn = document.createElement("div");
    btn.id = "sl-back-btn";
    btn.title = "Go Back";
    btn.innerHTML =
      '<svg viewBox="0 0 512 512" style="width:20px;height:20px;">' +
      '<path d="M328.6 106.5l-143.5 136.9 143.5 136.9" ' +
      'style="stroke:currentColor;stroke-linecap:round;stroke-linejoin:round;stroke-width:48;fill:none;"></path></svg>';

    var style = document.getElementById("sl-back-btn-style");
    if (!style) {
      style = document.createElement("style");
      style.id = "sl-back-btn-style";
      style.textContent =
        "#sl-back-btn {" +
        "position:fixed; top:12px; z-index:10000;" +
        "margin-left:max(0rem, calc(1rem - var(--safe-area-inset-left, 0px)));" +
        "cursor:pointer; color:white;" +
        "align-items:center; display:flex; flex:none;" +
        "justify-content:center;" +
        "height:3.5rem; width:3.5rem;" +
        "border-radius:0.75rem; opacity:0.6;" +
        "transition:opacity 0.15s, background 0.15s;" +
        "}" +
        "#sl-back-btn:hover {" +
        "opacity:1; background:rgba(255,255,255,0.08);" +
        "}";
      document.head.appendChild(style);
    }

    btn.addEventListener("click", function () {
      window.history.back();
    });

    document.body.appendChild(btn);
  }

  window.addEventListener("hashchange", updateBackButton);
  onDomReady(updateBackButton);
}
