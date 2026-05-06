<script lang="ts">
  import { onMount } from 'svelte';
  import { openExternalUrl } from '../ipc';
  import { ICONS } from '../icons';
  import PluginsTab from './PluginsTab.svelte';
  import ThemesTab from './ThemesTab.svelte';
  import MarketplaceTab from './MarketplaceTab.svelte';
  import SettingsTab from './SettingsTab.svelte';
  import AboutTab from './AboutTab.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let activeTab = $state('plugins');

  const tabs = [
    { id: 'plugins', label: 'Plugins', icon: ICONS.mods },
    { id: 'themes', label: 'Themes', icon: ICONS.theme },
    { id: 'marketplace', label: 'Marketplace', icon: ICONS.marketplace },
    { id: 'settings', label: 'Settings', icon: ICONS.wrench },
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
    {#each tabs as tab}
      <div
        class="sl-tab"
        class:sl-active={activeTab === tab.id}
        onclick={() => switchTab(tab.id)}
        role="tab"
        tabindex="0"
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') switchTab(tab.id); }}
      >
        {@html tab.icon}
        {tab.label}
      </div>
    {/each}
  </div>
  <div class="sl-content">
    <div class="sl-tab-content" class:sl-visible={activeTab === 'plugins'}>
      {#if activeTab === 'plugins' || activeTab === 'plugins'}
        <PluginsTab />
      {/if}
    </div>
    <div class="sl-tab-content" class:sl-visible={activeTab === 'themes'}>
      {#if activeTab === 'themes'}
        <ThemesTab />
      {/if}
    </div>
    <div class="sl-tab-content" class:sl-visible={activeTab === 'marketplace'}>
      {#if activeTab === 'marketplace'}
        <MarketplaceTab />
      {/if}
    </div>
    <div class="sl-tab-content" class:sl-visible={activeTab === 'settings'}>
      {#if activeTab === 'settings'}
        <SettingsTab />
      {/if}
    </div>
    <div class="sl-tab-content" class:sl-visible={activeTab === 'about'}>
      {#if activeTab === 'about'}
        <AboutTab />
      {/if}
    </div>
  </div>
</div>
