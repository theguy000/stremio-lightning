<script lang="ts">
  import { onMount } from 'svelte';
  import { getSetting, saveSetting } from '../ipc';
  import { ICONS } from '../icons';
  import type { SettingSchema } from '../types';

  interface Props {
    pluginName: string;
    schema: SettingSchema[];
    onclose: () => void;
  }

  let { pluginName, schema, onclose }: Props = $props();

  let baseName = $derived(pluginName.replace('.plugin.js', ''));
  let currentValues: Record<string, unknown> = $state({});
  let loaded = $state(false);

  async function loadCurrentValues() {
    const values: Record<string, unknown> = {};
    for (const setting of schema) {
      try {
        const val = await getSetting(baseName, setting.key);
        values[setting.key] = (val === null || val === undefined) ? (setting.defaultValue ?? setting.default ?? '') : val;
      } catch {
        values[setting.key] = setting.defaultValue ?? setting.default ?? '';
      }
    }
    currentValues = values;
    loaded = true;
  }

  async function handleClose() {
    // Save all current values
    for (const setting of schema) {
      const value = currentValues[setting.key];
      if (value !== undefined) {
        try {
          await saveSetting(baseName, setting.key, JSON.stringify(value));
        } catch (e) {
          console.error(`Failed to save setting ${setting.key}:`, e);
        }
      }
    }

    // Notify plugin
    if ((window as any).StremioEnhancedAPI?._notifySettingsSaved) {
      (window as any).StremioEnhancedAPI._notifySettingsSaved(baseName, currentValues);
    }

    onclose();
  }

  function handleOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) handleClose();
  }

  function handleToggleChange(key: string, e: Event) {
    currentValues[key] = (e.target as HTMLInputElement).checked;
    currentValues = { ...currentValues };
  }

  function handleInputChange(key: string, e: Event) {
    currentValues[key] = (e.target as HTMLInputElement).value;
    currentValues = { ...currentValues };
  }

  function handleSelectChange(key: string, e: Event) {
    currentValues[key] = (e.target as HTMLSelectElement).value;
    currentValues = { ...currentValues };
  }

  onMount(() => {
    loadCurrentValues();
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="sl-modal-overlay" onclick={handleOverlayClick}>
  <div class="sl-modal">
    <div class="sl-modal-header">
      <div class="sl-modal-title">{baseName} Settings</div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="sl-modal-close" onclick={handleClose}>
        {@html ICONS.close}
      </div>
    </div>
    <div class="sl-modal-body">
      {#if !loaded}
        <div class="sl-empty">Loading settings...</div>
      {:else}
        {#each schema as setting}
          <div class="sl-setting-row">
            <div class="sl-setting-label">
              <div class="sl-setting-label-text">{setting.label}</div>
              {#if setting.description}
                <div class="sl-setting-label-desc">{setting.description}</div>
              {/if}
            </div>
            <div class="sl-setting-control">
              {#if setting.type === 'toggle'}
                <!-- svelte-ignore a11y_label_has_associated_control -->
                <label class="sl-toggle">
                  <input
                    type="checkbox"
                    checked={currentValues[setting.key] === true || currentValues[setting.key] === 'true'}
                    onchange={(e) => handleToggleChange(setting.key, e)}
                  />
                  <div class="sl-toggle-track"><div class="sl-toggle-thumb"></div></div>
                </label>
              {:else if setting.type === 'input'}
                <input
                  class="sl-setting-input"
                  type="text"
                  placeholder={setting.label}
                  value={String(currentValues[setting.key] || '')}
                  oninput={(e) => handleInputChange(setting.key, e)}
                />
              {:else if setting.type === 'select'}
                <select
                  class="sl-setting-select"
                  value={String(currentValues[setting.key] || '')}
                  onchange={(e) => handleSelectChange(setting.key, e)}
                >
                  {#if setting.options}
                    {#each setting.options as opt}
                      <option value={opt.value} selected={String(opt.value) === String(currentValues[setting.key])}>{opt.label}</option>
                    {/each}
                  {/if}
                </select>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>
    <div class="sl-modal-footer">
      <button class="sl-btn sl-btn-primary" onclick={handleClose}>Close</button>
    </div>
  </div>
</div>
