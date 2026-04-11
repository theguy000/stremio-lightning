<script lang="ts">
  import { onMount } from 'svelte';
  import { registry, installedPlugins, installedThemes, marketplaceLoading, refreshMarketplace, installMod, uninstallMod, isInstalled } from '../stores/marketplace';
  import { ICONS } from '../icons';
  import type { Registry, RegistryEntry, InstalledMod } from '../types';

  let reg: Registry | null = $state(null);
  let instPlugins: InstalledMod[] = $state([]);
  let instThemes: InstalledMod[] = $state([]);
  let loading = $state(true);
  let searchFilter = $state('');

  // Track install/uninstall in-progress
  let actionInProgress: Record<string, string> = $state({});

  registry.subscribe((v) => { reg = v; });
  installedPlugins.subscribe((v) => { instPlugins = v; });
  installedThemes.subscribe((v) => { instThemes = v; });
  marketplaceLoading.subscribe((v) => { loading = v; });

  function getFileNameFromUrl(url: string) {
    return (url.split('/').pop() || '').split('?')[0];
  }

  function matchesSearch(entry: RegistryEntry) {
    if (!searchFilter) return true;
    const text = `${entry.name} ${entry.description || ''} ${entry.author} ${entry.version}`.toLowerCase();
    return text.includes(searchFilter.toLowerCase());
  }

  async function handleAction(entry: RegistryEntry, type: 'plugin' | 'theme') {
    const installed = type === 'plugin' ? instPlugins : instThemes;
    const match = isInstalled(entry.download, installed);
    const key = entry.download;

    if (match) {
      actionInProgress[key] = 'Removing...';
      actionInProgress = { ...actionInProgress };
      try {
        await uninstallMod(match.filename, type);
      } catch (e) {
        console.error('Uninstall failed:', e);
      }
    } else {
      actionInProgress[key] = 'Installing...';
      actionInProgress = { ...actionInProgress };
      try {
        await installMod(entry, type);
      } catch (e) {
        console.error('Install failed:', e);
      }
    }
    delete actionInProgress[key];
    actionInProgress = { ...actionInProgress };
  }

  onMount(async () => {
    await refreshMarketplace();
  });
</script>

<div class="sl-section-header">
  <div class="sl-section-title">Marketplace</div>
</div>

<input
  class="sl-search"
  type="text"
  placeholder="Search plugins and themes..."
  autocomplete="off"
  spellcheck="false"
  bind:value={searchFilter}
/>

<div class="sl-submit-link">
  <a class="sl-link" href="https://github.com/REVENGE977/stremio-enhanced-registry" target="_blank" rel="noreferrer">Submit your plugins and themes here</a>
</div>

{#if loading}
  <div class="sl-empty">Loading marketplace...</div>
{:else if !reg}
  <div class="sl-empty">Failed to load marketplace. Check your connection.</div>
{:else}
  {#each reg.plugins as entry}
    {#if matchesSearch(entry)}
      {@const match = isInstalled(entry.download, instPlugins)}
      {@const isInst = !!match}
      {@const inProgress = actionInProgress[entry.download]}
      <div class="sl-card">
        {#if entry.preview}
          <img class="sl-card-logo" src={entry.preview} alt="Preview" loading="lazy" />
        {:else}
          <div class="sl-card-logo-placeholder">{@html ICONS.mods}</div>
        {/if}
        <div class="sl-card-info">
          <div class="sl-card-name">
            {entry.name}
            <span class="sl-card-version">{entry.version}</span>
            <span class="sl-badge">plugin</span>
          </div>
          <div class="sl-card-desc">{entry.description || ''}</div>
          <div class="sl-card-author">by {entry.author}</div>
        </div>
        <div class="sl-card-actions">
          <a class="sl-btn sl-btn-ghost" href={entry.repo} target="_blank" rel="noreferrer">
            {@html ICONS.github} Repo
          </a>
          <button
            class="sl-btn {isInst ? 'sl-btn-danger' : 'sl-btn-primary'}"
            disabled={!!inProgress}
            onclick={() => handleAction(entry, 'plugin')}
          >
            {inProgress || (isInst ? 'Uninstall' : 'Install')}
          </button>
        </div>
      </div>
    {/if}
  {/each}

  {#each reg.themes as entry}
    {#if matchesSearch(entry)}
      {@const match = isInstalled(entry.download, instThemes)}
      {@const isInst = !!match}
      {@const inProgress = actionInProgress[entry.download]}
      <div class="sl-card">
        {#if entry.preview}
          <img class="sl-card-logo" src={entry.preview} alt="Preview" loading="lazy" />
        {:else}
          <div class="sl-card-logo-placeholder">{@html ICONS.theme}</div>
        {/if}
        <div class="sl-card-info">
          <div class="sl-card-name">
            {entry.name}
            <span class="sl-card-version">{entry.version}</span>
            <span class="sl-badge">theme</span>
          </div>
          <div class="sl-card-desc">{entry.description || ''}</div>
          <div class="sl-card-author">by {entry.author}</div>
        </div>
        <div class="sl-card-actions">
          <a class="sl-btn sl-btn-ghost" href={entry.repo} target="_blank" rel="noreferrer">
            {@html ICONS.github} Repo
          </a>
          <button
            class="sl-btn {isInst ? 'sl-btn-danger' : 'sl-btn-primary'}"
            disabled={!!inProgress}
            onclick={() => handleAction(entry, 'theme')}
          >
            {inProgress || (isInst ? 'Uninstall' : 'Install')}
          </button>
        </div>
      </div>
    {/if}
  {/each}
{/if}
