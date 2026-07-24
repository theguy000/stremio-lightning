<script lang="ts">
  import { onMount } from 'svelte';
  import ModsButton from './lib/components/ModsButton.svelte';
  import ModsPanel from './lib/components/ModsPanel.svelte';
  import { createLogger } from './lib/logging';
  import { loadEnabledFromStorage, loadPlugin, refreshPlugins } from './lib/stores/plugins';
  import { loadThemeFromStorage } from './lib/stores/themes';
  import { loadSettingsFromStorage } from './lib/stores/settings';

  let panelOpen = $state(false);
  const logger = createLogger('ui.plugins');

  function togglePanel() {
    panelOpen = !panelOpen;
  }

  onMount(async () => {
    const handleThemeChanged = () => requestAnimationFrame(loadSettingsFromStorage);
    document.addEventListener('sl-theme-changed', handleThemeChanged);

    await loadThemeFromStorage();
    loadSettingsFromStorage();

    // Load enabled plugins
    await refreshPlugins();
    const enabled = loadEnabledFromStorage();
    await Promise.all(
      enabled.map(async (pluginName) => {
        try {
          await loadPlugin(pluginName);
        } catch (e) {
          logger.error(`Failed to load plugin ${pluginName}:`, e);
        }
      })
    );

    return () => document.removeEventListener('sl-theme-changed', handleThemeChanged);
  });
</script>

<ModsButton active={panelOpen} ontoggle={togglePanel} />
<ModsPanel open={panelOpen} onclose={() => (panelOpen = false)} />
