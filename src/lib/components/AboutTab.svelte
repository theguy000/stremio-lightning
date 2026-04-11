<script lang="ts">
  import { openExternalUrl } from '../ipc';
  import { ICONS } from '../icons';

  function handleLink(e: MouseEvent) {
    e.preventDefault();
    const href = (e.currentTarget as HTMLAnchorElement).href;
    openExternalUrl(href);
  }
</script>

<div class="sl-about">
  <h2>Stremio Lightning</h2>
  <p>A lightweight Stremio desktop client built with Tauri.<br>
  Supports plugins and themes from the Stremio Enhanced ecosystem.</p>
  <a class="sl-btn sl-btn-ghost" href="https://github.com/theguy000/stremio-lightning" target="_blank" rel="noreferrer" onclick={handleLink}>
    {@html ICONS.github} GitHub Repository
  </a>
</div>
