// Stremio Lightning - Mod UI
// Navbar button + self-contained overlay panel for plugins, themes, and marketplace
(function() {
  'use strict';

  if (!window.__TAURI__) return;

  var invoke = window.__TAURI__.core.invoke;
  var API = window.StremioEnhancedAPI;
  if (!API) return;

  // ============================================
  // Icons
  // ============================================
  var ICONS = {
    modsOutline: '<svg viewBox="0 -960 960 960" style="fill:currentcolor;"><path d="M638-468 468-638q-6-6-8.5-13t-2.5-15q0-8 2.5-15t8.5-13l170-170q6-6 13-8.5t15-2.5q8 0 15 2.5t13 8.5l170 170q6 6 8.5 13t2.5 15q0 8-2.5 15t-8.5 13L694-468q-6 6-13 8.5t-15 2.5q-8 0-15-2.5t-13-8.5Zm-518-92v-240q0-17 11.5-28.5T160-840h240q17 0 28.5 11.5T440-800v240q0 17-11.5 28.5T400-520H160q-17 0-28.5-11.5T120-560Zm400 400v-240q0-17 11.5-28.5T560-440h240q17 0 28.5 11.5T840-400v240q0 17-11.5 28.5T800-120H560q-17 0-28.5-11.5T520-160Zm-400 0v-240q0-17 11.5-28.5T160-440h240q17 0 28.5 11.5T440-400v240q0 17-11.5 28.5T400-120H160q-17 0-28.5-11.5T120-160Zm80-440h160v-160H200v160Zm467 48 113-113-113-113-113 113 113 113Zm-67 352h160v-160H600v160Zm-400 0h160v-160H200v160Zm160-400Zm194-65ZM360-360Zm240 0Z"></path></svg>',
    mods: '<svg viewBox="0 -960 960 960" style="fill:currentcolor;"><path d="M638-468 468-638q-6-6-8.5-13t-2.5-15q0-8 2.5-15t8.5-13l170-170q6-6 13-8.5t15-2.5q8 0 15 2.5t13 8.5l170 170q6 6 8.5 13t2.5 15q0 8-2.5 15t-8.5 13L694-468q-6 6-13 8.5t-15 2.5q-8 0-15-2.5t-13-8.5Zm-518-92v-240q0-17 11.5-28.5T160-840h240q17 0 28.5 11.5T440-800v240q0 17-11.5 28.5T400-520H160q-17 0-28.5-11.5T120-560Zm400 400v-240q0-17 11.5-28.5T560-440h240q17 0 28.5 11.5T840-400v240q0 17-11.5 28.5T800-120H560q-17 0-28.5-11.5T520-160Zm-400 0v-240q0-17 11.5-28.5T160-440h240q17 0 28.5 11.5T440-400v240q0 17-11.5 28.5T400-120H160q-17 0-28.5-11.5T120-160Z"></path></svg>',
    theme: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" style="fill:currentcolor;width:16px;height:16px;"><path d="M4 3h16a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zm2 9h6a1 1 0 0 1 1 1v3h1v6h-4v-6h1v-2H5a1 1 0 0 1-1-1v-2h2v1zm11.732 1.732l1.768-1.768 1.768 1.768a2.5 2.5 0 1 1-3.536 0z"></path></svg>',
    settings: '<svg viewBox="0 0 512 512" style="width:16px;height:16px;fill:currentcolor;"><path d="M464 250C464 247.333 463 245 461 243L455 240L429 236L426 234L424 231L423 222V218L425 216L448 203L453 198V190L449 179C448.333 177 446.667 175.333 444 174C442 172.667 439.667 172.333 437 173L412 178L408 177L406 175L401 167C400.404 165.878 400.18 164.595 400.359 163.337C400.539 162.079 401.114 160.91 402 160L419 141L421 134C421.667 131.333 421 129 419 127L412 118C410.409 115.878 408.04 114.475 405.414 114.1C402.789 113.725 400.122 114.409 398 116L376 129C375.065 129.916 373.809 130.429 372.5 130.429C371.191 130.429 369.935 129.916 369 129L362 122L360 120V116L369 91.9998C370.096 89.7279 370.292 87.1258 369.551 84.7151C368.809 82.3043 367.183 80.2628 365 78.9998L355 72.9998C353 70.9998 350.667 70.6665 348 71.9998C345.333 72.6665 343 73.9998 341 75.9998L325 95.9998L323 97.9998H319L310 93.9998L307 91.9998L306 88.9998V62.9998C306.667 60.3332 306 57.9998 304 55.9998C302.667 53.3332 300.667 51.9998 298 51.9998L286 49.9998C284 49.3332 281.667 49.6665 279 50.9998L275 56.9998L266 81.9998L264 84.9998H251C249.667 85.6665 248.667 85.3332 248 83.9998L246 81.9998L237 56.9998C237 54.9998 235.667 52.9998 233 50.9998L226 49.9998L214 51.9998C211.333 51.9998 209.333 53.3332 208 55.9998C206 57.9998 205.333 60.3332 206 62.9998V88.9998L205 91.9998L202 93.9998L193 97.9998C190.333 98.6665 188.333 97.9998 187 95.9998L170 75.9998C169.333 73.9998 167.333 72.6665 164 71.9998C162 71.3332 159.667 71.6665 157 72.9998L147 78.9998L142 83.9998V91.9998L152 116V119C152 121 151.333 122 150 122L143 128C142.606 128.525 142.113 128.968 141.548 129.302C140.983 129.637 140.357 129.857 139.707 129.95C139.057 130.042 138.395 130.006 137.759 129.843C137.123 129.68 136.525 129.394 136 129L114 115L107 114L100 117L93 126C91.168 127.869 90.142 130.382 90.142 133C90.142 135.617 91.168 138.131 93 140L111 160L112 163L111 166L106 175L104 177H100L75 173C72.612 172.335 70.062 172.58 67.845 173.689C65.627 174.797 63.901 176.69 63 179L59 190V197C60.334 200.333 62 202.333 64 203L87 216L89 218V223L87 231L86 234L83 236L57 240C54.525 240.249 52.231 241.411 50.567 243.26C48.903 245.109 47.988 247.512 48 250V262C47.988 264.488 48.903 266.89 50.567 268.74C52.231 270.589 54.525 271.751 57 272L83 276L86 278L87 281L89 290V294L87 296L64 309L59 314V322L63 333C63.667 335 65.334 336.667 68 338C70 339.333 72.334 339.667 75 339L100 334L104 335L106 337L111 345C111.733 346.02 112.127 347.244 112.127 348.5C112.127 349.756 111.733 350.98 111 352L94 372C92.306 373.689 91.274 375.929 91.09 378.314C90.907 380.7 91.584 383.072 93 385L100 394C101.591 396.122 103.96 397.524 106.586 397.899C109.211 398.274 111.878 397.591 114 396L136 383C137.061 382.204 138.394 381.863 139.707 382.05C141.02 382.238 142.204 382.939 143 384L150 390C151.333 390.667 152 391.667 152 393V396L143 420C141.904 422.272 141.708 424.874 142.45 427.285C143.191 429.695 144.817 431.737 147 433L157 439C158.111 439.739 159.359 440.248 160.67 440.497C161.98 440.746 163.328 440.73 164.632 440.451C165.937 440.171 167.173 439.633 168.266 438.869C169.36 438.105 170.289 437.129 171 436L187 416L192 414L202 418L205 420L206 423V449C205.333 451.667 206 454 208 456C209.333 458.667 211.333 460 214 460L226 462C228.34 462.158 230.67 461.565 232.649 460.305C234.628 459.046 236.153 457.187 237 455L246 430L248 428C249.333 426.667 250.333 426.333 251 427H261L264 428L266 430L275 455C275.667 457 277 458.667 279 460C280.333 462 282.333 462.667 285 462H287L298 460C301.333 459.333 303.333 458 304 456C306 454 307 451.667 307 449L306 423L307 420L310 418L319 415C322.333 414.333 324.333 414.667 325 416L342 436C343.418 438.012 345.528 439.431 347.927 439.985C350.325 440.538 352.844 440.188 355 439L365 433L370 428V420L360 396V393L362 390L369 384L373 382L376 383L398 396C400.667 398 403 398.667 405 398C407.667 398 410 396.667 412 394L419 385C421 383.667 422 381.667 422 379C422 376.333 421 374 419 372L402 352L400 349L401 345L406 337L409 335L412 334L437 339C438.246 339.477 439.575 339.699 440.909 339.655C442.242 339.61 443.553 339.299 444.765 338.74C445.976 338.181 447.064 337.385 447.963 336.399C448.862 335.413 449.555 334.258 450 333L453 322V314L449 309L426 296L423 294V289L425 281L426 278L429 276L455 272C457.475 271.751 459.769 270.589 461.433 268.74C463.097 266.89 464.013 264.488 464 262V250Z"></path></svg>',
    github: '<svg viewBox="0 0 24 24" style="width:16px;height:16px;fill:currentcolor;"><path d="M12,2A10,10 0 0,0 2,12C2,16.42 4.87,20.17 8.84,21.5C9.34,21.58 9.5,21.27 9.5,21C9.5,20.77 9.5,20.14 9.5,19.31C6.73,19.91 6.14,17.97 6.14,17.97C5.68,16.81 5.03,16.5 5.03,16.5C4.12,15.88 5.1,15.9 5.1,15.9C6.1,15.97 6.63,16.93 6.63,16.93C7.5,18.45 8.97,18 9.54,17.76C9.63,17.11 9.89,16.67 10.17,16.42C7.95,16.17 5.62,15.31 5.62,11.5C5.62,10.39 6,9.5 6.65,8.79C6.55,8.54 6.2,7.5 6.75,6.15C6.75,6.15 7.59,5.88 9.5,7.17C10.29,6.95 11.15,6.84 12,6.84C12.85,6.84 13.71,6.95 14.5,7.17C16.41,5.88 17.25,6.15 17.25,6.15C17.8,7.5 17.45,8.54 17.35,8.79C18,9.5 18.38,10.39 18.38,11.5C18.38,15.32 16.04,16.16 13.81,16.41C14.17,16.72 14.5,17.33 14.5,18.26C14.5,19.6 14.5,20.68 14.5,21C14.5,21.27 14.66,21.59 15.17,21.5C19.14,20.16 22,16.42 22,12A10,10 0 0,0 12,2Z"></path></svg>',
    close: '<svg viewBox="0 0 512 512" style="fill:currentcolor;"><path d="M289.9 256l95-95c4.5-4.53 7-10.63 7.1-17 0-6.38-2.5-12.5-7-17.02s-10.6-7.07-17-7.08c-3.2-0.01-6.3 0.61-9.2 1.81s-5.6 2.96-7.8 5.19l-95 95-95-95c-3.4-3.33-7.6-5.6-12.3-6.51-4.6-0.91-9.4-0.42-13.8 1.4-4.4 1.79-8.1 4.86-10.8 8.81-2.6 3.94-4 8.58-4 13.33-0.1 3.15 0.5 6.28 1.7 9.19 1.2 2.92 3 5.57 5.2 7.78l95 95-95 95c-2.8 2.8-4.8 6.24-6 10.02-1.1 3.78-1.3 7.78-0.5 11.64 0.8 3.87 2.5 7.48 5 10.52 2.5 3.05 5.8 5.43 9.4 6.93 4.4 1.81 9.2 2.29 13.8 1.39 4.7-0.91 8.9-3.17 12.3-6.5l95-95 95 95c3.4 3.34 7.6 5.6 12.3 6.51 4.6 0.92 9.4 0.43 13.8-1.39 4.4-1.8 8.1-4.87 10.8-8.82 2.6-3.94 4-8.58 4-13.33 0.1-3.15-0.5-6.28-1.7-9.2-1.2-2.91-3-5.56-5.2-7.77z"></path></svg>'
  };

  // ============================================
  // Utilities
  // ============================================
  function escapeHtml(str) {
    if (!str) return '';
    return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }

  function getFileNameFromUrl(url) {
    return url.split('/').pop().split('?')[0];
  }

  function applyBlurIntensity(percent, enabled) {
    var root = document.documentElement;
    if (!enabled) {
      root.style.setProperty('--sl-blur', '0px');
      root.style.setProperty('--sl-blur-panel', '0px');
      var styles = getComputedStyle(document.documentElement);
      var primaryBg = styles.getPropertyValue('--primary-background-color').trim() || 'rgba(12, 11, 17, 1)';
      var secondaryBg = styles.getPropertyValue('--secondary-background-color').trim() || 'rgba(26, 23, 62, 1)';
      root.style.setProperty('--sl-panel-bg', 'linear-gradient(41deg, ' + primaryBg + ' 0%, ' + secondaryBg + ' 100%)');
    } else {
      var scale = percent / 100;
      root.style.setProperty('--sl-blur', (16 * scale) + 'px');
      root.style.setProperty('--sl-blur-panel', (30 * scale) + 'px');
      root.style.removeProperty('--sl-panel-bg');
    }
  }

  // ============================================
  // CSS Injection
  // ============================================
  function injectStyles() {
    if (document.getElementById('sl-mod-styles')) return;
    var style = document.createElement('style');
    style.id = 'sl-mod-styles';
    style.textContent =
      '#sl-mods-btn { position:fixed; left:1rem; bottom:1rem; z-index:100000; display:flex; flex-direction:row; align-items:center; justify-content:center; gap:0.55rem; min-height:3.5rem; padding:0.85rem 1rem; border-radius:0.9rem; background:rgba(12,11,17,0.92); border:1px solid rgba(255,255,255,0.08); color:rgba(255,255,255,0.78); cursor:pointer; user-select:none; box-sizing:border-box; box-shadow:0 18px 40px rgba(0,0,0,0.28); backdrop-filter:blur(var(--sl-blur, 14px)); -webkit-backdrop-filter:blur(var(--sl-blur, 14px)); transition:background 0.15s,color 0.15s,border-color 0.15s,transform 0.15s,opacity 0.15s; }' +
      '#sl-mods-btn[data-sl-ready="false"] { opacity:0; visibility:hidden; pointer-events:none; }' +
      '#sl-mods-btn .sl-mods-icon-wrap { position:relative; width:22px; height:22px; flex-shrink:0; }' +
      '#sl-mods-btn .sl-mods-icon-main { position:relative; width:100%; height:100%; color:currentColor; }' +
      '#sl-mods-btn .sl-mods-icon-glyph { position:absolute; inset:0; display:flex; align-items:center; justify-content:center; }' +
      '#sl-mods-btn .sl-mods-icon-main svg { width:100%; height:100%; overflow:visible; }' +
      '#sl-mods-btn .sl-mods-icon-filled { opacity:0; }' +
      '#sl-mods-btn[data-sl-active] .sl-mods-icon-outline { opacity:0; }' +
      '#sl-mods-btn[data-sl-active] .sl-mods-icon-filled { opacity:1; }' +
      '#sl-mods-btn .sl-mods-label { font-size:0.85rem; font-weight:600; line-height:1; letter-spacing:0.01em; }' +
      '#sl-mods-btn:hover { background:rgba(255,255,255,0.08); color:white; border-color:rgba(255,255,255,0.14); }' +
      '#sl-mods-btn:focus-visible { outline:2px solid var(--primary-accent-color, #7b5bf5); outline-offset:2px; }' +
      '#sl-mods-btn[data-sl-anchor="nav"] { position:fixed; left:1rem; top:1rem; bottom:auto; width:calc(var(--sl-nav-width, 94px) - 1.2rem); min-width:0; padding:0.7rem 0.4rem; flex-direction:column; gap:0.2rem; border:none; box-shadow:none; background:transparent; backdrop-filter:none; -webkit-backdrop-filter:none; border-radius:0.75rem; color:rgba(255,255,255,0.52); }' +
      '#sl-mods-btn[data-sl-anchor="nav"] .sl-mods-icon-wrap { width:2.2rem; height:2.2rem; }' +
      '#sl-mods-btn[data-sl-anchor="nav"] .sl-mods-icon-main { opacity:0.35; transition:opacity 0.15s; }' +
      '#sl-mods-btn[data-sl-anchor="nav"] .sl-mods-label { max-height:0; overflow:hidden; font-size:0.72rem; font-weight:500; opacity:0; color:rgba(255,255,255,0.6); transition:none; }' +
      '#sl-mods-btn[data-sl-anchor="nav"]:hover { background:var(--overlay-color, rgba(255,255,255,0.08)); color:rgba(255,255,255,0.52); border-color:transparent; }' +
      '#sl-mods-btn[data-sl-anchor="nav"]:hover .sl-mods-label { max-height:1rem; opacity:1; color:rgba(255,255,255,0.6); }' +
      '#sl-mods-btn[data-sl-active] { background:rgba(255,255,255,0.08); color:var(--primary-accent-color, #7b5bf5); border-color:rgba(255,255,255,0.14); }' +
      '#sl-mods-btn[data-sl-active] .sl-mods-icon-main { color:inherit; opacity:1; }' +
      '#sl-mods-btn[data-sl-active] .sl-mods-label { color:inherit; opacity:0; }' +
      '#sl-mods-btn[data-sl-anchor="nav"][data-sl-active] { background:transparent; color:var(--primary-accent-color, #7b5bf5); }' +
      '#sl-mods-btn[data-sl-anchor="nav"][data-sl-active]:hover { background:var(--overlay-color, rgba(255,255,255,0.08)); }' +
      '#sl-mods-btn[data-sl-anchor="nav"][data-sl-active] .sl-mods-label { max-height:0; opacity:0; color:var(--primary-accent-color, #7b5bf5); }' +
      '#sl-mods-btn[data-sl-anchor="nav"][data-sl-active]:hover .sl-mods-label { max-height:1rem; opacity:1; color:var(--primary-accent-color, #7b5bf5); }' +
      'nav[data-sl-mods-muted] .selected:not(:hover):not(:focus):not(:focus-visible):not(:focus-within) { background:transparent !important; }' +
      'nav[data-sl-mods-muted] .selected:not(:hover):not(:focus):not(:focus-visible):not(:focus-within) .icon, nav[data-sl-mods-muted] .selected:not(:hover):not(:focus):not(:focus-visible):not(:focus-within) svg { color:var(--primary-foreground-color, rgba(255,255,255,0.52)) !important; opacity:0.35 !important; }' +
      'nav[data-sl-mods-muted] .selected:not(:hover):not(:focus):not(:focus-visible):not(:focus-within) .label { color:var(--primary-foreground-color, rgba(255,255,255,0.6)) !important; opacity:0 !important; }' +
      '#sl-mods-btn[data-sl-anchor="floating"] { padding:0.85rem; background:rgba(12,11,17,0.38); backdrop-filter:blur(22px) saturate(140%); -webkit-backdrop-filter:blur(22px) saturate(140%); border-radius:50%; }' +
      '#sl-mods-btn[data-sl-anchor="floating"] .sl-mods-label { display:none; }' +
      '#sl-mods-btn[data-sl-anchor="floating"]:hover { background:rgba(255,255,255,0.12); }' +
      '@media only screen and (max-width: 768px) { #sl-mods-btn[data-sl-anchor="floating"] { left:auto; right:0.75rem; bottom:0.75rem; } }' +

      '#sl-mod-panel { position:fixed; top:0; right:0; bottom:0; z-index:99999; display:none; flex-direction:row; color:var(--primary-foreground-color, #f2f2f2); font-family:inherit; background:var(--sl-panel-bg, linear-gradient(180deg, rgba(0,0,0,0.28) 0%, rgba(0,0,0,0.16) 16%, rgba(0,0,0,0.12) 100%)); backdrop-filter:blur(var(--sl-blur-panel, 30px)) saturate(135%); -webkit-backdrop-filter:blur(var(--sl-blur-panel, 30px)) saturate(135%); overflow:hidden; }' +
      '#sl-mod-panel.sl-open { display:flex; }' +

      '.sl-sidebar { position:relative; z-index:1; flex:none; align-self:stretch; display:flex; flex-direction:column; width:18rem; min-width:18rem; padding:3rem 1.5rem 2rem; box-sizing:border-box; }' +
      '.sl-sidebar-title { margin:0 0 1.25rem; padding:0 0.5rem; font-size:1.75rem; line-height:1.15; font-weight:500; color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-tab { flex:none; align-self:stretch; display:flex; align-items:center; gap:0.9rem; min-height:4rem; padding:0 1.5rem; margin-bottom:0.5rem; border-radius:4rem; color:var(--primary-foreground-color, #f2f2f2); opacity:0.42; cursor:pointer; user-select:none; transition:background-color 0.12s ease-out, opacity 0.12s ease-out; }' +
      '.sl-tab:hover { background-color:var(--overlay-color, rgba(255,255,255,0.08)); opacity:1; }' +
      '.sl-tab.sl-active { background-color:var(--overlay-color, rgba(255,255,255,0.08)); opacity:1; font-weight:600; }' +
      '.sl-tab svg { width:1.25rem; height:1.25rem; flex:none; color:currentColor; opacity:0.8; }' +

      '.sl-content { position:relative; z-index:1; flex:1; align-self:stretch; padding:0 3rem 2.5rem; overflow-y:auto; overflow-x:hidden; box-sizing:border-box; }' +
      '.sl-content::-webkit-scrollbar { width:6px; }' +
      '.sl-content::-webkit-scrollbar-thumb { background:var(--overlay-color, rgba(255,255,255,0.12)); border-radius:999px; }' +
      '.sl-tab-content { display:none; max-width:72rem; min-height:100%; padding:calc(var(--horizontal-nav-bar-size, 5.5rem) + 1.5rem) 0 2rem; box-sizing:border-box; }' +
      '.sl-tab-content.sl-visible { display:block; }' +

      '.sl-section-header { display:flex; align-items:center; justify-content:space-between; gap:1rem; margin:0 0 2rem; }' +
      '.sl-section-title { flex:none; font-size:1.8rem; line-height:1.2; font-weight:500; color:var(--primary-foreground-color, #f2f2f2); }' +

      '.sl-card { display:flex; flex-direction:row; align-items:flex-start; gap:1rem; padding:1.5rem; margin:0 0 1.25rem; border:0.15rem solid transparent; border-radius:var(--border-radius, 1rem); background-color:var(--overlay-color, rgba(255,255,255,0.08)); backdrop-filter:blur(var(--sl-blur, 16px)) saturate(120%); -webkit-backdrop-filter:blur(var(--sl-blur, 16px)) saturate(120%); transition:border-color 0.1s ease-out, background-color 0.1s ease-out; }' +
      '.sl-card:hover { border-color:var(--overlay-color, rgba(255,255,255,0.12)); }' +
      '.sl-card-info { flex:1 1 auto; min-width:0; margin-right:0.75rem; }' +
      '.sl-card-name { display:flex; align-items:center; gap:0.5rem; flex-wrap:wrap; max-height:none; font-size:1.2rem; font-weight:500; color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-card-version { font-size:0.9rem; font-weight:400; color:var(--primary-foreground-color, #f2f2f2); opacity:0.55; }' +
      '.sl-card-desc { margin-top:0.5rem; padding:0 0.5rem 0 0; font-size:0.95rem; line-height:1.55; color:var(--primary-foreground-color, #f2f2f2); opacity:0.78; }' +
      '.sl-card-author { margin-top:0.5rem; font-size:0.9rem; color:var(--primary-foreground-color, #f2f2f2); opacity:0.4; }' +
      '.sl-card-actions { flex:none; display:flex; flex-direction:column; gap:0.85rem; width:16rem; max-width:100%; }' +

      '.sl-toggle { position:relative; display:inline-block; width:44px; height:24px; cursor:pointer; align-self:flex-end; }' +
      '.sl-toggle input { display:none; }' +
      '.sl-toggle-track { width:100%; height:100%; background:var(--overlay-color, rgba(255,255,255,0.15)); border-radius:12px; transition:background 0.2s; position:relative; }' +
      '.sl-toggle input:checked + .sl-toggle-track { background:var(--secondary-accent-color, var(--primary-accent-color, #7b5bf5)); }' +
      '.sl-toggle-thumb { width:20px; height:20px; background:white; border-radius:50%; position:absolute; top:2px; left:2px; transition:transform 0.2s; }' +
      '.sl-toggle input:checked + .sl-toggle-track .sl-toggle-thumb { transform:translateX(20px); }' +

      '.sl-btn { display:inline-flex; align-items:center; justify-content:center; gap:0.6rem; min-height:3.5rem; width:100%; padding:0 1rem; border:none; border-radius:3.5rem; font-size:1rem; font-weight:700; text-decoration:none; cursor:pointer; outline:none; transition:background-color 0.12s ease-out, filter 0.12s ease-out, box-shadow 0.12s ease-out, opacity 0.12s ease-out; box-sizing:border-box; }' +
      '.sl-btn svg { width:1.2rem; height:1.2rem; flex:none; }' +
      '.sl-btn-primary { background-color:var(--secondary-accent-color, var(--primary-accent-color, #7b5bf5)); color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-btn-primary:hover { filter:brightness(1.25); }' +
      '.sl-btn-applied { background-color:rgba(255,255,255,0.06); color:var(--primary-foreground-color, #f2f2f2); opacity:0.5; cursor:default; box-shadow:inset 0 0 0 2px rgba(255,255,255,0.12); pointer-events:none; }' +
      '.sl-btn-danger { background-color:var(--overlay-color, rgba(255,255,255,0.08)); color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-btn-danger:hover { opacity:1; box-shadow:inset 0 0 0 2px var(--danger-accent-color, #ff6b6b); }' +
      '.sl-btn-warning { background-color:var(--overlay-color, rgba(255,255,255,0.08)); color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-btn-warning:hover { box-shadow:inset 0 0 0 2px var(--secondary-accent-color, var(--primary-accent-color, #7b5bf5)); }' +
      '.sl-btn-ghost { background-color:transparent; color:var(--primary-foreground-color, #f2f2f2); opacity:0.88; }' +
      '.sl-btn-ghost:hover { background-color:var(--overlay-color, rgba(255,255,255,0.08)); opacity:1; }' +

      '.sl-gear-btn { width:3rem; height:3rem; border-radius:100%; display:none; align-items:center; justify-content:center; cursor:pointer; color:var(--primary-foreground-color, #f2f2f2); opacity:0.6; transition:background-color 0.12s ease-out, opacity 0.12s ease-out; }' +
      '.sl-gear-btn:hover { background-color:var(--overlay-color, rgba(255,255,255,0.08)); opacity:1; }' +

      '.sl-search { width:min(100%, 30rem); min-height:3.5rem; padding:0 1.25rem; margin:0 0 2rem; border-radius:3rem; border:2px solid transparent; background-color:var(--overlay-color, rgba(255,255,255,0.08)); color:var(--primary-foreground-color, #f2f2f2); font-size:1rem; outline:none; box-sizing:border-box; backdrop-filter:blur(var(--sl-blur, 14px)) saturate(120%); -webkit-backdrop-filter:blur(var(--sl-blur, 14px)) saturate(120%); transition:border-color 0.12s ease-out, background-color 0.12s ease-out; }' +
      '.sl-search:hover, .sl-search:focus { border-color:var(--primary-foreground-color, #f2f2f2); background-color:transparent; }' +
      '.sl-search::placeholder { color:var(--primary-foreground-color, #f2f2f2); opacity:0.4; }' +

      '.sl-badge { display:inline-flex; align-items:center; min-height:1.6rem; padding:0 0.7rem; border-radius:2rem; font-size:0.72rem; font-weight:700; letter-spacing:0.04em; text-transform:uppercase; background-color:var(--overlay-color, rgba(255,255,255,0.08)); color:var(--primary-foreground-color, #f2f2f2); opacity:0.7; }' +

      '.sl-card-logo { width:5rem; height:5rem; padding:0.5rem; border-radius:var(--border-radius, 1rem); object-fit:cover; flex:none; background-color:rgba(255,255,255,0.03); }' +
      '.sl-card-logo-placeholder { width:5rem; height:5rem; padding:0.75rem; border-radius:var(--border-radius, 1rem); background-color:rgba(255,255,255,0.03); display:flex; align-items:center; justify-content:center; color:var(--primary-foreground-color, #f2f2f2); opacity:0.35; flex:none; box-sizing:border-box; }' +

      '.sl-reload-warning { margin:0 0 1.5rem; padding:1rem 1.25rem; border-radius:var(--border-radius, 1rem); background-color:rgba(255, 196, 80, 0.12); color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-reload-warning a { color:inherit; font-weight:700; cursor:pointer; text-decoration:underline; }' +

      '.sl-link { color:var(--secondary-accent-color, var(--primary-accent-color, #7b5bf5)); text-decoration:none; cursor:pointer; }' +
      '.sl-link:hover { text-decoration:underline; }' +

      '.sl-modal-overlay { position:fixed; inset:0; z-index:999999; display:flex; justify-content:center; align-items:center; padding:2rem; background-color:rgba(0,0,0,0.82); }' +
      '.sl-modal { width:min(44rem, 100%); max-height:calc(100vh - 4rem); display:flex; flex-direction:column; overflow:hidden; border-radius:var(--border-radius, 1rem); background-color:var(--modal-background-color, rgba(16,16,20,0.84)); backdrop-filter:blur(var(--sl-blur-panel, 28px)) saturate(135%); -webkit-backdrop-filter:blur(var(--sl-blur-panel, 28px)) saturate(135%); box-shadow:var(--outer-glow, 0 1.35rem 2.7rem rgba(0,0,0,0.45)); color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-modal-header { display:flex; align-items:center; justify-content:space-between; gap:1rem; padding:1.5rem 2rem; border-bottom:thin solid var(--overlay-color, rgba(255,255,255,0.08)); }' +
      '.sl-modal-title { font-size:1.35rem; font-weight:500; color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-modal-close { width:3rem; height:3rem; display:flex; align-items:center; justify-content:center; cursor:pointer; color:var(--primary-foreground-color, #f2f2f2); opacity:0.6; border-radius:100%; transition:background-color 0.12s ease-out, opacity 0.12s ease-out; }' +
      '.sl-modal-close:hover { background-color:var(--overlay-color, rgba(255,255,255,0.08)); opacity:1; }' +
      '.sl-modal-close svg { width:1rem; height:1rem; }' +
      '.sl-modal-body { flex:1; padding:1.5rem 2rem 2rem; overflow-y:auto; }' +
      '.sl-modal-footer { display:flex; justify-content:flex-end; padding:0 2rem 2rem; }' +

      '.sl-setting-row { display:flex; justify-content:space-between; align-items:center; gap:1.5rem; min-height:4.5rem; padding:0.9rem 0; }' +
      '.sl-setting-row:not(:last-child) { border-bottom:thin solid var(--overlay-color, rgba(255,255,255,0.08)); }' +
      '.sl-setting-label { flex:1 1 auto; min-width:0; padding-right:1rem; }' +
      '.sl-setting-label-text { font-size:1rem; color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-setting-label-desc { margin-top:0.35rem; font-size:0.85rem; line-height:1.45; color:var(--primary-foreground-color, #f2f2f2); opacity:0.45; }' +
      '.sl-setting-control { flex:none; width:min(18rem, 42%); }' +
      '.sl-setting-input, .sl-setting-select { width:100%; min-height:3.25rem; padding:0 1rem; border-radius:3rem; border:2px solid transparent; background-color:var(--overlay-color, rgba(255,255,255,0.08)); color:var(--primary-foreground-color, #f2f2f2); font-size:0.95rem; outline:none; box-sizing:border-box; transition:border-color 0.12s ease-out, background-color 0.12s ease-out; }' +
      '.sl-setting-input:hover, .sl-setting-input:focus, .sl-setting-select:hover, .sl-setting-select:focus { border-color:var(--primary-foreground-color, #f2f2f2); background-color:transparent; }' +
      '.sl-setting-select option { background-color:var(--modal-background-color, rgba(16,16,20,0.96)); color:var(--primary-foreground-color, #f2f2f2); }' +

      '.sl-about { max-width:35rem; }' +
      '.sl-about h2 { margin:0 0 0.75rem; font-size:1.8rem; font-weight:500; color:var(--primary-foreground-color, #f2f2f2); }' +
      '.sl-about p { margin:0 0 1.25rem; font-size:1rem; line-height:1.65; color:var(--primary-foreground-color, #f2f2f2); opacity:0.68; }' +

      '.sl-range-value { min-width:3rem; text-align:right; font-size:0.95rem; font-weight:600; color:var(--primary-foreground-color, #f2f2f2); opacity:0.7; }' +
      '.sl-setting-range { -webkit-appearance:none; appearance:none; width:100%; height:6px; border-radius:3px; background:var(--overlay-color, rgba(255,255,255,0.15)); outline:none; cursor:pointer; }' +
      '.sl-setting-range::-webkit-slider-thumb { -webkit-appearance:none; appearance:none; width:18px; height:18px; border-radius:50%; background:var(--secondary-accent-color, var(--primary-accent-color, #7b5bf5)); cursor:pointer; transition:transform 0.1s; }' +
      '.sl-setting-range::-webkit-slider-thumb:hover { transform:scale(1.2); }' +

      '.sl-empty { padding:3rem 1rem; text-align:center; font-size:1rem; color:var(--primary-foreground-color, #f2f2f2); opacity:0.4; }' +

      '.sl-submit-link { margin:0 0 1.5rem; font-size:0.95rem; color:var(--primary-foreground-color, #f2f2f2); opacity:0.68; }' +
      '@media only screen and (max-width: 1100px) { .sl-card { flex-wrap:wrap; } .sl-card-actions { flex-direction:row; width:100%; } .sl-btn { flex:1 1 12rem; } }' +
      '@media only screen and (max-width: 900px) { #sl-mod-panel { left:0 !important; } .sl-sidebar { width:15rem; min-width:15rem; padding:2rem 1rem 1.5rem; } .sl-content { padding:0 1.5rem 1.5rem; } .sl-tab-content { padding-top:calc(var(--horizontal-nav-bar-size, 5.5rem) + 1rem); } .sl-card { padding:1.25rem; } }' +
      '@media only screen and (max-width: 768px) { #sl-mod-panel { flex-direction:column; } .sl-sidebar { width:100%; min-width:0; padding:1rem 1rem 0.5rem; overflow-x:auto; overflow-y:hidden; } .sl-sidebar-title { margin-bottom:0.75rem; font-size:1.5rem; } .sl-sidebar::-webkit-scrollbar { display:none; } .sl-tab { flex:none; min-width:max-content; margin-right:0.5rem; margin-bottom:0; } .sl-content { padding:0 1rem 1.5rem; } .sl-tab-content { padding-top:1rem; } .sl-section-header { flex-direction:column; align-items:flex-start; margin-bottom:1.25rem; } .sl-search { width:100%; } .sl-setting-row { flex-direction:column; align-items:flex-start; } .sl-setting-control { width:100%; } .sl-modal-overlay { padding:1rem; } .sl-modal { width:100%; max-height:calc(100vh - 2rem); } .sl-modal-header, .sl-modal-body, .sl-modal-footer { padding-left:1.25rem; padding-right:1.25rem; } }';

    document.head.appendChild(style);
  }

  // ============================================
  // Mods Button
  // ============================================
  var _navButton = null;
  var _layoutSyncFrame = 0;
  var _layoutSyncTimeout = 0;
  var _layoutObserver = null;
  var _mutedNativeNav = null;

  function findVerticalNav() {
    var navs = document.querySelectorAll('nav');
    for (var i = 0; i < navs.length; i++) {
      var rect = navs[i].getBoundingClientRect();
      if (rect.width > 40 && rect.width < 200 && rect.height > 160 && rect.height > rect.width * 2) {
        return { element: navs[i], rect: rect };
      }
    }
    return null;
  }

  function findLastNavTab(navElement) {
    if (!navElement) return null;

    var candidates = navElement.querySelectorAll('[title], a[href^="#"], button');
    var last = null;

    for (var i = 0; i < candidates.length; i++) {
      var rect = candidates[i].getBoundingClientRect();
      if (rect.width < 20 || rect.height < 20) continue;
      if (!last || rect.bottom > last.rect.bottom) {
        last = { element: candidates[i], rect: rect };
      }
    }

    return last;
  }

  function getCurrentRoute() {
    var route = window.location.hash ? window.location.hash.replace(/^#/, '') : (window.location.pathname || '/');
    route = route.split('?')[0].split('#')[0];
    route = route.replace(/\/+$/, '');
    return route || '/';
  }

  function getNavItemLabel(element) {
    if (!element) return '';

    var fragments = [
      element.getAttribute('title') || '',
      element.getAttribute('aria-label') || '',
      element.textContent || '',
      element.getAttribute('href') || ''
    ];

    return fragments.join(' ').replace(/\s+/g, ' ').trim().toLowerCase();
  }

  function findSelectedNavItem(navElement) {
    if (!navElement) return null;

    var selectors = [
      '.selected',
      '[aria-current="page"]',
      '[aria-current="true"]',
      '[data-active="true"]',
      '[data-selected="true"]'
    ];

    for (var i = 0; i < selectors.length; i++) {
      var selected = navElement.querySelector(selectors[i]);
      if (selected) return selected;
    }

    return null;
  }

  function isHomeSelection(navElement) {
    var selected = findSelectedNavItem(navElement);
    if (!selected) return false;

    var label = getNavItemLabel(selected);
    return label.indexOf('home') !== -1 ||
      label.indexOf('discover') !== -1 ||
      label.indexOf('board') !== -1 ||
      label.indexOf('#/') !== -1 ||
      /(^|\s|\/)\/$/.test(label);
  }

  function shouldShowModsUi(nav) {
    var route = getCurrentRoute().toLowerCase();

    if (/^\/(player|list)(\/|$)/.test(route)) {
      return false;
    }

    if (route === '/' || route === '/home' || route === '/discover' || route === '/board') {
      return true;
    }

    if (!nav) return false;
    return isHomeSelection(nav.element);
  }

  function syncNavWidth() {
    var nav = findVerticalNav();
    var navWidth = nav ? Math.round(nav.rect.width) : 94;
    document.documentElement.style.setProperty('--sl-nav-width', navWidth + 'px');
    return nav;
  }

  function syncPanelPosition() {
    var panel = document.getElementById('sl-mod-panel');
    if (!panel) return;

    var nav = syncNavWidth();
    panel.style.left = nav ? Math.max(0, Math.round(nav.rect.right)) + 'px' : '0px';
  }

  function syncNativeNavSelectionOverride(nav) {
    var nextNav = _panelOpen && nav ? nav.element : null;

    if (_mutedNativeNav && _mutedNativeNav !== nextNav) {
      _mutedNativeNav.removeAttribute('data-sl-mods-muted');
      _mutedNativeNav = null;
    }

    if (nextNav) {
      nextNav.setAttribute('data-sl-mods-muted', '');
      _mutedNativeNav = nextNav;
    }
  }

  function syncModsButtonPosition() {
    var btn = createModsButton();
    if (btn.parentElement !== document.body) {
      document.body.appendChild(btn);
    }

    var nav = syncNavWidth();
    var shouldShow = shouldShowModsUi(nav);
    syncNativeNavSelectionOverride(shouldShow ? nav : null);

    if (!shouldShow) {
      btn.removeAttribute('data-sl-anchor');
      btn.setAttribute('data-sl-ready', 'false');
      btn.style.removeProperty('top');
      btn.style.removeProperty('bottom');
      btn.style.removeProperty('left');
      btn.style.removeProperty('width');

      if (_panelOpen) {
        closePanel();
      }

      syncPanelPosition();
      return;
    }

    if (nav) {
      var lastTab = findLastNavTab(nav.element);
      var navPadding = 10;
      var minTop = Math.round(nav.rect.top + 16);
      var maxTop = Math.round(nav.rect.bottom - 64);
      var desiredTop = lastTab ? Math.round(lastTab.rect.bottom + 12) : minTop;

      btn.setAttribute('data-sl-anchor', 'nav');
      btn.style.left = Math.round(nav.rect.left + navPadding) + 'px';
      btn.style.top = Math.max(minTop, Math.min(desiredTop, maxTop)) + 'px';
      btn.style.bottom = 'auto';
      btn.style.width = Math.max(48, Math.round(nav.rect.width - (navPadding * 2))) + 'px';
    } else {
      btn.setAttribute('data-sl-anchor', 'floating');
      btn.style.removeProperty('top');
      btn.style.removeProperty('width');
      btn.style.left = '1rem';
      btn.style.bottom = '1rem';
    }

    btn.setAttribute('data-sl-ready', 'true');

    syncPanelPosition();
  }

  function scheduleLayoutSync() {
    if (_layoutSyncFrame) {
      window.cancelAnimationFrame(_layoutSyncFrame);
    }
    if (_layoutSyncTimeout) {
      window.clearTimeout(_layoutSyncTimeout);
    }

    _layoutSyncFrame = window.requestAnimationFrame(function() {
      _layoutSyncFrame = 0;
      if (_layoutSyncTimeout) {
        window.clearTimeout(_layoutSyncTimeout);
        _layoutSyncTimeout = 0;
      }
      syncModsButtonPosition();
    });

    _layoutSyncTimeout = window.setTimeout(function() {
      _layoutSyncTimeout = 0;
      if (_layoutSyncFrame) {
        window.cancelAnimationFrame(_layoutSyncFrame);
        _layoutSyncFrame = 0;
      }
      syncModsButtonPosition();
    }, 120);
  }

  function observeLayoutChanges() {
    if (_layoutObserver || !document.body) return;

    _layoutObserver = new MutationObserver(function() {
      scheduleLayoutSync();
    });

    _layoutObserver.observe(document.body, {
      childList: true,
      subtree: true
    });
  }

  function createModsButton() {
    var existing = document.getElementById('sl-mods-btn');
    if (existing) {
      _navButton = existing;
      return existing;
    }

    if (_navButton && !_navButton.isConnected) {
      return _navButton;
    }

    var btn = document.createElement('div');
    btn.id = 'sl-mods-btn';
    btn.setAttribute('data-sl-mods-btn', '');
    btn.setAttribute('data-sl-ready', 'false');
    btn.setAttribute('tabindex', '0');
    btn.setAttribute('title', 'Mods');
    btn.setAttribute('role', 'button');
    btn.setAttribute('aria-label', 'Open mods');

    btn.innerHTML =
      '<div class="sl-mods-icon-wrap">' +
        '<div class="sl-mods-icon-main">' +
          '<div class="sl-mods-icon-glyph sl-mods-icon-outline" aria-hidden="true">' + ICONS.modsOutline + '</div>' +
          '<div class="sl-mods-icon-glyph sl-mods-icon-filled" aria-hidden="true">' + ICONS.mods + '</div>' +
        '</div>' +
      '</div>' +
      '<div class="sl-mods-label">Mods</div>';

    btn.addEventListener('click', function(e) {
      e.stopPropagation();
      togglePanel();
    });

    btn.addEventListener('keydown', function(e) {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        togglePanel();
      }
    });

    _navButton = btn;
    return btn;
  }

  // ============================================
  // Panel
  // ============================================
  var _panelOpen = false;

  function createPanel() {
    if (document.getElementById('sl-mod-panel')) return document.getElementById('sl-mod-panel');

    var panel = document.createElement('div');
    panel.id = 'sl-mod-panel';

    panel.innerHTML =
      '<div class="sl-sidebar">' +
        '<div class="sl-sidebar-title">Mods</div>' +
        '<div class="sl-tab sl-active" data-tab="plugins">' + ICONS.mods + ' Plugins</div>' +
        '<div class="sl-tab" data-tab="themes">' + ICONS.theme + ' Themes</div>' +
        '<div class="sl-tab" data-tab="marketplace">' +
          '<svg viewBox="0 0 24 24" style="fill:currentcolor;width:16px;height:16px;"><path d="M20 4H4v2h16V4zm1 10v-2l-1-5H4l-1 5v2h1v6h10v-6h4v6h2v-6h1zm-9 4H6v-4h6v4z"/></svg>' +
          ' Marketplace' +
        '</div>' +
        '<div class="sl-tab" data-tab="settings">' +
          '<svg viewBox="0 0 24 24" style="fill:currentcolor;width:16px;height:16px;"><path d="M22.7 19l-9.1-9.1c.9-2.3.4-5-1.5-6.9-2-2-5-2.4-7.4-1.3L9 6 6 9 1.6 4.7C.4 7.1.9 10.1 2.9 12.1c1.9 1.9 4.6 2.4 6.9 1.5l9.1 9.1c.4.4 1 .4 1.4 0l2.3-2.3c.5-.4.5-1.1.1-1.4z"/></svg>' +
          ' Settings' +
        '</div>' +
        '<div class="sl-tab" data-tab="about">' +
          '<svg viewBox="0 0 24 24" style="fill:currentcolor;width:16px;height:16px;"><path d="M12 22C6.477 22 2 17.523 2 12S6.477 2 12 2s10 4.477 10 10-4.477 10-10 10zm-1-11v6h2v-6h-2zm0-4v2h2V7h-2z"/></svg>' +
          ' About' +
        '</div>' +
      '</div>' +
      '<div class="sl-content">' +
        '<div class="sl-tab-content sl-visible" data-content="plugins"></div>' +
        '<div class="sl-tab-content" data-content="themes"></div>' +
        '<div class="sl-tab-content" data-content="marketplace"></div>' +
        '<div class="sl-tab-content" data-content="settings"></div>' +
        '<div class="sl-tab-content" data-content="about"></div>' +
      '</div>';

    document.body.appendChild(panel);

    // Tab switching
    panel.querySelectorAll('.sl-tab').forEach(function(tab) {
      tab.addEventListener('click', function() {
        switchTab(tab.getAttribute('data-tab'));
      });
    });

    return panel;
  }

  function switchTab(tabName) {
    var panel = document.getElementById('sl-mod-panel');
    if (!panel) return;

    panel.querySelectorAll('.sl-tab').forEach(function(t) {
      t.classList.toggle('sl-active', t.getAttribute('data-tab') === tabName);
    });

    panel.querySelectorAll('.sl-tab-content').forEach(function(c) {
      c.classList.toggle('sl-visible', c.getAttribute('data-content') === tabName);
    });

    // Lazy-load tab content
    var content = panel.querySelector('[data-content="' + tabName + '"]');
    if (!content || content.hasAttribute('data-loaded')) return;

    content.setAttribute('data-loaded', 'true');
    if (tabName === 'plugins') populatePlugins(content);
    else if (tabName === 'themes') populateThemes(content);
    else if (tabName === 'marketplace') populateMarketplace(content);
    else if (tabName === 'settings') populateSettings(content);
    else if (tabName === 'about') populateAbout(content);
  }

  function openPanel() {
    var panel = createPanel();

    panel.classList.add('sl-open');
    _panelOpen = true;

    if (_navButton) _navButton.setAttribute('data-sl-active', '');

    // Position panel and neutralize the native selected nav item while Mods is open.
    syncModsButtonPosition();

    // Load default tab
    switchTab('plugins');
  }

  function closePanel() {
    var panel = document.getElementById('sl-mod-panel');
    if (!panel) return;
    panel.classList.remove('sl-open');
    _panelOpen = false;
    if (_navButton) _navButton.removeAttribute('data-sl-active');
    syncNativeNavSelectionOverride(null);
  }

  function togglePanel() {
    if (_panelOpen) closePanel();
    else openPanel();
  }

  // ============================================
  // Plugins Tab
  // ============================================
  function populatePlugins(container) {
    container.innerHTML =
      '<div class="sl-section-header"><div class="sl-section-title">Plugins</div></div>' +
      '<div id="sl-plugins-list"></div>';

    var list = container.querySelector('#sl-plugins-list');
    var enabledPlugins = JSON.parse(localStorage.getItem('enabledPlugins') || '[]');

    API.getPlugins().then(function(plugins) {
      if (plugins.length === 0) {
        list.innerHTML = '<div class="sl-empty">No plugins installed. Browse the marketplace to find plugins.</div>';
        return;
      }

      plugins.forEach(function(plugin) {
        if (!plugin.metadata) return;
        var isEnabled = enabledPlugins.indexOf(plugin.filename) !== -1;
        var card = document.createElement('div');
        card.className = 'sl-card';
        card.setAttribute('data-plugin-card', plugin.filename);

        card.innerHTML =
          '<div class="sl-card-info">' +
            '<div class="sl-card-name">' + escapeHtml(plugin.metadata.name) +
              ' <span class="sl-card-version">' + escapeHtml(plugin.metadata.version) + '</span></div>' +
            '<div class="sl-card-desc">' + escapeHtml(plugin.metadata.description) + '</div>' +
            '<div class="sl-card-author">by ' + escapeHtml(plugin.metadata.author) + '</div>' +
          '</div>' +
          '<div class="sl-card-actions">' +
            '<div class="sl-gear-btn" data-plugin-settings="' + escapeHtml(plugin.filename) + '" title="Settings">' +
              ICONS.settings +
            '</div>' +
            '<button class="sl-btn sl-btn-warning" data-plugin-update="' + escapeHtml(plugin.filename) + '" style="display:none;">Update</button>' +
            '<label class="sl-toggle">' +
              '<input type="checkbox" data-plugin-toggle="' + escapeHtml(plugin.filename) + '"' + (isEnabled ? ' checked' : '') + '>' +
              '<div class="sl-toggle-track"><div class="sl-toggle-thumb"></div></div>' +
            '</label>' +
          '</div>';

        list.appendChild(card);

        // Check for registered settings and show gear if available
        var baseName = plugin.filename.replace('.plugin.js', '');
        invoke('get_registered_settings', { pluginName: baseName }).then(function(schema) {
          if (schema && schema !== null && Array.isArray(schema) && schema.length > 0) {
            var gearBtn = card.querySelector('[data-plugin-settings]');
            if (gearBtn) gearBtn.style.display = 'flex';
          }
        });

        checkItemUpdate(plugin.filename);
      });

      // Bind toggle events
      list.querySelectorAll('[data-plugin-toggle]').forEach(function(checkbox) {
        checkbox.addEventListener('change', function() {
          var pluginName = checkbox.getAttribute('data-plugin-toggle');
          if (checkbox.checked) {
            loadPlugin(pluginName);
          } else {
            unloadPlugin(pluginName);
            showReloadWarning(container);
          }
        });
      });

      // Bind card click to toggle plugin
      list.querySelectorAll('[data-plugin-card]').forEach(function(card) {
        card.style.cursor = 'pointer';
        card.addEventListener('click', function(e) {
          // Don't toggle if clicking the toggle itself, gear button, or update button
          if (e.target.closest('.sl-toggle') || e.target.closest('.sl-gear-btn') || e.target.closest('[data-plugin-update]')) return;
          var checkbox = card.querySelector('[data-plugin-toggle]');
          if (checkbox) {
            checkbox.checked = !checkbox.checked;
            checkbox.dispatchEvent(new Event('change'));
          }
        });
      });

      // Bind settings gear clicks
      list.querySelectorAll('[data-plugin-settings]').forEach(function(gear) {
        gear.addEventListener('click', function() {
          openPluginSettingsModal(gear.getAttribute('data-plugin-settings'));
        });
      });
    });
  }

  function loadPlugin(pluginName) {
    if (document.getElementById(pluginName)) return;
    API.getModContent(pluginName, 'plugin').then(function(content) {
      var baseName = pluginName.replace('.plugin.js', '');
      var wrapped = '(function() {\n' +
        'var StremioEnhancedAPI = {\n' +
        '  logger: {\n' +
        '    info: function(m) { window.StremioEnhancedAPI.info("' + baseName + '", m); },\n' +
        '    warn: function(m) { window.StremioEnhancedAPI.warn("' + baseName + '", m); },\n' +
        '    error: function(m) { window.StremioEnhancedAPI.error("' + baseName + '", m); }\n' +
        '  },\n' +
        '  getSetting: function(k) { return window.StremioEnhancedAPI.getSetting("' + baseName + '", k); },\n' +
        '  saveSetting: function(k, v) { return window.StremioEnhancedAPI.saveSetting("' + baseName + '", k, v); },\n' +
        '  registerSettings: function(s) { return window.StremioEnhancedAPI.registerSettings("' + baseName + '", s); },\n' +
        '  onSettingsSaved: function(cb) { return window.StremioEnhancedAPI.onSettingsSaved("' + baseName + '", cb); }\n' +
        '};\n' +
        'try {\n' + content + '\n} catch(err) { console.error("[ModController] Plugin crashed: ' + pluginName + '", err); }\n' +
        '})();';
      var script = document.createElement('script');
      script.id = pluginName;
      script.textContent = wrapped;
      document.body.appendChild(script);
    }).catch(function(e) {
      console.error('[StremioLightning] Failed to load plugin:', pluginName, e);
    });

    var enabled = JSON.parse(localStorage.getItem('enabledPlugins') || '[]');
    if (enabled.indexOf(pluginName) === -1) {
      enabled.push(pluginName);
      localStorage.setItem('enabledPlugins', JSON.stringify(enabled));
    }
  }

  function unloadPlugin(pluginName) {
    var el = document.getElementById(pluginName);
    if (el) el.remove();
    var enabled = JSON.parse(localStorage.getItem('enabledPlugins') || '[]');
    enabled = enabled.filter(function(x) { return x !== pluginName; });
    localStorage.setItem('enabledPlugins', JSON.stringify(enabled));
  }

  function showReloadWarning(container) {
    if (container.querySelector('.sl-reload-warning')) return;
    var warning = document.createElement('div');
    warning.className = 'sl-reload-warning';
    warning.innerHTML = 'Reload is required to fully disable plugins. <a id="sl-reload-link">Click here to reload</a>.';
    container.insertBefore(warning, container.firstChild.nextSibling);
    container.querySelector('#sl-reload-link').addEventListener('click', function() { location.reload(); });
  }

  // ============================================
  // Themes Tab
  // ============================================
  function populateThemes(container) {
    container.innerHTML =
      '<div class="sl-section-header"><div class="sl-section-title">Themes</div></div>' +
      '<div id="sl-themes-list"></div>';

    var list = container.querySelector('#sl-themes-list');
    var currentTheme = localStorage.getItem('currentTheme');
    var isDefault = !currentTheme || currentTheme === 'Default';

    // Default theme card
    var defaultCard = document.createElement('div');
    defaultCard.className = 'sl-card';
    defaultCard.innerHTML =
      '<div class="sl-card-info">' +
        '<div class="sl-card-name">Default</div>' +
        '<div class="sl-card-desc">The built-in Stremio theme</div>' +
      '</div>' +
      '<div class="sl-card-actions">' +
        '<button class="sl-btn ' + (isDefault ? 'sl-btn-applied' : 'sl-btn-primary') + '" data-theme-apply="Default"' + (isDefault ? ' disabled' : '') + '>' +
          (isDefault ? 'Applied' : 'Apply') +
        '</button>' +
      '</div>';
    list.appendChild(defaultCard);

    defaultCard.querySelector('[data-theme-apply]').addEventListener('click', function() {
      API.applyTheme('Default').then(refreshThemeButtons);
    });

    // Installed themes
    API.getThemes().then(function(themes) {
      themes.forEach(function(theme) {
        if (!theme.metadata) return;
        var isApplied = currentTheme === theme.filename;
        var card = document.createElement('div');
        card.className = 'sl-card';
        card.setAttribute('data-theme-card', theme.filename);

        card.innerHTML =
          '<div class="sl-card-info">' +
            '<div class="sl-card-name">' + escapeHtml(theme.metadata.name) +
              ' <span class="sl-card-version">' + escapeHtml(theme.metadata.version) + '</span></div>' +
            '<div class="sl-card-desc">' + escapeHtml(theme.metadata.description) + '</div>' +
            '<div class="sl-card-author">by ' + escapeHtml(theme.metadata.author) + '</div>' +
          '</div>' +
          '<div class="sl-card-actions">' +
            '<button class="sl-btn sl-btn-warning" data-theme-update="' + escapeHtml(theme.filename) + '" style="display:none;">Update</button>' +
            '<button class="sl-btn ' + (isApplied ? 'sl-btn-applied' : 'sl-btn-primary') + '" data-theme-apply="' + escapeHtml(theme.filename) + '"' + (isApplied ? ' disabled' : '') + '>' +
              (isApplied ? 'Applied' : 'Apply') +
            '</button>' +
          '</div>';

        list.appendChild(card);

        card.querySelector('[data-theme-apply]').addEventListener('click', function() {
          API.applyTheme(theme.filename).then(refreshThemeButtons);
        });

        checkItemUpdate(theme.filename);
      });
    });
  }

  function refreshThemeButtons() {
    var currentTheme = localStorage.getItem('currentTheme') || 'Default';
    document.querySelectorAll('[data-theme-apply]').forEach(function(btn) {
      var themeName = btn.getAttribute('data-theme-apply');
      var isApplied = themeName === currentTheme;
      btn.textContent = isApplied ? 'Applied' : 'Apply';
      btn.disabled = isApplied;
      btn.className = 'sl-btn ' + (isApplied ? 'sl-btn-applied' : 'sl-btn-primary');
    });
  }

  // ============================================
  // Marketplace Tab
  // ============================================
  function populateMarketplace(container) {
    container.innerHTML =
      '<div class="sl-section-header"><div class="sl-section-title">Marketplace</div></div>' +
      '<input class="sl-search" id="sl-marketplace-search" type="text" placeholder="Search plugins and themes..." autocomplete="off" spellcheck="false">' +
      '<div class="sl-submit-link">' +
        '<a class="sl-link" href="https://github.com/REVENGE977/stremio-enhanced-registry" target="_blank" rel="noreferrer">Submit your plugins and themes here</a>' +
      '</div>' +
      '<div id="sl-marketplace-list"><div class="sl-empty">Loading marketplace...</div></div>';

    var list = container.querySelector('#sl-marketplace-list');

    API.getRegistry().then(function(registry) {
      Promise.all([API.getPlugins(), API.getThemes()]).then(function(results) {
        var installedPlugins = results[0].map(function(p) { return p.filename; });
        var installedThemes = results[1].map(function(t) { return t.filename; });

        list.innerHTML = '';

        registry.plugins.forEach(function(entry) {
          var fileName = getFileNameFromUrl(entry.download);
          var installed = installedPlugins.indexOf(fileName) !== -1;
          list.appendChild(createMarketplaceCard(entry, 'plugin', installed));
        });

        registry.themes.forEach(function(entry) {
          var fileName = getFileNameFromUrl(entry.download);
          var installed = installedThemes.indexOf(fileName) !== -1;
          list.appendChild(createMarketplaceCard(entry, 'theme', installed));
        });

        setupMarketplaceSearch(container);
      });
    }).catch(function(e) {
      console.error('[StremioLightning] Failed to load registry:', e);
      list.innerHTML = '<div class="sl-empty">Failed to load marketplace. Check your connection.</div>';
    });
  }

  function createMarketplaceCard(entry, type, installed) {
    var card = document.createElement('div');
    card.className = 'sl-card';
    card.setAttribute('data-marketplace-card', '');

    var logoHtml = '';
    if (entry.preview) {
      logoHtml = '<img class="sl-card-logo" src="' + escapeHtml(entry.preview) + '" alt="Preview" loading="lazy">';
    } else {
      logoHtml = '<div class="sl-card-logo-placeholder">' + (type === 'theme' ? ICONS.theme : ICONS.mods) + '</div>';
    }

    card.innerHTML =
      logoHtml +
      '<div class="sl-card-info">' +
        '<div class="sl-card-name">' + escapeHtml(entry.name) +
          ' <span class="sl-card-version">' + escapeHtml(entry.version) + '</span>' +
          ' <span class="sl-badge">' + escapeHtml(type) + '</span></div>' +
        '<div class="sl-card-desc">' + escapeHtml(entry.description || '') + '</div>' +
        '<div class="sl-card-author">by ' + escapeHtml(entry.author) + '</div>' +
      '</div>' +
      '<div class="sl-card-actions">' +
        '<a class="sl-btn sl-btn-ghost" href="' + escapeHtml(entry.repo) + '" target="_blank" rel="noreferrer">' +
          ICONS.github + ' Repo' +
        '</a>' +
        '<button class="sl-btn ' + (installed ? 'sl-btn-danger' : 'sl-btn-primary') + '" data-marketplace-action data-link="' + escapeHtml(entry.download) + '" data-type="' + escapeHtml(type) + '">' +
          (installed ? 'Uninstall' : 'Install') +
        '</button>' +
      '</div>';

    card.querySelector('[data-marketplace-action]').addEventListener('click', function(e) {
      var btn = e.currentTarget;
      var link = btn.getAttribute('data-link');
      var modType = btn.getAttribute('data-type');
      var isInstalled = btn.textContent.trim() === 'Uninstall';

      if (!isInstalled) {
        btn.textContent = 'Installing...';
        btn.disabled = true;
        API.downloadMod(link, modType).then(function() {
          btn.textContent = 'Uninstall';
          btn.disabled = false;
          btn.className = 'sl-btn sl-btn-danger';
        }).catch(function(err) {
          console.error('[StremioLightning] Install failed:', err);
          btn.textContent = 'Install';
          btn.disabled = false;
        });
      } else {
        var fileName = getFileNameFromUrl(link);
        btn.textContent = 'Removing...';
        btn.disabled = true;
        API.deleteMod(fileName, modType).then(function() {
          if (modType === 'plugin') {
            var enabled = JSON.parse(localStorage.getItem('enabledPlugins') || '[]');
            enabled = enabled.filter(function(x) { return x !== fileName; });
            localStorage.setItem('enabledPlugins', JSON.stringify(enabled));
          }
          if (modType === 'theme' && localStorage.getItem('currentTheme') === fileName) {
            API.applyTheme('Default');
          }
          btn.textContent = 'Install';
          btn.disabled = false;
          btn.className = 'sl-btn sl-btn-primary';
        }).catch(function(err) {
          console.error('[StremioLightning] Uninstall failed:', err);
          btn.textContent = 'Uninstall';
          btn.disabled = false;
        });
      }
    });

    return card;
  }

  function setupMarketplaceSearch(container) {
    var searchInput = container.querySelector('#sl-marketplace-search');
    var list = container.querySelector('#sl-marketplace-list');
    if (!searchInput || !list) return;

    searchInput.addEventListener('input', function() {
      var filter = searchInput.value.trim().toLowerCase();
      var cards = list.querySelectorAll('[data-marketplace-card]');
      cards.forEach(function(card) {
        var text = card.textContent.toLowerCase();
        card.style.display = text.indexOf(filter) !== -1 ? '' : 'none';
      });
    });
  }

  // ============================================
  // Settings Tab
  // ============================================
  function populateSettings(container) {
    var blurVal = parseInt(localStorage.getItem('sl-blur-intensity') || '100', 10);
    var blurEnabled = localStorage.getItem('sl-blur-enabled') !== 'false';
    container.innerHTML =
      '<div style="max-width:35rem;">' +
        '<div class="sl-section-header"><div class="sl-section-title">Settings</div></div>' +
        '<h3 style="margin:0 0 0.75rem; font-size:1.1rem; font-weight:500; color:var(--primary-foreground-color, #f2f2f2); opacity:0.6;">Blur Effect</h3>' +
        '<div class="sl-setting-row">' +
          '<div class="sl-setting-label">' +
            '<div class="sl-setting-label-text">Backdrop Blur</div>' +
            '<div class="sl-setting-label-desc">Enable or disable the backdrop blur effect</div>' +
          '</div>' +
          '<div class="sl-setting-control">' +
            '<label class="sl-toggle">' +
              '<input type="checkbox" id="sl-blur-toggle"' + (blurEnabled ? ' checked' : '') + '>' +
              '<div class="sl-toggle-track"><div class="sl-toggle-thumb"></div></div>' +
            '</label>' +
          '</div>' +
        '</div>' +
        '<div class="sl-setting-row" id="sl-blur-intensity-row"' + (blurEnabled ? '' : ' style="opacity:0.4; pointer-events:none;"') + '>' +
          '<div class="sl-setting-label">' +
            '<div class="sl-setting-label-text">Blur Intensity</div>' +
            '<div class="sl-setting-label-desc">Controls the backdrop blur strength of the mods panel</div>' +
          '</div>' +
          '<div class="sl-setting-control" style="display:flex; align-items:center; gap:0.75rem;">' +
            '<input class="sl-setting-range" type="range" id="sl-blur-range" min="0" max="100" value="' + blurVal + '">' +
            '<span class="sl-range-value" id="sl-blur-value">' + blurVal + '%</span>' +
          '</div>' +
        '</div>' +
      '</div>';

    var toggle = document.getElementById('sl-blur-toggle');
    var intensityRow = document.getElementById('sl-blur-intensity-row');
    var range = document.getElementById('sl-blur-range');
    var label = document.getElementById('sl-blur-value');

    toggle.addEventListener('change', function() {
      var enabled = toggle.checked;
      localStorage.setItem('sl-blur-enabled', enabled);
      intensityRow.style.opacity = enabled ? '' : '0.4';
      intensityRow.style.pointerEvents = enabled ? '' : 'none';
      applyBlurIntensity(parseInt(range.value, 10), enabled);
    });

    range.addEventListener('input', function() {
      label.textContent = range.value + '%';
      applyBlurIntensity(parseInt(range.value, 10), true);
      localStorage.setItem('sl-blur-intensity', range.value);
    });
  }

  // ============================================
  // About Tab
  // ============================================
  function populateAbout(container) {
    container.innerHTML =
      '<div class="sl-about">' +
        '<h2>Stremio Lightning</h2>' +
        '<p>A lightweight Stremio desktop client built with Tauri.<br>' +
        'Supports plugins and themes from the Stremio Enhanced ecosystem.</p>' +
        '<a class="sl-btn sl-btn-ghost" href="https://github.com/theguy000/stremio-lightning" target="_blank" rel="noreferrer">' +
          ICONS.github + ' GitHub Repository' +
        '</a>' +
      '</div>';
  }

  // ============================================
  // Plugin Settings Modal
  // ============================================
  function buildToggleSetting(setting, currentValue) {
    var isChecked = currentValue === true || currentValue === 'true';
    return '<div class="sl-setting-row">' +
      '<div class="sl-setting-label">' +
        '<div class="sl-setting-label-text">' + escapeHtml(setting.label) + '</div>' +
        (setting.description ? '<div class="sl-setting-label-desc">' + escapeHtml(setting.description) + '</div>' : '') +
      '</div>' +
      '<div class="sl-setting-control">' +
        '<label class="sl-toggle">' +
          '<input type="checkbox" data-setting-key="' + escapeHtml(setting.key) + '" data-setting-type="toggle"' + (isChecked ? ' checked' : '') + '>' +
          '<div class="sl-toggle-track"><div class="sl-toggle-thumb"></div></div>' +
        '</label>' +
      '</div>' +
    '</div>';
  }

  function buildInputSetting(setting, currentValue) {
    return '<div class="sl-setting-row">' +
      '<div class="sl-setting-label">' +
        '<div class="sl-setting-label-text">' + escapeHtml(setting.label) + '</div>' +
        (setting.description ? '<div class="sl-setting-label-desc">' + escapeHtml(setting.description) + '</div>' : '') +
      '</div>' +
      '<div class="sl-setting-control">' +
        '<input class="sl-setting-input" type="text" data-setting-key="' + escapeHtml(setting.key) + '" data-setting-type="input" placeholder="' + escapeHtml(setting.label) + '" value="' + escapeHtml(String(currentValue || '')) + '">' +
      '</div>' +
    '</div>';
  }

  function buildSelectSetting(setting, currentValue) {
    var optionsHtml = '';
    if (setting.options) {
      setting.options.forEach(function(opt) {
        var selected = (String(opt.value) === String(currentValue)) ? ' selected' : '';
        optionsHtml += '<option value="' + escapeHtml(String(opt.value)) + '"' + selected + '>' + escapeHtml(opt.label) + '</option>';
      });
    }
    return '<div class="sl-setting-row">' +
      '<div class="sl-setting-label">' +
        '<div class="sl-setting-label-text">' + escapeHtml(setting.label) + '</div>' +
        (setting.description ? '<div class="sl-setting-label-desc">' + escapeHtml(setting.description) + '</div>' : '') +
      '</div>' +
      '<div class="sl-setting-control">' +
        '<select class="sl-setting-select" data-setting-key="' + escapeHtml(setting.key) + '" data-setting-type="select">' +
          optionsHtml +
        '</select>' +
      '</div>' +
    '</div>';
  }

  function openPluginSettingsModal(pluginName) {
    var pluginBaseName = pluginName.replace('.plugin.js', '');
    var modalId = 'sl-modal-' + pluginBaseName.replace(/[^a-zA-Z0-9]/g, '');

    var existing = document.getElementById(modalId);
    if (existing) existing.remove();

    invoke('get_registered_settings', { pluginName: pluginBaseName }).then(function(schema) {
      if (!schema || schema === null) return;
      var settingsArr = Array.isArray(schema) ? schema : [];
      if (settingsArr.length === 0) return;

      var promises = settingsArr.map(function(setting) {
        return invoke('get_setting', { pluginName: pluginBaseName, key: setting.key }).then(function(val) {
          return { key: setting.key, value: (val === null || val === undefined) ? setting.defaultValue : val };
        });
      });

      Promise.all(promises).then(function(results) {
        var currentValues = {};
        results.forEach(function(r) { currentValues[r.key] = r.value; });

        var settingsHtml = '';
        settingsArr.forEach(function(setting) {
          var val = currentValues[setting.key] !== undefined ? currentValues[setting.key] : '';
          if (setting.type === 'toggle') settingsHtml += buildToggleSetting(setting, val);
          else if (setting.type === 'input') settingsHtml += buildInputSetting(setting, val);
          else if (setting.type === 'select') settingsHtml += buildSelectSetting(setting, val);
        });

        var modalHtml =
          '<div class="sl-modal-overlay" id="' + modalId + '">' +
            '<div class="sl-modal">' +
              '<div class="sl-modal-header">' +
                '<div class="sl-modal-title">' + escapeHtml(pluginBaseName) + ' Settings</div>' +
                '<div class="sl-modal-close" data-close-modal>' + ICONS.close + '</div>' +
              '</div>' +
              '<div class="sl-modal-body">' + settingsHtml + '</div>' +
              '<div class="sl-modal-footer">' +
                '<button class="sl-btn sl-btn-primary" data-close-modal>Close</button>' +
              '</div>' +
            '</div>' +
          '</div>';

        document.body.insertAdjacentHTML('beforeend', modalHtml);
        var modalEl = document.getElementById(modalId);
        if (!modalEl) return;

        function handleClose() {
          settingsArr.forEach(function(setting) {
            var el = modalEl.querySelector('[data-setting-key="' + setting.key + '"]');
            if (!el) return;
            var value;
            var type = el.getAttribute('data-setting-type');
            if (type === 'toggle') value = el.checked;
            else if (type === 'input' || type === 'select') value = el.value;
            if (value !== undefined) {
              API.saveSetting(pluginBaseName, setting.key, value);
            }
          });
          API._notifySettingsSaved(pluginBaseName, currentValues);
          modalEl.remove();
        }

        modalEl.querySelectorAll('[data-close-modal]').forEach(function(btn) {
          btn.addEventListener('click', handleClose);
        });

        modalEl.addEventListener('click', function(e) {
          if (e.target === modalEl) handleClose();
        });
      });
    });
  }

  // ============================================
  // Update Checks
  // ============================================
  function checkItemUpdate(fileName) {
    var type = fileName.indexOf('.theme.css') !== -1 ? 'theme' : 'plugin';
    API.checkModUpdates(fileName, type).then(function(info) {
      if (!info || !info.has_update) return;

      var updateBtn = document.querySelector('[data-plugin-update="' + fileName + '"]') ||
                      document.querySelector('[data-theme-update="' + fileName + '"]');
      if (!updateBtn) return;

      updateBtn.style.display = 'inline-flex';
      updateBtn.addEventListener('click', function() {
        updateBtn.textContent = 'Updating...';
        updateBtn.disabled = true;
        API.downloadMod(info.update_url, type).then(function() {
          updateBtn.textContent = 'Updated!';
          var panel = document.getElementById('sl-mod-panel');
          if (panel) {
            var content = panel.querySelector('[data-content="' + (type === 'plugin' ? 'plugins' : 'themes') + '"]');
            if (content) {
              content.removeAttribute('data-loaded');
              switchTab(type === 'plugin' ? 'plugins' : 'themes');
            }
          }
        }).catch(function() {
          updateBtn.textContent = 'Update';
          updateBtn.disabled = false;
        });
      });
    }).catch(function() {});
  }

  // ============================================
  // External Link Handler
  // ============================================
  document.addEventListener('click', function(e) {
    var link = e.target.closest('a[href]');
    if (!link) return;
    var href = link.getAttribute('href');
    if (!href || href.charAt(0) === '#') return;
    if (href.indexOf('http://') === 0 || href.indexOf('https://') === 0) {
      e.preventDefault();
      e.stopPropagation();
      invoke('open_external_url', { url: href });
    }
  }, true);

  // ============================================
  // Init
  // ============================================
  function init() {
    injectStyles();
    var blurEnabled = localStorage.getItem('sl-blur-enabled') !== 'false';
    applyBlurIntensity(parseInt(localStorage.getItem('sl-blur-intensity') || '100', 10), blurEnabled);
    createModsButton();
    syncModsButtonPosition();

    // Close panel when navigating
    window.addEventListener('hashchange', function() {
      closePanel();
      scheduleLayoutSync();
    });

    // Close panel when clicking Stremio nav items
    document.addEventListener('click', function(e) {
      if (!_panelOpen) return;
      var navItem = e.target.closest('nav [title]');
      if (navItem) {
        closePanel();
      }
    });

    document.addEventListener('keydown', function(e) {
      if (e.key === 'Escape' && _panelOpen) {
        closePanel();
      }
    });

    // Re-sync nav width if layout changes
    window.addEventListener('resize', function() {
      scheduleLayoutSync();
    });

    // Re-apply blur intensity when theme changes so panel background updates
    window.addEventListener('sl-theme-changed', function() {
      var blurEnabled = localStorage.getItem('sl-blur-enabled') !== 'false';
      applyBlurIntensity(parseInt(localStorage.getItem('sl-blur-intensity') || '100', 10), blurEnabled);
    });

    observeLayoutChanges();
    scheduleLayoutSync();
  }

  var _initialized = false;

  function start() {
    if (_initialized) return;
    _initialized = true;
    init();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start, { once: true });
  } else {
    start();
  }

  window.addEventListener('load', function() {
    if (_initialized) {
      scheduleLayoutSync();
      return;
    }

    start();
  }, { once: true });
})();
