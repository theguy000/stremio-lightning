function initCoreStyles() {
  var style = document.getElementById("sl-core-ui-styles");
  if (style) {
    return;
  }

  style = document.createElement("style");
  style.id = "sl-core-ui-styles";
  style.textContent = [
    ".back-button-container-lDB1N svg {",
    "  filter:",
    "    drop-shadow(1px 0 0 rgba(15, 15, 25, 0.25))",
    "    drop-shadow(-1px 0 0 rgba(15, 15, 25, 0.25))",
    "    drop-shadow(0 1px 0 rgba(15, 15, 25, 0.25))",
    "    drop-shadow(0 -1px 0 rgba(15, 15, 25, 0.25)) !important;",
    "}"
  ].join("\n");
  document.head.appendChild(style);
}

