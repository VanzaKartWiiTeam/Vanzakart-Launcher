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
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import MiiAvatar from '$lib/components/MiiAvatar.svelte';
  import { formatRelative } from '$lib/stores/app.svelte';
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

  function medal(position: number): string {
    return position === 1 ? 'podium-1' : position === 2 ? 'podium-2' : 'podium-3';
  }

  function gain(value: number): string {
    return value > 0 ? `+${value}` : `${value}`;
  }
</script>

<div class="page">
  <div class="toolbar">
    <input class="vk-input search" bind:value={query} placeholder="Cerca per nome o friend code…" />
    <button class="vk-btn" onclick={load} disabled={loading}>
      <Icon name="refresh" size={14} />
      {loading ? 'Aggiorno…' : 'Aggiorna'}
    </button>
  </div>

  {#if loading}
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
  {:else if error}
    <div class="vk-error">
      <strong>Classifica non disponibile.</strong>
      <p>{error}</p>
    </div>
  {:else if entries.length === 0}
    <div class="vk-card vk-empty">
      <Icon name="trophy" size={28} />
      <p>Nessun giocatore in classifica.</p>
    </div>
  {:else}
    {#if !searching && podium.length > 0}
      <section class="podium">
        {#each podium as entry (entry.friendCode || entry.position)}
          <button class="step {medal(entry.position)}" onclick={() => (selected = entry)}>
            <span class="rank">#{entry.position}</span>
            <MiiAvatar
              studioData={entry.studioData}
              initial={entry.avatarInitial}
              accent={entry.accentColor}
              name={entry.name}
              size={entry.position === 1 ? 68 : 56}
              shape="rounded"
            />
            <span class="name">{entry.name}</span>
            <span class="points">{entry.points.toLocaleString('it-IT')} VR</span>
            <span class="vk-faint sub">{entry.wins} vittorie · {entry.winrate.toFixed(1)}%</span>
            {#if entry.rankImage}
              <img class="rank-image" src={entry.rankImage} alt="" />
            {/if}
          </button>
        {/each}
      </section>
    {/if}

    <section class="vk-card table-card">
      <div class="row head">
        <span>Pos</span>
        <span>Giocatore</span>
        <span class="num">VR</span>
        <span class="num">24 h</span>
        <span class="num">Vittorie</span>
        <span class="num">Gare</span>
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
              <span class="player-name">{entry.name}</span>
              {#if entry.rankImage}<img class="rank-mini" src={entry.rankImage} alt="" />{/if}
              {#if entry.isSuspicious}
                <span class="vk-badge vk-badge--warning">Sospetto</span>
              {/if}
            </span>
            <span class="num strong">{entry.points.toLocaleString('it-IT')}</span>
            <span
              class="num gain"
              class:up={entry.vrLast24Hours > 0}
              class:down={entry.vrLast24Hours < 0}
            >
              {entry.vrLast24Hours === 0 ? '—' : gain(entry.vrLast24Hours)}
            </span>
            <span class="num">{entry.wins}</span>
            <span class="num">{entry.games}</span>
          </button>
        {:else}
          <p class="vk-faint no-match">Nessun giocatore corrisponde alla ricerca.</p>
        {/each}
      </div>

      <footer class="table-foot">
        <span class="vk-faint">
          {searching
            ? `${rest.length} ${rest.length === 1 ? 'risultato' : 'risultati'}`
            : `${entries.length} giocatori`}
        </span>
        {#if hasMore && !searching}
          <button class="vk-btn" onclick={loadMore} disabled={loadingMore}>
            {loadingMore ? 'Carico…' : 'Carica altri'}
          </button>
        {/if}
      </footer>
    </section>
  {/if}

  {#if selected}
    <aside class="vk-card details vk-rainbow-top">
      <header class="details-head">
        <MiiAvatar
          studioData={selected.studioData}
          initial={selected.avatarInitial}
          accent={selected.accentColor}
          name={selected.name}
          size={56}
          shape="rounded"
        />
        <div class="details-id">
          <p class="vk-eyebrow">Dettagli giocatore</p>
          <h3 class="details-name">{selected.name}</h3>
        </div>
        <button class="vk-btn vk-btn--ghost" onclick={() => (selected = null)}>Chiudi</button>
      </header>

      <div class="details-grid">
        <div><span class="vk-faint">Posizione</span><strong>#{selected.position}</strong></div>
        <div>
          <span class="vk-faint">VR</span><strong>{selected.points.toLocaleString('it-IT')}</strong>
        </div>
        <div>
          <span class="vk-faint">Prestigio</span><strong>{selected.prestigeRank || '—'}</strong>
        </div>
        <div>
          <span class="vk-faint">Friend code</span><strong class="vk-mono"
            >{selected.friendCode || '—'}</strong
          >
        </div>
        <div><span class="vk-faint">Vittorie</span><strong>{selected.wins}</strong></div>
        <div><span class="vk-faint">Gare</span><strong>{selected.games}</strong></div>
        <div>
          <span class="vk-faint">Win rate</span><strong>{selected.winrate.toFixed(1)}%</strong>
        </div>
        <div>
          <span class="vk-faint">Online</span><strong
            >{formatRelative(selected.lastSeen) || '—'}</strong
          >
        </div>
        <div>
          <span class="vk-faint">VR 24 h</span><strong>{gain(selected.vrLast24Hours)}</strong>
        </div>
        <div>
          <span class="vk-faint">VR settimana</span><strong>{gain(selected.vrLastWeek)}</strong>
        </div>
        <div>
          <span class="vk-faint">VR mese</span><strong>{gain(selected.vrLastMonth)}</strong>
        </div>
      </div>
    </aside>
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
    gap: 6px;
    padding: 18px 16px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    text-align: center;
  }

  /* L'ordine visuale è 2° · 1° · 3°, come nel WPF. */
  .podium .step:nth-child(1) {
    order: 2;
    background: var(--vk-podium-1);
    min-height: 244px;
  }
  .podium .step:nth-child(2) {
    order: 1;
    background: var(--vk-podium-2);
    min-height: 216px;
  }
  .podium .step:nth-child(3) {
    order: 3;
    background: var(--vk-podium-3);
    min-height: 198px;
  }

  .podium .step:nth-child(1) {
    border-color: rgb(255 209 102 / 0.55);
    box-shadow: var(--vk-glow-gold);
  }

  .rank {
    font-size: var(--vk-fs-eyebrow);
    font-weight: 900;
    color: var(--vk-text-secondary);
  }

  .rank-image {
    width: 34px;
    height: 34px;
    object-fit: contain;
  }

  .name {
    font-size: 17px;
    font-weight: 900;
  }

  .points {
    font-size: 15px;
    font-weight: 800;
    color: var(--vk-cyan-soft);
  }

  .sub {
    font-size: var(--vk-fs-micro);
  }

  /* --- Tabella --- */

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
    background: rgb(0 242 255 / 0.08);
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
    width: 20px;
    height: 20px;
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

  .gain.up {
    color: var(--vk-success);
  }

  .gain.down {
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
    position: relative;
    overflow: hidden;
  }

  .details-head {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 14px;
  }

  .details-id {
    min-width: 0;
    margin-right: auto;
  }

  .details-name {
    margin: 2px 0 0;
    font-size: 22px;
    font-weight: 900;
  }

  .details-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 12px;
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

  .skeleton {
    height: 220px;
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
