<script lang="ts">
  import { onMount } from 'svelte';
  import { ICONS } from '../icons';

  interface Props {
    active: boolean;
    ontoggle: () => void;
  }

  let { active, ontoggle }: Props = $props();

  let buttonEl: HTMLDivElement | undefined = $state();
  let anchorMode: 'nav' | 'floating' | '' = $state('');
  let ready = $state(false);
  let btnStyle = $state('');

  let _layoutSyncFrame = 0;
  let _layoutSyncTimeout = 0;
  let _layoutObserver: MutationObserver | null = null;
  let _mutedNativeNav: HTMLElement | null = null;

  function findVerticalNav() {
    const navs = document.querySelectorAll('nav');
    for (let i = 0; i < navs.length; i++) {
      const rect = navs[i].getBoundingClientRect();
      if (rect.width > 40 && rect.width < 200 && rect.height > 160 && rect.height > rect.width * 2) {
        return { element: navs[i], rect };
      }
    }
    return null;
  }

  function findLastNavTab(navElement: HTMLElement) {
    const candidates = navElement.querySelectorAll('[title], a[href^="#"], button');
    let last: { element: Element; rect: DOMRect } | null = null;
    for (let i = 0; i < candidates.length; i++) {
      const rect = candidates[i].getBoundingClientRect();
      if (rect.width < 20 || rect.height < 20) continue;
      if (!last || rect.bottom > last.rect.bottom) {
        last = { element: candidates[i], rect };
      }
    }
    return last;
  }

  function getCurrentRoute() {
    let route = window.location.hash ? window.location.hash.replace(/^#/, '') : (window.location.pathname || '/');
    route = route.split('?')[0].split('#')[0];
    route = route.replace(/\/+$/, '');
    return route || '/';
  }

  function shouldShowModsUi() {
    const route = getCurrentRoute().toLowerCase();
    return !/^\/(player|list)(\/|$)/.test(route);
  }

  function syncNavWidth() {
    const nav = findVerticalNav();
    const navWidth = nav ? Math.round(nav.rect.width) : 94;
    document.documentElement.style.setProperty('--sl-nav-width', navWidth + 'px');
    return nav;
  }

  function syncPanelPosition() {
    const panel = document.getElementById('sl-mod-panel');
    if (!panel) return;
    const nav = syncNavWidth();
    panel.style.left = nav ? Math.max(0, Math.round(nav.rect.right)) + 'px' : '0px';
  }

  function syncNativeNavSelectionOverride(nav: { element: HTMLElement; rect: DOMRect } | null) {
    const nextNav = active && nav ? nav.element : null;
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
    if (!buttonEl) return;

    const nav = syncNavWidth();
    const shouldShow = shouldShowModsUi();
    syncNativeNavSelectionOverride(shouldShow ? nav : null);

    if (!shouldShow) {
      anchorMode = '';
      ready = false;
      btnStyle = '';
      syncPanelPosition();
      return;
    }

    if (nav) {
      const lastTab = findLastNavTab(nav.element);
      const navPadding = 10;
      const minTop = Math.round(nav.rect.top + 16);
      const maxTop = Math.round(nav.rect.bottom - 64);
      const desiredTop = lastTab ? Math.round(lastTab.rect.bottom + 12) : minTop;

      anchorMode = 'nav';
      const top = Math.max(minTop, Math.min(desiredTop, maxTop));
      const left = Math.round(nav.rect.left + navPadding);
      const width = Math.max(48, Math.round(nav.rect.width - (navPadding * 2)));
      btnStyle = `left:${left}px;top:${top}px;bottom:auto;width:${width}px;`;
    } else {
      anchorMode = 'floating';
      btnStyle = 'left:1rem;bottom:1rem;';
    }

    ready = true;
    syncPanelPosition();
  }

  function scheduleLayoutSync() {
    if (_layoutSyncFrame) window.cancelAnimationFrame(_layoutSyncFrame);
    if (_layoutSyncTimeout) window.clearTimeout(_layoutSyncTimeout);

    _layoutSyncFrame = window.requestAnimationFrame(() => {
      _layoutSyncFrame = 0;
      if (_layoutSyncTimeout) { window.clearTimeout(_layoutSyncTimeout); _layoutSyncTimeout = 0; }
      syncModsButtonPosition();
    });

    _layoutSyncTimeout = window.setTimeout(() => {
      _layoutSyncTimeout = 0;
      if (_layoutSyncFrame) { window.cancelAnimationFrame(_layoutSyncFrame); _layoutSyncFrame = 0; }
      syncModsButtonPosition();
    }, 120);
  }

  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    ontoggle();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      ontoggle();
    }
  }

  // Re-sync when active state changes
  $effect(() => {
    // Read active to track it
    void active;
    syncModsButtonPosition();
  });

  onMount(() => {
    syncModsButtonPosition();

    // Observe DOM mutations for layout changes
    if (document.body) {
      _layoutObserver = new MutationObserver(() => scheduleLayoutSync());
      _layoutObserver.observe(document.body, { childList: true, subtree: true });
    }

    window.addEventListener('resize', scheduleLayoutSync);
    window.addEventListener('hashchange', scheduleLayoutSync);

    return () => {
      _layoutObserver?.disconnect();
      window.removeEventListener('resize', scheduleLayoutSync);
      window.removeEventListener('hashchange', scheduleLayoutSync);
      // Clean up muted nav
      if (_mutedNativeNav) {
        _mutedNativeNav.removeAttribute('data-sl-mods-muted');
        _mutedNativeNav = null;
      }
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={buttonEl}
  id="sl-mods-btn"
  data-sl-mods-btn
  data-sl-ready={ready ? 'true' : 'false'}
  data-sl-anchor={anchorMode || undefined}
  data-sl-active={active ? '' : undefined}
  tabindex="0"
  title="Mods"
  role="button"
  aria-label="Open mods"
  style={btnStyle}
  onclick={handleClick}
  onkeydown={handleKeydown}
>
  <div class="sl-mods-icon-wrap">
    <div class="sl-mods-icon-main">
      <div class="sl-mods-icon-glyph sl-mods-icon-outline" aria-hidden="true">{@html ICONS.modsOutline}</div>
      <div class="sl-mods-icon-glyph sl-mods-icon-filled" aria-hidden="true">{@html ICONS.mods}</div>
    </div>
  </div>
  <div class="sl-mods-label">Mods</div>
</div>
