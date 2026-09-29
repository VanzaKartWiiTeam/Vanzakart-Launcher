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
   * Il nome mostrato è quello del giocatore, cioè del Mii che ha corso: il
   * server manda anche il profilo con cui il tempo è stato caricato, che
   * spesso è quello di sistema e non si mostra mai (vedi `ghosts.rs`).
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
  /** Il tempo di cui si vedono i dettagli. */
  let expanded = $state<number | null>(null);

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

  /**
   * Le piste che si vedono. Quelle con un record vengono prima: sono le
   * poche con qualcosa da battere, e altrimenti si perderebbero fra le
   * duecento che non ne hanno ancora uno.
   */
  const tracks = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    const shown = (catalog?.tracks ?? []).filter((track) => {
      if (needle && !track.name.toLowerCase().includes(needle)) return false;
      if (category !== 'all' && track.category !== category) return false;
      if (onlyRecords && !track.record) return false;
      return true;
    });
    return [...shown].sort((a, b) => Number(b.record !== null) - Number(a.record !== null));
  });

  const withRecord = $derived((catalog?.tracks ?? []).filter((track) => track.record).length);

  /**
   * La categoria della maggior parte delle piste. Scritta su duecento card
   * non dice niente: si mostra solo quella delle altre, che è l'eccezione.
   */
  const commonCategory = $derived.by(() => {
    const counts: Record<string, number> = {};
    for (const track of catalog?.tracks ?? []) {
      counts[track.category] = (counts[track.category] ?? 0) + 1;
    }
    return Object.entries(counts).sort((a, b) => b[1] - a[1])[0]?.[0] ?? '';
  });
  const filtering = $derived(query.trim() !== '' || category !== 'all' || onlyRecords);

  /** Il tempo da cui si misura il distacco: il primo della classifica. */
  const leaderMs = $derived.by(() => {
    const first = board?.entries[0];
    if (first && first.rank === 1) return first.finishTimeMs;
    return selected?.record?.finishTimeMs ?? null;
  });

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
    expanded = null;
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
    expanded = null;
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

  function playerName(name: string): string {
    return name || t('ghosts.unknownPlayer');
  }

  /** Aggiorna una pista ovunque compaia: lista, testata e classifica. */
  function patchTrack(trackId: number, update: (track: GhostTrackView) => GhostTrackView) {
    if (selected?.id === trackId) selected = update(selected);
    if (board?.track.id === trackId) board = { ...board, track: update(board.track) };
    if (catalog) {
      catalog = {
        ...catalog,
        tracks: catalog.tracks.map((track) => (track.id === trackId ? update(track) : track))
      };
    }
  }

  function setInstalled(trackId: number, submissionId: number, installed: boolean, count: number) {
    if (board) {
      board = {
        ...board,
        entries: board.entries.map((item) =>
          item.submissionId === submissionId ? { ...item, installed } : item
        )
      };
    }
    patchTrack(trackId, (track) => ({
      ...track,
      installedCount: count,
      record:
        track.record?.submissionId === submissionId ? { ...track.record, installed } : track.record
    }));
  }

  async function download(
    track: GhostTrackView,
    submissionId: number,
    player: string,
    time: string
  ): Promise<void> {
    if (working.includes(submissionId)) return;
    working = [...working, submissionId];
    try {
      const outcome = await api.installGhost(track.id, submissionId);
      setInstalled(track.id, submissionId, true, outcome.installedCount);
      app.toast(
        outcome.alreadyPresent ? t('ghosts.alreadyInstalled') : t('ghosts.installed'),
        t('ghosts.installedBody', {
          player: playerName(player),
          time,
          track: outcome.trackName
        }),
        'success'
      );
    } catch (caught) {
      app.toast(t('ghosts.installFailed'), api.errorMessage(caught), 'warning');
    } finally {
      working = working.filter((id) => id !== submissionId);
    }
  }

  async function remove(entry: GhostEntryView) {
    if (!selected || working.includes(entry.submissionId)) return;
    const trackId = selected.id;
    working = [...working, entry.submissionId];
    try {
      const left = await api.removeGhost(entry.submissionId);
      setInstalled(trackId, entry.submissionId, false, left);
      app.toast(
        t('ghosts.removed'),
        t('ghosts.removedBody', { player: playerName(entry.playerName) }),
        'info'
      );
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

  /** Distacco dal primo, come lo scrive il sito: `+0.412`, `+1:02.310`. */
  function gap(ms: number): string {
    const minutes = Math.floor(ms / 60_000);
    const seconds = ((ms % 60_000) / 1000).toFixed(3);
    return minutes > 0 ? `+${minutes}:${seconds.padStart(6, '0')}` : `+${seconds}`;
  }

  function toggle(entry: GhostEntryView) {
    expanded = expanded === entry.submissionId ? null : entry.submissionId;
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

      {#if track.record}
        <div class="record">
          <span class="vk-eyebrow">{t('ghosts.record')}</span>
          <strong class="record-time">{track.record.finishTime}</strong>
          <span class="record-holder">
            {#if track.record.country}<span class="flag">{track.record.country}</span>{/if}
            {playerName(track.record.playerName)}
          </span>
        </div>
      {/if}

      {#if track.installable}
        <button
          class="vk-btn folder"
          onclick={() => openFolder(track.id)}
          title={t('ghosts.whereHint')}
        >
          <Icon name="folder" size={14} />
          {t('ghosts.folder')}
          {#if track.installedCount > 0}
            <span class="count">{track.installedCount}</span>
          {/if}
        </button>
      {/if}
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
          <span>{t('ghosts.col.combo')}</span>
          <span class="num">{t('ghosts.col.ghost')}</span>
        </div>

        <ul class="rows">
          {#each board.entries as entry (entry.submissionId)}
            {@const busy = working.includes(entry.submissionId)}
            {@const open = expanded === entry.submissionId}
            <li class="entry" class:mine={entry.installed} class:open>
              <div class="row">
                <span class="rank {medal(entry.rank)}">{entry.rank}</span>

                <!-- Il nome apre i dettagli del tempo: giri, controller, data. -->
                <button
                  class="player"
                  aria-expanded={open}
                  title={t('ghosts.details')}
                  onclick={() => toggle(entry)}
                >
                  {#if entry.country}
                    <span class="flag" title={entry.countryName || entry.country}>
                      {entry.country}
                    </span>
                  {/if}
                  <strong class="player-name">{playerName(entry.playerName)}</strong>
                  {#if entry.profileName}
                    <span class="profile" title={entry.profileName}>{entry.profileName}</span>
                  {/if}
                  <span class="caret" class:up={open} aria-hidden="true">
                    <Icon name="chevron" size={12} />
                  </span>
                </button>

                <span class="num time-cell">
                  <strong class="time">{entry.finishTime}</strong>
                  {#if leaderMs !== null && entry.finishTimeMs > leaderMs}
                    <span class="gap">{gap(entry.finishTimeMs - leaderMs)}</span>
                  {/if}
                </span>

                <span class="combo">
                  {entry.character || t('common.dash')}
                  {#if entry.vehicle}<span class="vk-faint"> · {entry.vehicle}</span>{/if}
                </span>

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
                      onclick={() =>
                        download(track, entry.submissionId, entry.playerName, entry.finishTime)}
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
              </div>

              {#if open}
                <dl class="details">
                  {#if entry.lapSplits.length > 0}
                    <div>
                      <dt>{t('ghosts.detail.laps')}</dt>
                      <dd class="splits">
                        {#each entry.lapSplits as lap, index (index)}
                          <span class:best={lap === entry.fastestLap}>{lap}</span>
                        {/each}
                      </dd>
                    </div>
                  {/if}
                  {#if entry.fastestLap}
                    <div>
                      <dt>{t('ghosts.detail.bestLap')}</dt>
                      <dd>{entry.fastestLap}</dd>
                    </div>
                  {/if}
                  {#if CONTROLLERS[entry.controller]}
                    <div>
                      <dt>{t('ghosts.detail.controller')}</dt>
                      <dd>{t(CONTROLLERS[entry.controller]!)}</dd>
                    </div>
                  {/if}
                  <div>
                    <dt>{t('ghosts.detail.drift')}</dt>
                    <dd>
                      {entry.automaticDrift ? t('ghosts.drift.auto') : t('ghosts.drift.manual')}
                      {#if entry.shroomless}· {t('ghosts.shroomless')}{/if}
                    </dd>
                  </div>
                  {#if entry.dateSet}
                    <div>
                      <dt>{t('ghosts.detail.date')}</dt>
                      <dd>{formatDate(entry.dateSet)}</dd>
                    </div>
                  {/if}
                </dl>
              {/if}
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

      {#if categories.length > 1}
        <div class="segmented" role="group" aria-label={t('ghosts.track')}>
          <button class:active={category === 'all'} onclick={() => (category = 'all')}>
            {t('ghosts.allTracks')}
          </button>
          {#each categories as item (item)}
            <button class:active={category === item} onclick={() => (category = item)}>
              {item}
            </button>
          {/each}
        </div>
      {/if}

      <button
        class="chip"
        class:active={onlyRecords}
        aria-pressed={onlyRecords}
        onclick={() => (onlyRecords = !onlyRecords)}
        disabled={withRecord === 0}
      >
        <Icon name="stopwatch" size={12} />
        {t('ghosts.onlyRecords', { count: withRecord })}
      </button>

      <span class="vk-spacer"></span>

      <button
        class="vk-btn icon-btn"
        onclick={() => load(true)}
        disabled={loading}
        title={t('common.refreshAction')}
        aria-label={t('common.refreshAction')}
      >
        <span class:spinning={loading}><Icon name="refresh" size={15} /></span>
      </button>
      {#if catalog && !catalog.blocker}
        <button
          class="vk-btn icon-btn"
          onclick={() => openFolder()}
          title={t('ghosts.allFoldersHint')}
          aria-label={t('ghosts.allFolders')}
        >
          <Icon name="folder" size={15} />
        </button>
      {/if}
    </div>

    {#if catalog?.blocker}
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

    {#if catalog && !catalog.recordsAvailable}
      <p class="vk-faint hint">{t('ghosts.recordsUnavailable')}</p>
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
      {#if filtering}
        <p class="vk-faint hint">
          {t('ghosts.count', { shown: tracks.length, total: catalog.tracks.length })}
        </p>
      {/if}

      <ul class="grid">
        {#each tracks as track (track.id)}
          {@const record = track.record}
          <li
            class="track"
            class:has-record={record !== null}
            class:unavailable={!track.installable && track.blocker === 'track-not-in-modpack'}
            title={track.installable ? undefined : blockerText(track.blocker)}
          >
            <!-- Il pulsante copre tutta la card; l'azione sul record sta sopra. -->
            <button class="track-open" onclick={() => openTrack(track)}>
              <span class="track-top">
                {#if track.category && track.category !== commonCategory}
                  <span class="vk-badge track-category">{track.category}</span>
                {/if}
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
            </button>

            {#if record}
              <div class="track-record">
                <strong>{record.finishTime}</strong>
                <span class="holder">
                  {#if record.country}<span class="flag">{record.country}</span>{/if}
                  {playerName(record.playerName)}
                </span>
                {#if track.installable}
                  {#if record.installed}
                    <span class="record-done" title={t('ghosts.recordInstalled')}>
                      <Icon name="check" size={13} label={t('ghosts.recordInstalled')} />
                    </span>
                  {:else}
                    <button
                      class="vk-btn icon-btn record-get"
                      title={t('ghosts.downloadRecord')}
                      aria-label={t('ghosts.downloadRecord')}
                      disabled={working.includes(record.submissionId)}
                      onclick={() =>
                        download(track, record.submissionId, record.playerName, record.finishTime)}
                    >
                      <Icon name="download" size={13} />
                    </button>
                  {/if}
                {/if}
              </div>
            {:else}
              <span class="vk-faint track-record empty">{t('ghosts.noRecord')}</span>
            {/if}
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
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .search {
    flex: 1 1 220px;
    min-width: 200px;
    max-width: 420px;
  }

  /* Categorie: una scelta sola fra poche, quindi un controllo segmentato. */
  .segmented {
    display: inline-flex;
    padding: 3px;
    border: 1px solid var(--vk-stroke);
    border-radius: 999px;
    background: var(--vk-input);
  }

  .segmented button {
    padding: 5px 14px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    cursor: pointer;
  }

  .segmented button.active {
    background: var(--vk-active-surface);
    color: var(--vk-text);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--vk-cyan) 40%, transparent);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
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

  .icon-btn {
    padding: 7px 9px;
  }

  .spinning {
    display: inline-flex;
    animation: spin 1s linear infinite;
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

  .hint {
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
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel-soft);
    transition:
      transform var(--vk-dur-fast) var(--vk-ease),
      border-color var(--vk-dur-fast) var(--vk-ease),
      box-shadow var(--vk-dur-fast) var(--vk-ease);
  }

  .track:hover {
    transform: translateY(-2px);
    border-color: #3a4c74;
  }

  .track:focus-within {
    border-color: var(--vk-cyan);
  }

  /* Una pista con un record ha qualcosa da battere: si accende. */
  .track.has-record {
    border-color: color-mix(in srgb, #ffd166 35%, var(--vk-stroke));
    box-shadow: 0 0 16px rgb(255 209 102 / 0.08);
  }

  .track.unavailable {
    opacity: 0.55;
  }

  .track-open {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    outline: none;
  }

  /* Tutta la card apre la pista, non solo il titolo. */
  .track-open::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
  }

  .track-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .track-category,
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
    align-items: center;
    gap: 8px;
    margin-top: auto;
    min-width: 0;
    font-size: var(--vk-fs-micro);
  }

  .track-record strong {
    font-size: 16px;
    font-weight: 900;
    color: #ffd166;
    font-variant-numeric: tabular-nums;
  }

  .holder {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 700;
  }

  .track-record.empty {
    font-size: var(--vk-fs-eyebrow);
  }

  /* L'azione sul record sta sopra il pulsante che copre la card. */
  .record-get,
  .record-done {
    position: relative;
    z-index: 1;
    flex: none;
  }

  .record-get {
    padding: 5px 7px;
  }

  .record-done {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 8px;
    color: var(--vk-success);
    background: color-mix(in srgb, var(--vk-success) 14%, transparent);
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

  .record {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    font-size: var(--vk-fs-micro);
  }

  .record-time {
    font-size: 26px;
    font-weight: 900;
    line-height: 1;
    color: #ffd166;
    font-variant-numeric: tabular-nums;
    text-shadow: 0 0 14px rgb(255 209 102 / 0.35);
  }

  .record-holder {
    font-weight: 800;
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
    grid-template-columns: 44px minmax(0, 1.6fr) 112px minmax(0, 1.2fr) 168px;
    align-items: center;
    gap: 12px;
    padding: 8px 18px;
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

  .entry:hover,
  .entry.open {
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
    padding: 4px 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .player-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 15px;
    font-weight: 900;
  }

  .player:hover .player-name {
    color: var(--vk-cyan-soft);
  }

  .profile {
    flex: none;
    max-width: 40%;
    padding: 1px 7px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.07);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .caret {
    display: inline-flex;
    flex: none;
    color: var(--vk-text-faint);
    transition: transform var(--vk-dur-fast) var(--vk-ease);
  }

  .caret.up {
    transform: rotate(180deg);
  }

  .time-cell {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    line-height: 1.2;
  }

  .time {
    font-weight: 900;
    color: var(--vk-cyan-soft);
  }

  .gap {
    font-size: var(--vk-fs-eyebrow);
    color: var(--vk-text-faint);
  }

  .combo {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  .details {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 28px;
    margin: 0;
    padding: 4px 18px 14px 74px;
    font-size: var(--vk-fs-micro);
  }

  .details div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .details dt {
    color: var(--vk-text-faint);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .details dd {
    margin: 0;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .splits {
    display: flex;
    gap: 10px;
  }

  .splits .best {
    color: var(--vk-cyan-soft);
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

  @media (max-width: 1000px) {
    .row {
      grid-template-columns: 40px minmax(0, 1.4fr) 100px 150px;
    }

    .row > :nth-child(4) {
      display: none;
    }

    .details {
      padding-left: 18px;
    }
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .track,
    .caret {
      transition: none;
    }

    .track:hover {
      transform: none;
    }

    .spinning {
      animation: none;
    }
  }
</style>
