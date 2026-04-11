<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { themes, currentTheme, refreshThemes, applyTheme } from '../stores/themes';
  import { checkModUpdates, downloadMod } from '../ipc';
  import type { InstalledMod, UpdateInfo } from '../types';

  let themeList: InstalledMod[] = $state([]);
  let activeTheme: string = $state('');
  let updates: Record<string, UpdateInfo> = $state({});

  themes.subscribe((v) => { themeList = v; });
  currentTheme.subscribe((v) => { activeTheme = v; });

  function isDefault() {
    return !activeTheme || activeTheme === 'Default';
  }

  async function handleApply(filename: string) {
    await applyTheme(filename);
  }

  async function handleUpdate(filename: string, info: UpdateInfo) {
    if (!info.update_url) return;
    await downloadMod(info.update_url, 'theme');
    await refreshThemes();
    delete updates[filename];
    updates = { ...updates };
  }

  onMount(async () => {
    await refreshThemes();

    const list = get(themes);
    for (const theme of list) {
      if (!theme.metadata) continue;
      try {
        const info = await checkModUpdates(theme.filename, 'theme');
        if (info?.has_update) {
          updates[theme.filename] = info;
          updates = { ...updates };
        }
      } catch { /* ignore */ }
    }
  });
</script>

<div class="sl-section-header">
  <div class="sl-section-title">Themes</div>
</div>

<!-- Default theme card -->
<div class="sl-card">
  <div class="sl-card-info">
    <div class="sl-card-name">Default</div>
    <div class="sl-card-desc">The built-in Stremio theme</div>
  </div>
  <div class="sl-card-actions">
    <button
      class="sl-btn {isDefault() ? 'sl-btn-applied' : 'sl-btn-primary'}"
      disabled={isDefault()}
      onclick={() => handleApply('Default')}
    >
      {isDefault() ? 'Applied' : 'Apply'}
    </button>
  </div>
</div>

{#each themeList as theme}
  {#if theme.metadata}
    {@const isApplied = activeTheme === theme.filename}
    <div class="sl-card">
      <div class="sl-card-info">
        <div class="sl-card-name">
          {theme.metadata.name}
          <span class="sl-card-version">{theme.metadata.version}</span>
        </div>
        <div class="sl-card-desc">{theme.metadata.description}</div>
        <div class="sl-card-author">by {theme.metadata.author}</div>
      </div>
      <div class="sl-card-actions">
        {#if updates[theme.filename]}
          <button class="sl-btn sl-btn-warning" onclick={() => handleUpdate(theme.filename, updates[theme.filename])}>Update</button>
        {/if}
        <button
          class="sl-btn {isApplied ? 'sl-btn-applied' : 'sl-btn-primary'}"
          disabled={isApplied}
          onclick={() => handleApply(theme.filename)}
        >
          {isApplied ? 'Applied' : 'Apply'}
        </button>
      </div>
    </div>
  {/if}
{/each}
