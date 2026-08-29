<script lang="ts">
  /**
   * Leaderboard.
   *
   * Ricalca il `LeaderboardView` del WPF: podio a tre gradini con i gradienti
   * oro/argento/bronzo, ricerca, tabella con POS/VR/WINS/GAMES e pannello di
   * dettaglio del giocatore selezionato.
   *
   * Come nel legacy ogni riga porta la faccia del giocatore: il server manda
   * il Mii insieme alla classifica, e senza faccia una classifica di nomi
   * corti è illeggibile. La classifica arriva a pagine da cento (§D-058).
   *
   * Il dettaglio sta **accanto** alla tabella, non sotto: sotto lo si trovava
   * solo scorrendo, e chi cliccava una riga non vedeva succedere niente
   * (§D-066).
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import MiiAvatar from '$lib/components/MiiAvatar.svelte';
  import { formatRelative } from '$lib/stores/app.svelte';
  import { formatNumber, t } from '$lib/stores/i18n.svelte';
  import type { LeaderboardEntry } from '$lib/api/types';

  let entries = $state<LeaderboardEntry[]>([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  let hasMore = $state(false);
  let error = $state('');
  let query = $state('');
  let selected = $state<LeaderboardEntry | null>(null);

  const filtered = $derived(
    entries.filter((entry) => {
      const needle = query.trim().toLowerCase();
      if (needle === '') return true;
      return (
        entry.name.toLowerCase().includes(needle) ||
        entry.friendCode.replace(/-/g, '').includes(needle.replace(/-/g, ''))
      );
    })
  );

  const searching = $derived(query.trim() !== '');
  const podium = $derived(entries.slice(0, 3));
  const rest = $derived(filtered.filter((entry) => entry.position > 3 || searching));

  $effect(() => {
    void load();
  });

  async function load() {
    loading = true;
    try {
      const page = await api.fetchLeaderboard(0);
      entries = page.entries;
      hasMore = page.hasMore;
      error = '';
    } catch (caught) {
      error = api.errorMessage(caught);
      entries = [];
      hasMore = false;
    } finally {
      loading = false;
    }
  }

  /** Aggiunge la pagina successiva senza perdere quella già mostrata. */
  async function loadMore() {
    loadingMore = true;
    try {
      const page = await api.fetchLeaderboard(entries.length);
      const known = new Set(entries.map((entry) => entry.position));
      entries = [...entries, ...page.entries.filter((entry) => !known.has(entry.position))];
      hasMore = page.hasMore && page.entries.length > 0;
      error = '';
    } catch (caught) {
      error = api.errorMessage(caught);
      hasMore = false;
    } finally {
      loadingMore = false;
    }
  }

  /** Oro, argento e bronzo: la classe porta il colore a tutto il gradino. */
  function medal(position: number): string {
    return position === 1 ? 'gold' : position === 2 ? 'silver' : 'bronze';
  }

  function gain(value: number): string {
    return value > 0 ? `+${value}` : `${value}`;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && selected) selected = null;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="page">
  <div class="toolbar">
    <input class="vk-input search" bind:value={query} placeholder={t('board.search')} />
    <button class="vk-btn" onclick={load} disabled={loading}>
      <Icon name="refresh" size={14} />
      {loading ? t('common.refreshing') : t('common.refreshAction')}
    </button>
  </div>

  {#if loading}
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
  {:else if error}
    <div class="vk-error">
      <strong>{t('board.unavailable')}</strong>
      <p>{error}</p>
    </div>
  {:else if entries.length === 0}
    <div class="vk-card vk-empty">
      <Icon name="trophy" size={28} />
      <p>{t('board.empty')}</p>
    </div>
  {:else}
    {#if !searching && podium.length > 0}
      <section class="podium">
        {#each podium as entry (entry.friendCode || entry.position)}
          <button
            class="step {medal(entry.position)}"
            class:selected={selected === entry}
            onclick={() => (selected = selected === entry ? null : entry)}
          >
            <span class="place">#{entry.position}</span>

            <span class="face">
              <MiiAvatar
                studioData={entry.studioData}
                initial={entry.avatarInitial}
                accent={entry.accentColor}
                name={entry.name}
                size={entry.position === 1 ? 96 : 76}
                shape="rounded"
              />
            </span>

            <span class="name">{entry.name}</span>
            <span class="points">{t('board.vr', { points: formatNumber(entry.points) })}</span>
            <span class="vk-faint sub">
              {t('board.podiumSub', { wins: entry.wins, winrate: entry.winrate.toFixed(1) })}
            </span>

            {#if entry.rankImage}
              <img
                class="rank-image"
                src={entry.rankImage}
                alt={t('board.rank', { rank: entry.prestigeRank })}
                title={t('board.rank', { rank: entry.prestigeRank })}
              />
            {/if}
          </button>
        {/each}
      </section>
    {/if}

    <div class="board" class:with-details={selected !== null}>
      <section class="vk-card table-card">
        <div class="row head">
          <span>{t('board.col.pos')}</span>
          <span>{t('board.col.player')}</span>
          <span class="num">{t('board.col.vr')}</span>
          <span class="num">{t('board.col.day')}</span>
          <span class="num">{t('board.col.wins')}</span>
          <span class="num">{t('board.col.games')}</span>
        </div>

        <div class="rows">
          {#each rest as entry (entry.friendCode || entry.position)}
            <button
              class="row entry"
              class:suspicious={entry.isSuspicious}
              class:selected={selected === entry}
              onclick={() => (selected = selected === entry ? null : entry)}
            >
              <span class="pos">{entry.position}</span>
              <span class="player">
                <MiiAvatar
                  studioData={entry.studioData}
                  initial={entry.avatarInitial}
                  accent={entry.accentColor}
                  name={entry.name}
                  size={30}
                />
                {#if entry.rankImage}
                  <img
                    class="rank-mini"
                    src={entry.rankImage}
                    alt={t('board.rank', { rank: entry.prestigeRank })}
                    title={t('board.rank', { rank: entry.prestigeRank })}
                  />
                {/if}
                <span class="player-name">{entry.name}</span>
                {#if entry.isSuspicious}
                  <span class="vk-badge vk-badge--warning">{t('board.suspicious')}</span>
                {/if}
              </span>
              <span class="num strong">{formatNumber(entry.points)}</span>
              <span
                class="num gain"
                class:up={entry.vrLast24Hours > 0}
                class:down={entry.vrLast24Hours < 0}
              >
                {entry.vrLast24Hours === 0 ? t('common.dash') : gain(entry.vrLast24Hours)}
              </span>
              <span class="num">{entry.wins}</span>
              <span class="num">{entry.games}</span>
            </button>
          {:else}
            <p class="vk-faint no-match">{t('board.noMatch')}</p>
          {/each}
        </div>

        <footer class="table-foot">
          <span class="vk-faint">
            {searching
              ? rest.length === 1
                ? t('board.resultOne', { count: rest.length })
                : t('board.resultMany', { count: rest.length })
              : t('board.playerCount', { count: entries.length })}
          </span>
          {#if hasMore && !searching}
            <button class="vk-btn" onclick={loadMore} disabled={loadingMore}>
              {loadingMore ? t('board.loadingMore') : t('board.loadMore')}
            </button>
          {/if}
        </footer>
      </section>

      {#if selected}
        {@const player = selected}
        <aside class="vk-card details vk-rainbow-top">
          <header class="details-head">
            <MiiAvatar
              studioData={player.studioData}
              initial={player.avatarInitial}
              accent={player.accentColor}
              name={player.name}
              size={64}
              shape="rounded"
            />
            <div class="details-id">
              <p class="vk-eyebrow">{t('board.position', { position: player.position })}</p>
              <h3 class="details-name">{player.name}</h3>
              {#if player.rankImage}
                <p class="details-rank">
                  <img src={player.rankImage} alt="" />
                  {t('board.rankShort', { rank: player.prestigeRank })}
                </p>
              {/if}
            </div>
            <button
              class="close"
              onclick={() => (selected = null)}
              title={t('board.closeDetailsHint')}
              aria-label={t('board.closeDetails')}
            >
              <Icon name="close" size={14} />
            </button>
          </header>

          <p class="headline">{formatNumber(player.points)} <span>VR</span></p>

          <div class="details-grid">
            <div>
              <span class="vk-faint">{t('board.wins')}</span><strong>{player.wins}</strong>
            </div>
            <div>
              <span class="vk-faint">{t('board.games')}</span><strong>{player.games}</strong>
            </div>
            <div>
              <span class="vk-faint">{t('board.winRate')}</span>
              <strong>{player.winrate.toFixed(1)}%</strong>
            </div>
            <div>
              <span class="vk-faint">{t('board.online')}</span>
              <strong>{formatRelative(player.lastSeen) || t('common.dash')}</strong>
            </div>
          </div>

          <div class="trend">
            <p class="vk-eyebrow">{t('board.trend')}</p>
            <div class="trend-row">
              <span class="vk-faint">{t('board.trend24')}</span>
              <strong class:up={player.vrLast24Hours > 0} class:down={player.vrLast24Hours < 0}>
                {gain(player.vrLast24Hours)}
              </strong>
            </div>
            <div class="trend-row">
              <span class="vk-faint">{t('board.trendWeek')}</span>
              <strong class:up={player.vrLastWeek > 0} class:down={player.vrLastWeek < 0}>
                {gain(player.vrLastWeek)}
              </strong>
            </div>
            <div class="trend-row">
              <span class="vk-faint">{t('board.trendMonth')}</span>
              <strong class:up={player.vrLastMonth > 0} class:down={player.vrLastMonth < 0}>
                {gain(player.vrLastMonth)}
              </strong>
            </div>
          </div>

          <p class="fc">
            <span class="vk-faint">{t('board.friendCode')}</span>
            <span class="vk-mono">{player.friendCode || t('common.dash')}</span>
          </p>

          {#if player.isSuspicious}
            <p class="vk-badge vk-badge--warning flagged">{t('board.flagged')}</p>
          {/if}
        </aside>
      {/if}
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-bottom: 12px;
  }

  .toolbar {
    display: flex;
    gap: 12px;
  }

  .search {
    flex: 1;
  }

  /* --- Podio --- */

  .podium {
    display: grid;
    grid-template-columns: 1fr 1.15fr 1fr;
    align-items: end;
    gap: 14px;
  }

  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 18px 16px;
    border: 1px solid color-mix(in srgb, var(--medal) 42%, var(--vk-stroke));
    border-radius: var(--vk-radius-card);
    text-align: center;
    transition:
      transform var(--vk-dur-fast) var(--vk-ease),
      box-shadow var(--vk-dur-fast) var(--vk-ease);
  }

  .step:hover {
    transform: translateY(-2px);
  }

  .step.selected {
    box-shadow: 0 0 0 1px var(--medal) inset;
  }

  /* Oro, argento e bronzo: un colore solo per gradino, ripreso da cornice,
     posizione, nome e alone. */
  .gold {
    --medal: #ffd166;
  }
  .silver {
    --medal: #d6e0f0;
  }
  .bronze {
    --medal: #e08a4b;
  }

  /* L'ordine visuale è 2° · 1° · 3°, come nel WPF. */
  .podium .step:nth-child(1) {
    order: 2;
    background: var(--vk-podium-1);
    min-height: 330px;
    box-shadow: var(--vk-glow-gold);
  }
  .podium .step:nth-child(2) {
    order: 1;
    background: var(--vk-podium-2);
    min-height: 300px;
  }
  .podium .step:nth-child(3) {
    order: 3;
    background: var(--vk-podium-3);
    min-height: 276px;
  }

  .place {
    font-size: var(--vk-fs-eyebrow);
    font-weight: 900;
    letter-spacing: 0.06em;
    color: var(--medal);
  }

  /* Cornice della medaglia attorno alla faccia. */
  .face {
    display: inline-flex;
    padding: 3px;
    border-radius: 22px;
    background: linear-gradient(
      150deg,
      var(--medal),
      color-mix(in srgb, var(--medal) 25%, #0b1020)
    );
    box-shadow: 0 0 18px color-mix(in srgb, var(--medal) 40%, transparent);
  }

  .name {
    font-size: 17px;
    font-weight: 900;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .points {
    font-size: 16px;
    font-weight: 900;
    color: var(--medal);
  }

  .sub {
    font-size: var(--vk-fs-micro);
  }

  .rank-image {
    width: 40px;
    height: 40px;
    object-fit: contain;
  }

  /* --- Tabella e dettagli --- */

  .board {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }

  .board.with-details {
    grid-template-columns: minmax(0, 1fr) 340px;
  }

  .table-card {
    padding: 0;
    overflow: hidden;
  }

  .row {
    display: grid;
    grid-template-columns: 52px 1fr 92px 72px 78px 72px;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px 18px;
    text-align: left;
    background: transparent;
    border: none;
    color: inherit;
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

  .rows {
    max-height: 460px;
    overflow-y: auto;
  }

  .entry:hover {
    background: rgb(255 255 255 / 0.04);
  }

  .entry.selected {
    background: rgb(0 242 255 / 0.1);
    box-shadow: inset 2px 0 0 var(--vk-cyan);
  }

  .entry.suspicious {
    background: rgb(255 209 102 / 0.06);
  }

  .pos {
    font-weight: 900;
    color: var(--vk-text-secondary);
    font-variant-numeric: tabular-nums;
  }

  .player {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .player-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 700;
  }

  .rank-mini {
    width: 22px;
    height: 22px;
    object-fit: contain;
    flex: none;
  }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .strong {
    font-weight: 800;
    color: var(--vk-cyan-soft);
  }

  .gain {
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-faint);
  }

  .gain.up,
  .trend strong.up {
    color: var(--vk-success);
  }

  .gain.down,
  .trend strong.down {
    color: var(--vk-danger);
  }

  .no-match {
    padding: 18px;
    margin: 0;
    text-align: center;
    font-size: var(--vk-fs-small);
  }

  .table-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 18px;
    border-top: 1px solid var(--vk-stroke);
    font-size: var(--vk-fs-micro);
  }

  /* --- Dettagli --- */

  .details {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-height: calc(100vh - var(--vk-header-h) - var(--vk-titlebar-h) - 40px);
    overflow: hidden auto;
  }

  .details-head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .details-id {
    min-width: 0;
    margin-right: auto;
  }

  .details-name {
    margin: 2px 0 0;
    font-size: 20px;
    font-weight: 900;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .details-rank {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px 0 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .details-rank img {
    width: 22px;
    height: 22px;
    object-fit: contain;
  }

  .close {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border: 1px solid var(--vk-stroke);
    border-radius: 9px;
    background: transparent;
    color: var(--vk-text-secondary);
  }

  .close:hover {
    border-color: var(--vk-danger);
    color: var(--vk-danger);
  }

  .headline {
    margin: 0;
    font-size: 30px;
    font-weight: 900;
    line-height: 1;
    color: var(--vk-cyan-soft);
    font-variant-numeric: tabular-nums;
  }

  .headline span {
    font-size: 14px;
    color: var(--vk-text-secondary);
  }

  .details-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .details-grid div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--vk-fs-small);
  }

  .details-grid .vk-faint {
    font-size: var(--vk-fs-eyebrow);
  }

  .trend {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
  }

  .trend-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    font-size: var(--vk-fs-small);
    font-variant-numeric: tabular-nums;
  }

  .fc {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    font-size: var(--vk-fs-small);
  }

  .fc .vk-faint {
    font-size: var(--vk-fs-eyebrow);
  }

  .flagged {
    align-self: flex-start;
  }

  .skeleton {
    height: 220px;
  }

  /* Sotto i 1200 px la colonna non ci sta: il dettaglio torna in linea, ma
     sopra la tabella, dove si vede senza scorrere. */
  @media (max-width: 1200px) {
    .board.with-details {
      grid-template-columns: minmax(0, 1fr);
    }

    .details {
      position: static;
      order: -1;
      max-height: none;
    }
  }

  @media (max-width: 900px) {
    .podium {
      grid-template-columns: 1fr;
    }
    .podium .step {
      order: 0 !important;
      min-height: 0 !important;
    }
    .row {
      grid-template-columns: 40px 1fr 80px 64px;
    }
    .row > :nth-child(5),
    .row > :nth-child(6) {
      display: none;
    }
  }
</style>
