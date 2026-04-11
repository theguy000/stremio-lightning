<script lang="ts">
  import { onMount } from 'svelte';
  import ModsButton from './lib/components/ModsButton.svelte';
  import ModsPanel from './lib/components/ModsPanel.svelte';
  import { loadEnabledFromStorage, loadPlugin, refreshPlugins } from './lib/stores/plugins';
  import { loadThemeFromStorage } from './lib/stores/themes';
  import { loadSettingsFromStorage } from './lib/stores/settings';

  let panelOpen = $state(false);

  function togglePanel() {
    panelOpen = !panelOpen;
  }

  onMount(async () => {
    // Load persisted settings
    loadSettingsFromStorage();
    loadThemeFromStorage();

    // Load enabled plugins
    await refreshPlugins();
    const enabled = loadEnabledFromStorage();
    for (const pluginName of enabled) {
      try {
        await loadPlugin(pluginName);
      } catch (e) {
        console.error(`Failed to load plugin ${pluginName}:`, e);
      }
    }

    // Re-apply blur intensity when theme changes
    window.addEventListener('sl-theme-changed', () => {
      loadSettingsFromStorage();
    });
  });
</script>

<ModsButton active={panelOpen} ontoggle={togglePanel} />
<ModsPanel open={panelOpen} onclose={() => (panelOpen = false)} />
