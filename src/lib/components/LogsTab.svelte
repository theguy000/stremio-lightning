<script lang="ts">
  import {
    logRecords,
    nativeLogState,
    filterLogRecords,
    startNativeLogPolling,
    type LogLevel,
  } from '../logging';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();
  let query = $state('');
  let level = $state<LogLevel | 'all'>('all');
  let source = $state('all');
  let copyLabel = $state('Copy logs');

  const pageSize = 200;
  const levels: Array<LogLevel | 'all'> = ['all', 'debug', 'info', 'warn', 'error'];
  let visibleLimit = $state(pageSize);
  let sources = $derived([...new Set($logRecords.map((record) => record.source))].sort());
  let filteredRecords = $derived(filterLogRecords($logRecords, { query, level, source }));
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

  function formatTimestamp(timestamp: number): string {
    const date = new Date(timestamp);
    const milliseconds = String(date.getMilliseconds()).padStart(3, '0');
    return `${date.toLocaleTimeString([], { hour12: false })}.${milliseconds}`;
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
</script>

<div class="sl-logs">
  <div class="sl-section-header sl-log-header">
    <div class="sl-section-title">Logs</div>
    <div class="sl-log-header-actions">
      <div class="sl-log-count">{filteredRecords.length} / {$logRecords.length} records</div>
      <button
        type="button"
        class="sl-btn sl-btn-ghost sl-log-copy"
        disabled={filteredRecords.length === 0}
        onclick={copyLogs}
      >
        {copyLabel}
      </button>
    </div>
  </div>

  <div class="sl-log-toolbar">
    <label class="sl-log-search-wrap">
      <span class="sl-log-label">Search</span>
      <input
        class="sl-search sl-log-search"
        type="search"
        placeholder="Search source or message"
        autocomplete="off"
        spellcheck="false"
        bind:value={query}
      />
    </label>

    <label class="sl-log-source-wrap">
      <span class="sl-log-label">Source</span>
      <select class="sl-log-source-filter" bind:value={source}>
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

  {#if $logRecords.length === 0 && $nativeLogState !== 'loading'}
    <div class="sl-empty sl-log-empty">No logs have been recorded this session.</div>
  {:else if filteredRecords.length === 0}
    <div class="sl-empty sl-log-empty">No logs match the current filters.</div>
  {:else}
    <div class="sl-log-list" role="log" aria-live="off">
      {#each visibleRecords as record (record.id)}
        <article class="sl-log-entry">
          <time class="sl-log-time" datetime={new Date(record.timestamp).toISOString()}>
            {formatTimestamp(record.timestamp)}
          </time>
          <span class="sl-log-level sl-log-level-{record.level}">{record.level}</span>
          <span class="sl-log-source">{record.source}</span>
          <span class="sl-log-message">{record.message}</span>
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
