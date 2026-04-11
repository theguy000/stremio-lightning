<script lang="ts">
  import { discordRpcEnabled, toggleDiscordRpc, blurEnabled, blurIntensity, applyBlurIntensity } from '../stores/settings';

  let discordOn = $state(false);
  let blurOn = $state(true);
  let blurVal = $state(100);

  discordRpcEnabled.subscribe((v) => { discordOn = v; });
  blurEnabled.subscribe((v) => { blurOn = v; });
  blurIntensity.subscribe((v) => { blurVal = v; });

  async function handleDiscordToggle(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    try {
      await toggleDiscordRpc(checked);
    } catch (err) {
      console.error('Failed to toggle Discord RPC:', err);
      // Revert on failure
      discordRpcEnabled.set(!checked);
    }
  }

  function handleBlurToggle(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    blurEnabled.set(checked);
    applyBlurIntensity(blurVal, checked);
  }

  function handleBlurRange(e: Event) {
    const value = parseInt((e.target as HTMLInputElement).value, 10);
    blurIntensity.set(value);
    applyBlurIntensity(value, blurOn);
  }
</script>

<div style="max-width:35rem;">
  <div class="sl-section-header">
    <div class="sl-section-title">Settings</div>
  </div>

  <h3 style="margin:0 0 0.75rem; font-size:1.1rem; font-weight:500; color:var(--primary-foreground-color, #f2f2f2); opacity:0.6;">Integrations</h3>

  <div class="sl-setting-row">
    <div class="sl-setting-label">
      <div class="sl-setting-label-text">Discord Rich Presence</div>
      <div class="sl-setting-label-desc">Show what you're watching on your Discord profile</div>
    </div>
    <div class="sl-setting-control">
      <!-- svelte-ignore a11y_label_has_associated_control -->
      <label class="sl-toggle">
        <input type="checkbox" checked={discordOn} onchange={handleDiscordToggle} />
        <div class="sl-toggle-track"><div class="sl-toggle-thumb"></div></div>
      </label>
    </div>
  </div>

  <h3 style="margin:1.5rem 0 0.75rem; font-size:1.1rem; font-weight:500; color:var(--primary-foreground-color, #f2f2f2); opacity:0.6;">Blur Effect</h3>

  <div class="sl-setting-row">
    <div class="sl-setting-label">
      <div class="sl-setting-label-text">Backdrop Blur</div>
      <div class="sl-setting-label-desc">Enable or disable the backdrop blur effect</div>
    </div>
    <div class="sl-setting-control">
      <!-- svelte-ignore a11y_label_has_associated_control -->
      <label class="sl-toggle">
        <input type="checkbox" checked={blurOn} onchange={handleBlurToggle} />
        <div class="sl-toggle-track"><div class="sl-toggle-thumb"></div></div>
      </label>
    </div>
  </div>

  <div class="sl-setting-row" style="{blurOn ? '' : 'opacity:0.4; pointer-events:none;'}">
    <div class="sl-setting-label">
      <div class="sl-setting-label-text">Blur Intensity</div>
      <div class="sl-setting-label-desc">Controls the backdrop blur strength of the mods panel</div>
    </div>
    <div class="sl-setting-control" style="display:flex; align-items:center; gap:0.75rem;">
      <input
        class="sl-setting-range"
        type="range"
        min="0"
        max="100"
        value={blurVal}
        oninput={handleBlurRange}
      />
      <span class="sl-range-value">{blurVal}%</span>
    </div>
  </div>
</div>
