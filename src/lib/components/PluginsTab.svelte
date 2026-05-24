<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { plugins, enabledPlugins, refreshPlugins, loadPlugin, unloadPlugin, loadEnabledFromStorage } from '../stores/plugins';
  import { checkModUpdates, downloadMod, getRegisteredSettings } from '../ipc';
  import { ICONS } from '../icons';
  import PluginSettingsModal from './PluginSettingsModal.svelte';
  import type { InstalledMod, UpdateInfo } from '../types';

  let pluginList: InstalledMod[] = $state([]);
  let enabled: string[] = $state([]);
  let showReloadWarning = $state(false);
  let updates: Record<string, UpdateInfo> = $state({});
  let hasSettings: Record<string, boolean> = $state({});

  // Settings modal state
  let settingsModalPlugin: string | null = $state(null);
  let settingsModalSchema: any[] | null = $state(null);

  plugins.subscribe((v) => { pluginList = v; });
  enabledPlugins.subscribe((v) => { enabled = v; });

  function isEnabled(filename: string) {
    return enabled.includes(filename);
  }

  async function handleToggle(filename: string, checked: boolean) {
    if (checked) {
      await loadPlugin(filename);
    } else {
      unloadPlugin(filename);
      showReloadWarning = true;
    }
  }

  function handleCardClick(e: MouseEvent, filename: string) {
    const target = e.target as Element;
    if (target.closest('.sl-toggle') || target.closest('.sl-gear-btn') || target.closest('[data-plugin-update]')) return;
    handleToggle(filename, !isEnabled(filename));
  }

  async function handleUpdate(filename: string, info: UpdateInfo) {
    if (!info.update_url) return;
    const type = filename.endsWith('.theme.css') ? 'theme' : 'plugin';
    const updatedFilename = await downloadMod(info.update_url, type);
    await refreshPlugins();
    if (updatedFilename === filename && isEnabled(filename)) {
      await loadPlugin(filename);
    } else if (isEnabled(filename)) {
      showReloadWarning = true;
    }
    delete updates[filename];
    updates = { ...updates };
  }

  async function openSettings(filename: string) {
    const baseName = filename.replace('.plugin.js', '');
    const allSchemas = await getRegisteredSettings();
    const schema = allSchemas[baseName];
    if (schema && Array.isArray(schema) && schema.length > 0) {
      settingsModalPlugin = filename;
      settingsModalSchema = schema as any[];
    }
  }

  function closeSettingsModal() {
    settingsModalPlugin = null;
    settingsModalSchema = null;
  }

  function handleReload() {
    location.reload();
  }

  onMount(() => {
    refreshPlugins().then(() => {
      loadEnabledFromStorage();

      const list = get(plugins);
      const validPlugins = list.filter(p => p.metadata);
      if (validPlugins.length === 0) return;

      getRegisteredSettings()
        .then((allSchemas) => {
          if (allSchemas && typeof allSchemas === 'object') {
            validPlugins.forEach((plugin) => {
              const baseName = plugin.filename.replace('.plugin.js', '');
              const schema = allSchemas[baseName];
              if (schema && Array.isArray(schema) && schema.length > 0) {
                hasSettings[plugin.filename] = true;
              }
            });
            hasSettings = { ...hasSettings };
          }
        })
        .catch(() => {});

      checkModUpdates('plugin')
        .then((results) => {
          if (results) {
            for (const [filename, info] of Object.entries(results)) {
              if (info?.has_update) {
                updates[filename] = info;
              }
            }
            updates = { ...updates };
          }
        })
        .catch(() => {});
    });

    const handleSettingsRegistered = (e: Event): void => {
      const { pluginName } = (e as CustomEvent<{ pluginName: string }>).detail;
      hasSettings[`${pluginName}.plugin.js`] = true;
      hasSettings = { ...hasSettings };
    };

    window.addEventListener('sl-settings-registered', handleSettingsRegistered);
    return () => {
      window.removeEventListener('sl-settings-registered', handleSettingsRegistered);
    };
  });
</script>

<div class="sl-section-header">
  <div class="sl-section-title">Plugins</div>
</div>

{#if showReloadWarning}
  <div class="sl-reload-warning">
    Reload is required to fully disable plugins.
    <!-- svelte-ignore a11y_missing_attribute -->
    <a onclick={handleReload} class="sl-reload-link" role="button" tabindex="0" onkeydown={(e) => { if (e.key === 'Enter') handleReload(); }}>Click here to reload</a>.
  </div>
{/if}

{#if pluginList.length === 0}
  <div class="sl-empty">No plugins installed. Browse the marketplace to find plugins.</div>
{:else}
  {#each pluginList as plugin}
    {#if plugin.metadata}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="sl-card" style="cursor:pointer;" onclick={(e) => handleCardClick(e, plugin.filename)}>
        <div class="sl-card-info">
          <div class="sl-card-name">
            {plugin.metadata.name}
            <span class="sl-card-version">{plugin.metadata.version}</span>
          </div>
          <div class="sl-card-desc">{plugin.metadata.description}</div>
          <div class="sl-card-author">by {plugin.metadata.author}</div>
        </div>
        <div class="sl-card-actions">
          <!-- svelte-ignore a11y_label_has_associated_control -->
          <label class="sl-toggle">
            <input
              type="checkbox"
              checked={isEnabled(plugin.filename)}
              onchange={(e) => handleToggle(plugin.filename, (e.target as HTMLInputElement).checked)}
            />
            <div class="sl-toggle-track"><div class="sl-toggle-thumb"></div></div>
          </label>
          {#if updates[plugin.filename]}
            <button class="sl-btn sl-btn-warning" onclick={() => handleUpdate(plugin.filename, updates[plugin.filename])}>Update</button>
          {/if}
          <button
            class="sl-gear-btn {hasSettings[plugin.filename] ? '' : 'sl-gear-hidden'}"
            title="Settings"
            onclick={() => openSettings(plugin.filename)}
            type="button"
          >
            {@html ICONS.gear}
          </button>
        </div>
      </div>
    {/if}
  {/each}
{/if}

{#if settingsModalPlugin && settingsModalSchema}
  <PluginSettingsModal pluginName={settingsModalPlugin} schema={settingsModalSchema} onclose={closeSettingsModal} />
{/if}
