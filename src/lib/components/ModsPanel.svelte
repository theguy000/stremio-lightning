<script lang="ts">
  import { onMount } from 'svelte';
  import { openExternalUrl } from '../ipc';
  import { ICONS } from '../icons';
  import PluginsTab from './PluginsTab.svelte';
  import ThemesTab from './ThemesTab.svelte';
  import MarketplaceTab from './MarketplaceTab.svelte';
  import SettingsTab from './SettingsTab.svelte';
  import LogsTab from './LogsTab.svelte';
  import AboutTab from './AboutTab.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let activeTab = $state('plugins');

  const tabs = [
    { id: 'plugins', label: 'Plugins', icon: ICONS.plugin },
    { id: 'themes', label: 'Themes', icon: ICONS.theme },
    { id: 'marketplace', label: 'Marketplace', icon: ICONS.marketplace },
    { id: 'settings', label: 'Settings', icon: ICONS.wrench },
    { id: 'logs', label: 'Logs', icon: ICONS.logs },
    { id: 'about', label: 'About', icon: ICONS.info },
  ];

  function switchTab(tabId: string) {
    activeTab = tabId;
  }

  // Emit custom event for bridge.js Discord tracker
  $effect(() => {
    window.dispatchEvent(new CustomEvent('sl-mods-panel', { detail: open }));
  });

  // Close on Escape
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) {
      onclose();
    }
  }

  // Close on nav item click
  function handleDocClick(e: MouseEvent) {
    if (!open) return;
    const navItem = (e.target as Element)?.closest?.('nav [title]');
    if (navItem) onclose();
  }

  // Close on hash change
  function handleHashChange() {
    if (open) onclose();
  }

  // External link interception
  function handleLinkClick(e: MouseEvent) {
    const link = (e.target as Element)?.closest?.('a[href]') as HTMLAnchorElement | null;
    if (!link) return;
    const href = link.getAttribute('href');
    if (!href || href.charAt(0) === '#') return;
    if (href.startsWith('http://') || href.startsWith('https://')) {
      e.preventDefault();
      e.stopPropagation();
      openExternalUrl(href);
    }
  }

  onMount(() => {
    document.addEventListener('keydown', handleKeydown);
    document.addEventListener('click', handleDocClick);
    window.addEventListener('hashchange', handleHashChange);

    return () => {
      document.removeEventListener('keydown', handleKeydown);
      document.removeEventListener('click', handleDocClick);
      window.removeEventListener('hashchange', handleHashChange);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div id="sl-mod-panel" class:sl-open={open} onclick={handleLinkClick}>
  <div class="sl-sidebar">
    <div class="sl-sidebar-title">Mods</div>
    <div class="sl-tab-list" role="tablist" aria-label="Mods sections">
      {#each tabs as tab}
        <div
          id={`sl-tab-${tab.id}`}
          class="sl-tab"
          class:sl-active={activeTab === tab.id}
          onclick={() => switchTab(tab.id)}
          role="tab"
          aria-selected={activeTab === tab.id}
          aria-controls={`sl-panel-${tab.id}`}
          tabindex="0"
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') switchTab(tab.id); }}
        >
          {@html tab.icon}
          {tab.label}
        </div>
      {/each}
    </div>
  </div>
  <div class="sl-content">
    <div id="sl-panel-plugins" class="sl-tab-content" class:sl-visible={activeTab === 'plugins'} role="tabpanel" aria-labelledby="sl-tab-plugins">
      <PluginsTab />
    </div>
    <div id="sl-panel-themes" class="sl-tab-content" class:sl-visible={activeTab === 'themes'} role="tabpanel" aria-labelledby="sl-tab-themes">
      <ThemesTab />
    </div>
    <div id="sl-panel-marketplace" class="sl-tab-content" class:sl-visible={activeTab === 'marketplace'} role="tabpanel" aria-labelledby="sl-tab-marketplace">
      <MarketplaceTab />
    </div>
    <div id="sl-panel-settings" class="sl-tab-content" class:sl-visible={activeTab === 'settings'} role="tabpanel" aria-labelledby="sl-tab-settings">
      <SettingsTab />
    </div>
    <div id="sl-panel-logs" class="sl-tab-content" class:sl-visible={activeTab === 'logs'} role="tabpanel" aria-labelledby="sl-tab-logs">
      <LogsTab active={open && activeTab === 'logs'} />
    </div>
    <div id="sl-panel-about" class="sl-tab-content" class:sl-visible={activeTab === 'about'} role="tabpanel" aria-labelledby="sl-tab-about">
      <AboutTab />
    </div>
  </div>
</div>
