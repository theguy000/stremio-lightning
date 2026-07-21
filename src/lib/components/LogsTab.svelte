<script lang="ts">
  import { tick } from 'svelte';
  import {
    clearDiagnostics,
    extendedDiagnosticsEnabled,
    getDiagnosticReport,
    logRecords,
    nativeLogState,
    filterLogRecords,
    setExtendedDiagnostics,
    startNativeLogPolling,
    type LogLevel,
    type LogRecord,
  } from '../logging';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();
  let query = $state('');
  let level = $state<LogLevel | 'all'>('all');
  let source = $state('all');
  let copyLabel = $state('Copy logs');
  let exportLabel = $state('Export diagnostics');
  let diagnosticsStatus = $state('');
  let extendedBusy = $state(false);
  let clearBusy = $state(false);
  let paused = $state(false);
  let pausedRecords = $state<LogRecord[] | null>(null);
  let followNewest = $state(true);
  let logList: HTMLDivElement | undefined = $state();

  const pageSize = 200;
  const levels: Array<LogLevel | 'all'> = ['all', 'debug', 'info', 'warn', 'error'];
  let visibleLimit = $state(pageSize);
  let displayedRecords = $derived(paused && pausedRecords ? pausedRecords : $logRecords);
  let sources = $derived([...new Set(displayedRecords.map((record) => sourceLabel(record.source)))].sort());
  let filteredRecords = $derived(
    filterLogRecords(displayedRecords, { query, level, source: 'all' })
      .filter((record) => source === 'all' || sourceLabel(record.source) === source),
  );
  let visibleRecords = $derived(filteredRecords.slice(0, visibleLimit));

  $effect(() => {
    if (!active) return;
    return startNativeLogPolling();
  });

  $effect(() => {
    if (source !== 'all' && !sources.includes(source)) source = 'all';
  });

  $effect(() => {
    query;
    level;
    source;
    visibleLimit = pageSize;
  });

  $effect(() => {
    const newestId = visibleRecords[0]?.id;
    if (!newestId || !active || paused || !followNewest) return;
    void tick().then(() => logList?.scrollTo({ top: 0, behavior: 'smooth' }));
  });

  function formatTimestamp(timestamp: number): string {
    return new Date(timestamp).toLocaleTimeString([], { hour12: false });
  }

  function sourceLabel(sourceName: string): string {
    const normalized = sourceName.toLowerCase();
    if (normalized.includes('update')) return 'Updater';
    if (normalized.includes('player') || normalized.includes('mpv')) return 'Player';

    switch (normalized.split('.')[0]) {
      case 'ui': return 'UI';
      case 'bridge': return 'Bridge';
      case 'native': return 'Native';
      default: return 'System';
    }
  }

  function sourceTone(sourceName: string): string {
    return sourceLabel(sourceName).toLowerCase();
  }

  function splitMessage(message: string): { summary: string; details: string } {
    const newline = message.indexOf('\n');
    if (newline === -1) return { summary: message, details: '' };
    return {
      summary: message.slice(0, newline).trimEnd(),
      details: message.slice(newline + 1).trim(),
    };
  }

  function togglePaused(): void {
    if (paused) {
      paused = false;
      pausedRecords = null;
    } else {
      pausedRecords = [...$logRecords];
      paused = true;
    }
  }

  async function clearLogs(): Promise<void> {
    if (clearBusy) return;
    clearBusy = true;
    diagnosticsStatus = '';
    try {
      await clearDiagnostics();
    } catch {
      diagnosticsStatus = 'Could not clear retained diagnostics.';
    } finally {
      clearBusy = false;
    }
    paused = false;
    pausedRecords = null;
  }

  async function changeExtendedDiagnostics(event: Event): Promise<void> {
    if (extendedBusy) return;
    const enabled = (event.currentTarget as HTMLInputElement).checked;
    extendedBusy = true;
    diagnosticsStatus = '';
    try {
      await setExtendedDiagnostics(enabled);
    } catch {
      diagnosticsStatus = 'Could not update Extended diagnostics.';
    } finally {
      extendedBusy = false;
    }
  }

  async function copyLogs(): Promise<void> {
    const text = filteredRecords
      .map((record) => `${new Date(record.timestamp).toISOString()} [${record.level.toUpperCase()}] ${record.source}: ${record.message}`)
      .join('\n');

    try {
      await navigator.clipboard.writeText(text);
      copyLabel = 'Copied';
    } catch {
      copyLabel = 'Copy failed';
    }
    window.setTimeout(() => copyLabel = 'Copy logs', 1500);
  }

  async function exportDiagnostics(): Promise<void> {
    if (exportLabel !== 'Export diagnostics') return;
    exportLabel = 'Preparing...';
    diagnosticsStatus = '';
    try {
      const report = await getDiagnosticReport();
      try {
        const blob = new Blob([report], { type: 'text/plain;charset=utf-8' });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement('a');
        anchor.href = url;
        anchor.download = `stremio-lightning-diagnostics-${new Date().toISOString().replace(/[:.]/g, '-')}.txt`;
        anchor.style.display = 'none';
        document.body.appendChild(anchor);
        anchor.click();
        anchor.remove();
        window.setTimeout(() => URL.revokeObjectURL(url), 0);
        diagnosticsStatus = 'Diagnostics report downloaded.';
      } catch {
        try {
          await navigator.clipboard.writeText(report);
          diagnosticsStatus = 'Download unavailable. Diagnostics report copied to clipboard.';
        } catch {
          diagnosticsStatus = 'Could not download or copy the diagnostics report.';
        }
      }
    } catch {
      diagnosticsStatus = 'Could not prepare the diagnostics report.';
    } finally {
      exportLabel = 'Export diagnostics';
    }
  }
</script>

<div class="sl-logs">
  <div class="sl-section-header sl-log-header">
    <div class="sl-log-heading">
      <div class="sl-section-title">Logs</div>
      <div class="sl-log-count">
        {filteredRecords.length} / {displayedRecords.length} records{paused ? ' (paused)' : ''}
      </div>
    </div>
    <div class="sl-log-header-actions">
      <label class="sl-log-extended" title="Includes additional browser request timing and console warnings for this session only.">
        Extended diagnostics (this session)
        <span class="sl-toggle">
          <input
            type="checkbox"
            checked={$extendedDiagnosticsEnabled}
            disabled={extendedBusy}
            onchange={changeExtendedDiagnostics}
          />
          <span class="sl-toggle-track"><span class="sl-toggle-thumb"></span></span>
        </span>
      </label>
      <button
        type="button"
        class="sl-btn sl-btn-ghost sl-log-action"
        class:sl-active={paused}
        aria-pressed={paused}
        onclick={togglePaused}
      >
        {paused ? 'Resume' : 'Pause'}
      </button>
      <button
        type="button"
        class="sl-btn sl-btn-ghost sl-log-action"
        class:sl-active={followNewest}
        aria-pressed={followNewest}
        onclick={() => followNewest = !followNewest}
      >
        Follow newest
      </button>
      <button
        type="button"
        class="sl-btn sl-btn-ghost sl-log-action"
        disabled={clearBusy}
        onclick={clearLogs}
      >
        {clearBusy ? 'Clearing...' : 'Clear'}
      </button>
      <button
        type="button"
        class="sl-btn sl-btn-ghost sl-log-action"
        disabled={exportLabel !== 'Export diagnostics'}
        onclick={exportDiagnostics}
      >
        {exportLabel}
      </button>
      <button
        type="button"
        class="sl-btn sl-btn-ghost sl-log-action"
        disabled={filteredRecords.length === 0}
        onclick={copyLogs}
      >
        {copyLabel}
      </button>
    </div>
  </div>

  <div class="sl-log-toolbar">
    <label class="sl-log-search-wrap">
      <input
        class="sl-search sl-log-search"
        type="search"
        aria-label="Search logs"
        placeholder="Search source or message"
        autocomplete="off"
        spellcheck="false"
        bind:value={query}
      />
    </label>

    <label class="sl-log-source-wrap">
      <select class="sl-log-source-filter" aria-label="Filter logs by source" bind:value={source}>
        <option value="all">All sources</option>
        {#each sources as sourceName}
          <option value={sourceName}>{sourceName}</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="sl-log-levels" aria-label="Filter logs by level">
    {#each levels as levelName}
      <button
        type="button"
        class="sl-log-level-filter"
        class:sl-active={level === levelName}
        aria-pressed={level === levelName}
        onclick={() => level = levelName}
      >
        {levelName === 'all' ? 'All' : levelName}
      </button>
    {/each}
  </div>

  {#if active && $nativeLogState === 'loading'}
    <div class="sl-log-status">Loading native logs...</div>
  {:else if $nativeLogState === 'unavailable'}
    <div class="sl-log-status sl-log-status-warning">
      Native logs are unavailable. Browser and plugin records are still shown.
    </div>
  {/if}

  {#if diagnosticsStatus}
    <div class="sl-log-status">{diagnosticsStatus}</div>
  {/if}

  {#if displayedRecords.length === 0 && $nativeLogState !== 'loading'}
    <div class="sl-empty sl-log-empty">No logs have been recorded this session.</div>
  {:else if filteredRecords.length === 0}
    <div class="sl-empty sl-log-empty">No logs match the current filters.</div>
  {:else}
    <div class="sl-log-list" role="log" aria-live="off" bind:this={logList}>
      {#each visibleRecords as record (record.id)}
        {@const message = splitMessage(record.message)}
        <article class="sl-log-entry" class:sl-log-entry-error={record.level === 'error'}>
          <time class="sl-log-time" datetime={new Date(record.timestamp).toISOString()}>
            {formatTimestamp(record.timestamp)}
          </time>
          <span class="sl-log-level sl-log-level-{record.level}">{record.level}</span>
          <span
            class="sl-log-source sl-log-source-{sourceTone(record.source)}"
            title={record.source}
          >
            {sourceLabel(record.source)}
          </span>
          <div class="sl-log-message">
            <span>{message.summary}</span>
            {#if message.details}
              <details class="sl-log-details">
                <summary>Show details</summary>
                <pre>{message.details}</pre>
              </details>
            {/if}
          </div>
        </article>
      {/each}
    </div>
    {#if visibleRecords.length < filteredRecords.length}
      <div class="sl-log-more">
        <button
          type="button"
          class="sl-btn sl-btn-ghost"
          onclick={() => visibleLimit += pageSize}
        >
          Show older records
        </button>
      </div>
    {/if}
  {/if}
</div>
