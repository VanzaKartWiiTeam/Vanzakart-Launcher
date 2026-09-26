<script lang="ts">
  /**
   * Time Trial: la classifica dei ghost.
   *
   * La pagina principale è la lista delle piste, ognuna con il suo record;
   * aprendone una compare la classifica dei tempi, e ogni tempo ha il suo
   * ghost da scaricare. Il ghost finisce direttamente dove il gioco lo cerca —
   * `Wii/shared2/Pulsar/<modpack>/Ghosts/<crc>/150/` — e compare in Time
   * Trial la prossima volta che si apre la pista (§D-087).
   *
   * Dal frontend non passa nessun indirizzo: si mandano al backend gli id
   * della pista e del tempo, e il file lo scarica, lo controlla e lo scrive
   * lui.
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import { app, formatDate } from '$lib/stores/app.svelte';
  import { formatNumber, t, type TranslationKey } from '$lib/stores/i18n.svelte';
  import type {
    GhostBlocker,
    GhostCatalogView,
    GhostEntryView,
    GhostLeaderboardView,
    GhostTrackView
  } from '$lib/api/types';

  let catalog = $state<GhostCatalogView | null>(null);
  let loading = $state(true);
  let error = $state('');

  let query = $state('');
  let category = $state('all');
  let onlyRecords = $state(false);

  let selected = $state<GhostTrackView | null>(null);
  let board = $state<GhostLeaderboardView | null>(null);
  let boardLoading = $state(false);
  let boardError = $state('');
  /** Tempi il cui ghost si sta scaricando o togliendo. */
  let working = $state<number[]>([]);

  const BLOCKERS: Record<Exclude<GhostBlocker, ''>, TranslationKey> = {
    'no-user-folder': 'ghosts.blocker.noUserFolder',
    'mod-not-installed': 'ghosts.blocker.modNotInstalled',
    'no-track-map': 'ghosts.blocker.noTrackMap',
    'track-not-in-modpack': 'ghosts.blocker.trackMissing'
  };

  const CONTROLLERS: TranslationKey[] = [
    'ghosts.controller.wheel',
    'ghosts.controller.nunchuk',
    'ghosts.controller.classic',
    'ghosts.controller.gamecube'
  ];

  const categories = $derived(
    [...new Set((catalog?.tracks ?? []).map((track) => track.category).filter(Boolean))].sort()
  );

  const tracks = $derived(
    (catalog?.tracks ?? []).filter((track) => {
      const needle = query.trim().toLowerCase();
      if (needle && !track.name.toLowerCase().includes(needle)) return false;
      if (category !== 'all' && track.category !== category) return false;
      if (onlyRecords && !track.record) return false;
      return true;
    })
  );

  const withRecord = $derived((catalog?.tracks ?? []).filter((track) => track.record).length);

  $effect(() => {
    void load(false);
  });

  async function load(refresh: boolean) {
    loading = true;
    try {
      catalog = await api.fetchGhostCatalog(refresh);
      error = '';
      // La pista aperta si aggiorna con i dati nuovi, se c'è ancora.
      if (selected) {
        selected = catalog.tracks.find((track) => track.id === selected?.id) ?? selected;
      }
    } catch (caught) {
      error = api.errorMessage(caught);
    } finally {
      loading = false;
    }
  }

  async function openTrack(track: GhostTrackView, page = 1) {
    selected = track;
    boardLoading = true;
    boardError = '';
    if (page === 1) board = null;
    try {
      board = await api.fetchGhostLeaderboard(track.id, page);
      selected = board.track;
      document.getElementById('vk-content')?.scrollTo({ top: 0 });
    } catch (caught) {
      boardError = api.errorMessage(caught);
    } finally {
      boardLoading = false;
    }
  }

  function closeTrack() {
    selected = null;
    board = null;
    boardError = '';
  }

  function blockerText(blocker: GhostBlocker): string {
    if (!blocker) return '';
    return t(BLOCKERS[blocker], { channel: catalog?.channel ?? 'Stable' });
  }

  /** Dove si risolve un blocco: la pagina giusta, con un clic. */
  function blockerRoute(blocker: GhostBlocker): 'settings' | 'mods' | null {
    if (blocker === 'no-user-folder') return 'settings';
    if (blocker === 'mod-not-installed' || blocker === 'no-track-map') return 'mods';
    return null;
  }

  function setInstalled(entry: GhostEntryView, installed: boolean, count: number) {
    if (board) {
      board = {
        ...board,
        entries: board.entries.map((item) =>
          item.submissionId === entry.submissionId ? { ...item, installed } : item
        )
      };
    }
    const update = (track: GhostTrackView) =>
      track.id === selected?.id ? { ...track, installedCount: count } : track;
    if (selected) selected = update(selected);
    if (catalog) catalog = { ...catalog, tracks: catalog.tracks.map(update) };
  }

  async function install(entry: GhostEntryView) {
    if (!selected || working.includes(entry.submissionId)) return;
    const track = selected;
    working = [...working, entry.submissionId];
    try {
      const outcome = await api.installGhost(track.id, entry.submissionId);
      setInstalled(entry, true, outcome.installedCount);
      app.toast(
        outcome.alreadyPresent ? t('ghosts.alreadyInstalled') : t('ghosts.installed'),
        t('ghosts.installedBody', {
          player: entry.playerName,
          time: entry.finishTime,
          track: outcome.trackName
        }),
        'success'
      );
    } catch (caught) {
      app.toast(t('ghosts.installFailed'), api.errorMessage(caught), 'warning');
    } finally {
      working = working.filter((id) => id !== entry.submissionId);
    }
  }

  async function remove(entry: GhostEntryView) {
    if (working.includes(entry.submissionId)) return;
    working = [...working, entry.submissionId];
    try {
      const left = await api.removeGhost(entry.submissionId);
      setInstalled(entry, false, left);
      app.toast(t('ghosts.removed'), t('ghosts.removedBody', { player: entry.playerName }), 'info');
    } catch (caught) {
      app.toast(t('home.operationFailed'), api.errorMessage(caught), 'warning');
    } finally {
      working = working.filter((id) => id !== entry.submissionId);
    }
  }

  async function openFolder(trackId?: number) {
    try {
      await api.openGhostFolder(trackId);
    } catch (caught) {
      app.toast(t('mods.folderFailed'), api.errorMessage(caught), 'warning');
    }
  }

  function medal(rank: number): string {
    return rank === 1 ? 'gold' : rank === 2 ? 'silver' : rank === 3 ? 'bronze' : '';
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && selected) closeTrack();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="page">
  {#if selected}
    {@const track = selected}
    <!-- ── CLASSIFICA DI UNA PISTA ───────────────────────────────────── -->
    <section class="vk-card vk-rainbow-top track-head">
      <button class="vk-btn back" onclick={closeTrack} title={t('ghosts.backHint')}>
        <span class="back-arrow" aria-hidden="true"><Icon name="chevron" size={14} /></span>
        {t('ghosts.back')}
      </button>

      <div class="track-title">
        <p class="vk-eyebrow">
          {track.category || t('ghosts.track')} · {t('ghosts.laps', { count: track.laps })} · 150cc
        </p>
        <h2 class="vk-title">{track.name}</h2>
      </div>

      <div class="track-side">
        {#if track.record}
          <div class="record">
            <span class="vk-eyebrow">{t('ghosts.record')}</span>
            <strong class="record-time">{track.record.finishTime}</strong>
            <span class="vk-faint">
              {#if track.record.country}<span class="flag">{track.record.country}</span>{/if}
              {track.record.playerName}
            </span>
          </div>
        {/if}
        {#if track.installable}
          <button class="vk-btn" onclick={() => openFolder(track.id)}>
            <Icon name="folder" size={14} />
            {t('ghosts.folder')}
            {#if track.installedCount > 0}
              <span class="count">{track.installedCount}</span>
            {/if}
          </button>
        {/if}
      </div>
    </section>

    {#if !track.installable}
      <div class="notice">
        <Icon name="warning" size={16} />
        <p>{blockerText(track.blocker)}</p>
        {#if blockerRoute(track.blocker)}
          <button
            class="vk-btn"
            onclick={() => app.navigate(blockerRoute(track.blocker) ?? 'mods')}
          >
            {t('ghosts.fix')}
          </button>
        {/if}
      </div>
    {:else}
      <p class="vk-faint hint">{t('ghosts.whereHint')}</p>
    {/if}

    {#if boardLoading && !board}
      <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
    {:else if boardError}
      <div class="vk-error">
        <strong>{t('ghosts.boardUnavailable')}</strong>
        <p>{boardError}</p>
        <button class="vk-btn" onclick={() => openTrack(track)}>{t('common.retry')}</button>
      </div>
    {:else if board && board.entries.length === 0}
      <div class="vk-card vk-empty">
        <Icon name="stopwatch" size={28} />
        <p>{t('ghosts.noTimes')}</p>
        <p class="vk-faint">{t('ghosts.noTimesHint')}</p>
      </div>
    {:else if board}
      <section class="vk-card table-card" class:dim={boardLoading}>
        <div class="row head">
          <span>#</span>
          <span>{t('ghosts.col.player')}</span>
          <span class="num">{t('ghosts.col.time')}</span>
          <span class="num">{t('ghosts.col.lap')}</span>
          <span>{t('ghosts.col.combo')}</span>
          <span>{t('ghosts.col.date')}</span>
          <span class="num">{t('ghosts.col.ghost')}</span>
        </div>

        <ul class="rows">
          {#each board.entries as entry (entry.submissionId)}
            {@const busy = working.includes(entry.submissionId)}
            <li class="row entry" class:mine={entry.installed}>
              <span class="rank {medal(entry.rank)}">{entry.rank}</span>
              <span class="player">
                {#if entry.country}
                  <span class="flag" title={entry.countryName || entry.country}>
                    {entry.country}
                  </span>
                {/if}
                <span class="player-id">
                  <strong class="player-name">{entry.playerName}</strong>
                  {#if entry.miiName && entry.miiName !== entry.playerName}
                    <span class="vk-faint mii">{entry.miiName}</span>
                  {/if}
                </span>
              </span>
              <span
                class="num time"
                title={entry.lapSplits.length > 0
                  ? t('ghosts.splits', { laps: entry.lapSplits.join(' · ') })
                  : undefined}
              >
                {entry.finishTime}
              </span>
              <span class="num vk-faint">{entry.fastestLap || t('common.dash')}</span>
              <span class="combo">
                <span>{entry.character || t('common.dash')}</span>
                <span class="vk-faint">
                  {entry.vehicle}
                  {#if CONTROLLERS[entry.controller]}
                    · {t(CONTROLLERS[entry.controller]!)}
                  {/if}
                  {#if entry.shroomless}· {t('ghosts.shroomless')}{/if}
                </span>
              </span>
              <span class="vk-faint date">{entry.dateSet ? formatDate(entry.dateSet) : ''}</span>
              <span class="num action">
                {#if entry.installed}
                  <span class="vk-badge vk-badge--success installed">
                    <Icon name="check" size={12} />
                    {t('ghosts.installedBadge')}
                  </span>
                  <button
                    class="vk-btn vk-btn--danger icon-btn"
                    title={t('ghosts.remove')}
                    aria-label={t('ghosts.remove')}
                    onclick={() => remove(entry)}
                    disabled={busy}
                  >
                    <Icon name="trash" size={13} />
                  </button>
                {:else}
                  <button
                    class="vk-btn vk-btn--primary small"
                    onclick={() => install(entry)}
                    disabled={busy || !track.installable}
                    title={track.installable
                      ? t('ghosts.downloadHint')
                      : blockerText(track.blocker)}
                  >
                    <Icon name="download" size={13} />
                    {busy ? t('ghosts.downloading') : t('ghosts.download')}
                  </button>
                {/if}
              </span>
            </li>
          {/each}
        </ul>

        <footer class="table-foot">
          <span class="vk-faint">
            {t('ghosts.total', { count: formatNumber(board.total) })}
            {#if board.fastestLap}
              · {t('ghosts.bestLap', { time: board.fastestLap })}
            {/if}
          </span>
          {#if board.totalPages > 1}
            <div class="pager">
              <button
                class="vk-btn small"
                onclick={() => openTrack(track, board!.page - 1)}
                disabled={board.page <= 1 || boardLoading}
              >
                {t('gb.previous')}
              </button>
              <span class="vk-faint">
                {t('ghosts.page', { page: board.page, pages: board.totalPages })}
              </span>
              <button
                class="vk-btn small"
                onclick={() => openTrack(track, board!.page + 1)}
                disabled={board.page >= board.totalPages || boardLoading}
              >
                {t('gb.next')}
              </button>
            </div>
          {/if}
        </footer>
      </section>
    {/if}
  {:else}
    <!-- ── LISTA DELLE PISTE ─────────────────────────────────────────── -->
    <div class="toolbar">
      <input class="vk-input search" bind:value={query} placeholder={t('ghosts.search')} />
      <button class="vk-btn" onclick={() => load(true)} disabled={loading}>
        <Icon name="refresh" size={14} />
        {loading ? t('common.refreshing') : t('common.refreshAction')}
      </button>
      {#if catalog && !catalog.blocker}
        <button class="vk-btn" onclick={() => openFolder()} title={t('ghosts.allFoldersHint')}>
          <Icon name="folder" size={14} />
          {t('ghosts.allFolders')}
        </button>
      {/if}
    </div>

    {#if catalog}
      <div class="filters">
        <button class="chip" class:active={category === 'all'} onclick={() => (category = 'all')}>
          {t('ghosts.allTracks')}
        </button>
        {#each categories as item (item)}
          <button class="chip" class:active={category === item} onclick={() => (category = item)}>
            {item}
          </button>
        {/each}
        <span class="vk-spacer"></span>
        <button
          class="chip"
          class:active={onlyRecords}
          onclick={() => (onlyRecords = !onlyRecords)}
          disabled={withRecord === 0}
        >
          <Icon name="stopwatch" size={12} />
          {t('ghosts.onlyRecords', { count: withRecord })}
        </button>
      </div>

      {#if catalog.blocker}
        <div class="notice">
          <Icon name="warning" size={16} />
          <p>{blockerText(catalog.blocker)}</p>
          {#if blockerRoute(catalog.blocker)}
            <button
              class="vk-btn"
              onclick={() => app.navigate(blockerRoute(catalog!.blocker) ?? 'mods')}
            >
              {t('ghosts.fix')}
            </button>
          {/if}
        </div>
      {/if}

      {#if !catalog.recordsAvailable}
        <p class="vk-faint hint">{t('ghosts.recordsUnavailable')}</p>
      {/if}
    {/if}

    {#if loading && !catalog}
      <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
    {:else if error && !catalog}
      <div class="vk-error">
        <strong>{t('ghosts.unavailable')}</strong>
        <p>{error}</p>
        <button class="vk-btn" onclick={() => load(true)}>{t('common.retry')}</button>
      </div>
    {:else if catalog && tracks.length === 0}
      <div class="vk-card vk-empty">
        <Icon name="stopwatch" size={28} />
        <p>{catalog.tracks.length === 0 ? t('ghosts.noTracks') : t('ghosts.noMatch')}</p>
      </div>
    {:else if catalog}
      <p class="vk-faint count-line">
        {t('ghosts.count', { shown: tracks.length, total: catalog.tracks.length })}
        · {catalog.channel} · {catalog.cc}cc
      </p>

      <ul class="grid">
        {#each tracks as track (track.id)}
          <li>
            <button
              class="track"
              class:has-record={track.record !== null}
              class:unavailable={!track.installable && track.blocker === 'track-not-in-modpack'}
              onclick={() => openTrack(track)}
              title={track.installable ? '' : blockerText(track.blocker)}
            >
              <span class="track-top">
                <span class="vk-badge track-category">{track.category || '—'}</span>
                <span class="vk-faint laps">{t('ghosts.laps', { count: track.laps })}</span>
                {#if track.installedCount > 0}
                  <span
                    class="vk-badge vk-badge--success ghosts-count"
                    title={t('ghosts.installedCount', { count: track.installedCount })}
                  >
                    <Icon name="stopwatch" size={11} />
                    {track.installedCount}
                  </span>
                {/if}
              </span>

              <span class="track-name">{track.name}</span>

              {#if track.record}
                <span class="track-record">
                  <strong>{track.record.finishTime}</strong>
                  <span class="vk-faint">
                    {#if track.record.country}<span class="flag">{track.record.country}</span>{/if}
                    {track.record.playerName}
                  </span>
                </span>
              {:else}
                <span class="vk-faint track-record empty">{t('ghosts.noRecord')}</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-bottom: 12px;
  }

  .toolbar {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }

  .search {
    flex: 1;
    min-width: 220px;
  }

  .filters {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: 999px;
    background: transparent;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    cursor: pointer;
  }

  .chip:hover:not(:disabled) {
    border-color: #3a4c74;
  }

  .chip:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .chip.active {
    border-color: transparent;
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    color: var(--vk-text);
  }

  .notice {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid color-mix(in srgb, var(--vk-warning) 40%, var(--vk-stroke));
    border-radius: var(--vk-radius-badge);
    background: color-mix(in srgb, var(--vk-warning) 8%, transparent);
    color: var(--vk-warning);
    font-size: var(--vk-fs-small);
  }

  .notice p {
    flex: 1;
    margin: 0;
  }

  .hint,
  .count-line {
    margin: 0;
    font-size: var(--vk-fs-micro);
  }

  .skeleton {
    height: 260px;
  }

  /* ---- Griglia delle piste ---- */

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 12px;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .track {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
    height: 100%;
    padding: 14px 16px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel-soft);
    color: inherit;
    text-align: left;
    transition:
      transform var(--vk-dur-fast) var(--vk-ease),
      border-color var(--vk-dur-fast) var(--vk-ease),
      box-shadow var(--vk-dur-fast) var(--vk-ease);
  }

  .track:hover {
    transform: translateY(-2px);
    border-color: #3a4c74;
  }

  /* Una pista con un record ha qualcosa da battere: si accende. */
  .track.has-record {
    border-color: color-mix(in srgb, #ffd166 35%, var(--vk-stroke));
    box-shadow: 0 0 16px rgb(255 209 102 / 0.08);
  }

  .track.unavailable {
    opacity: 0.55;
  }

  .track-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .track-category {
    font-size: var(--vk-fs-eyebrow);
  }

  .laps {
    font-size: var(--vk-fs-eyebrow);
  }

  .ghosts-count {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
    font-size: var(--vk-fs-eyebrow);
  }

  .track-name {
    font-size: 15px;
    font-weight: 900;
    line-height: 1.25;
  }

  .track-record {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-top: auto;
    font-size: var(--vk-fs-micro);
    min-width: 0;
  }

  .track-record strong {
    font-size: 16px;
    font-weight: 900;
    color: #ffd166;
    font-variant-numeric: tabular-nums;
  }

  .track-record .vk-faint {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-record.empty {
    font-style: italic;
  }

  .flag {
    display: inline-block;
    margin-right: 4px;
    padding: 1px 5px;
    border: 1px solid var(--vk-stroke);
    border-radius: 5px;
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    letter-spacing: 0.04em;
    color: var(--vk-text-secondary);
  }

  /* ---- Testata della pista ---- */

  .track-head {
    display: flex;
    align-items: center;
    gap: 18px;
    flex-wrap: wrap;
    padding: 20px 24px;
  }

  .back {
    flex: none;
  }

  .back-arrow {
    display: inline-flex;
    transform: rotate(90deg);
  }

  .track-title {
    flex: 1;
    min-width: 200px;
  }

  .track-title .vk-title {
    margin: 2px 0 0;
  }

  .track-side {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
  }

  .record {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    font-size: var(--vk-fs-micro);
  }

  .record-time {
    font-size: 24px;
    font-weight: 900;
    line-height: 1;
    color: #ffd166;
    font-variant-numeric: tabular-nums;
    text-shadow: 0 0 14px rgb(255 209 102 / 0.35);
  }

  .count {
    display: grid;
    place-items: center;
    min-width: 20px;
    height: 18px;
    padding: 0 6px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.1);
    font-size: var(--vk-fs-eyebrow);
  }

  /* ---- Tabella dei tempi ---- */

  .table-card {
    padding: 0;
    overflow: hidden;
    transition: opacity var(--vk-dur-fast) var(--vk-ease);
  }

  .table-card.dim {
    opacity: 0.6;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .row {
    display: grid;
    grid-template-columns: 44px minmax(0, 1.4fr) 92px 84px minmax(0, 1.3fr) 96px 150px;
    align-items: center;
    gap: 12px;
    padding: 9px 18px;
    font-size: var(--vk-fs-small);
  }

  .head {
    border-bottom: 1px solid var(--vk-stroke);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .entry {
    border-bottom: 1px solid rgb(255 255 255 / 0.04);
  }

  .entry:hover {
    background: rgb(255 255 255 / 0.03);
  }

  .entry.mine {
    background: color-mix(in srgb, var(--vk-success) 7%, transparent);
  }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .rank {
    font-weight: 900;
    color: var(--vk-text-secondary);
    font-variant-numeric: tabular-nums;
  }

  .rank.gold {
    color: #ffd166;
  }
  .rank.silver {
    color: #d6e0f0;
  }
  .rank.bronze {
    color: #e08a4b;
  }

  .player {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .player-id {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .player-name,
  .mii {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mii {
    font-size: var(--vk-fs-eyebrow);
  }

  .time {
    font-weight: 900;
    color: var(--vk-cyan-soft);
    cursor: default;
  }

  .combo {
    display: flex;
    flex-direction: column;
    min-width: 0;
    font-size: var(--vk-fs-micro);
  }

  .combo span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .date {
    font-size: var(--vk-fs-micro);
  }

  .action {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }

  .installed {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .small {
    padding: 5px 10px;
    font-size: var(--vk-fs-micro);
  }

  .icon-btn {
    padding: 6px 8px;
  }

  .table-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    padding: 10px 18px;
    border-top: 1px solid var(--vk-stroke);
    font-size: var(--vk-fs-micro);
  }

  .pager {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  @media (max-width: 1100px) {
    .row {
      grid-template-columns: 40px minmax(0, 1.4fr) 88px minmax(0, 1fr) 140px;
    }

    .row > :nth-child(4),
    .row > :nth-child(6) {
      display: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .track {
      transition: none;
    }

    .track:hover {
      transform: none;
    }
  }
</style>
