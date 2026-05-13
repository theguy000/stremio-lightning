// vite.config.ts
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [
    svelte(),
    {
      name: 'css-inject',
      apply: 'build',
      enforce: 'post',
      generateBundle(_, bundle) {
        // Find CSS and JS assets, inject CSS into JS
        let cssCode = '';
        const cssFiles: string[] = [];
        for (const [name, chunk] of Object.entries(bundle)) {
          if (name.endsWith('.css') && chunk.type === 'asset') {
            cssCode += chunk.source;
            cssFiles.push(name);
          }
        }
        // Remove CSS files from bundle
        cssFiles.forEach((f) => delete bundle[f]);
        // Inject CSS into JS
        if (cssCode) {
          for (const chunk of Object.values(bundle)) {
            if (chunk.type === 'chunk' && chunk.isEntry) {
              chunk.code = `(function(){var s=document.createElement('style');s.id='sl-mod-styles';s.textContent=${JSON.stringify(cssCode)};function inj(){var h=document.head||document.documentElement;if(h){h.appendChild(s)}else{document.addEventListener('DOMContentLoaded',function(){(document.head||document.documentElement).appendChild(s)},{once:true})}}inj()})();\n` + chunk.code;
              break;
            }
          }
        }
      },
    },
  ],
  build: {
    lib: {
      entry: 'src/main.ts',
      name: 'StremioLightningUI',
      formats: ['iife'],
      fileName: () => 'mod-ui-svelte.iife.js',
    },
    outDir: 'src/dist',
    emptyOutDir: false,
    minify: true,
    rollupOptions: {
      output: {
        inlineDynamicImports: true,
      },
    },
  },
  test: {
    environment: 'jsdom',
    environmentOptions: {
      jsdom: {
        url: 'http://127.0.0.1/',
      },
    },
    include: ['src/**/*.test.ts'],
    setupFiles: ['src/test/setup.ts'],
    globals: true,
    clearMocks: true,
    restoreMocks: true,
  },
});
