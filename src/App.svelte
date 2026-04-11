<script lang="ts">
  // Root overlay — will hold all UI components
</script>

<div id="sl-overlay-root">
  <p style="color: white; position: fixed; bottom: 80px; left: 20px; z-index: 100000; pointer-events: auto;">
    Svelte overlay loaded!
  </p>
</div>

<style>
  #sl-overlay-root {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 99999;
    pointer-events: none;
  }
</style>
