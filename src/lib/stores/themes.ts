import { writable } from 'svelte/store';
import { getThemes, getModContent } from '../ipc';
import type { InstalledMod } from '../types';

export const themes = writable<InstalledMod[]>([]);
export const currentTheme = writable<string>(localStorage.getItem('currentTheme') || '');

// Track which CSS properties we set so we only remove ours on theme change.
// Shared with plugin-api.ts via window.__slThemeInlineProps so both modules
// stay in sync and don't leave stale properties behind on theme switch.
declare global { interface Window { __slThemeInlineProps?: string[] } }
let _themeInlineProps: string[] = [];
if (typeof window !== 'undefined') {
  if (window.__slThemeInlineProps) {
    _themeInlineProps = window.__slThemeInlineProps;
  } else {
    window.__slThemeInlineProps = _themeInlineProps;
  }
}

export async function refreshThemes() {
  const list = await getThemes();
  themes.set(list);
}

export async function applyTheme(filename: string): Promise<void> {
  // Remove previous custom theme (matches bridge.js element id: 'activeTheme')
  const prev = document.getElementById('activeTheme');
  if (prev) prev.remove();

  // Remove only the CSS properties we previously set (from both this module
  // and plugin-api.ts, which shares the same tracking array)
  const root = document.documentElement;
  _themeInlineProps.forEach((v) => root.style.removeProperty(v));
  _themeInlineProps.length = 0;

  if (!filename || filename === 'default' || filename === 'Default') {
    localStorage.removeItem('currentTheme');
    currentTheme.set('');
    document.dispatchEvent(new CustomEvent('sl-theme-changed'));
    return;
  }

  const css = await getModContent(filename, 'theme');

  // Inject CSS
  const style = document.createElement('style');
  style.id = 'activeTheme';
  style.textContent = css;
  document.head.appendChild(style);

  // Extract and apply CSS custom properties, tracking what we set
  const varRegex = /--([\w-]+)\s*:\s*([^;]+)/g;
  let match: RegExpExecArray | null;
  while ((match = varRegex.exec(css)) !== null) {
    const prop = `--${match[1]}`;
    root.style.setProperty(prop, match[2].trim());
    _themeInlineProps.push(prop);
  }

  localStorage.setItem('currentTheme', filename);
  currentTheme.set(filename);
  document.dispatchEvent(new CustomEvent('sl-theme-changed'));
}

export function loadThemeFromStorage(): void {
  const stored = localStorage.getItem('currentTheme');
  if (stored) {
    applyTheme(stored).catch(console.error);
  }
}
