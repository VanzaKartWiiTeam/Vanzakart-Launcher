<script lang="ts">
  /**
   * Leaderboard.
   *
   * Ricalca il `LeaderboardView` del WPF: podio a tre gradini con i gradienti
   * oro/argento/bronzo, ricerca, tabella con POS/VR/WINS/GAMES e pannello di
   * dettaglio del giocatore selezionato.
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import type { LeaderboardEntry } from '$lib/api/types';

  let entries = $state<LeaderboardEntry[]>([]);
  let loading = $state(true);
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

  const podium = $derived(entries.slice(0, 3));
  const rest = $derived(filtered.filter((entry) => entry.position > 3 || query.trim() !== ''));

  $effect(() => {
    void load();
  });

  async function load() {
    loading = true;
    try {
      entries = await api.fetchLeaderboard(0);
      error = '';
    } catch (caught) {
      error = api.errorMessage(caught);
      entries = [];
    } finally {
      loading = false;
    }
  }

  function medal(position: number): string {
    return position === 1 ? 'podium-1' : position === 2 ? 'podium-2' : 'podium-3';
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
    {#if query.trim() === '' && podium.length > 0}
      <section class="podium">
        {#each podium as entry (entry.friendCode || entry.position)}
          <button class="step {medal(entry.position)}" onclick={() => (selected = entry)}>
            <span class="rank">#{entry.position}</span>
            {#if entry.rankImage}
              <img class="rank-image" src={entry.rankImage} alt="" />
            {/if}
            <span class="name">{entry.name}</span>
            <span class="points">{entry.points.toLocaleString('it-IT')} VR</span>
            <span class="vk-faint sub">{entry.wins} vittorie · {entry.winrate.toFixed(1)}%</span>
          </button>
        {/each}
      </section>
    {/if}

    <section class="vk-card table-card">
      <div class="row head">
        <span>Pos</span>
        <span>Giocatore</span>
        <span class="num">VR</span>
        <span class="num">Vittorie</span>
        <span class="num">Gare</span>
      </div>

      <div class="rows">
        {#each rest as entry (entry.friendCode || entry.position)}
          <button
            class="row entry"
            class:suspicious={entry.isSuspicious}
            onclick={() => (selected = entry)}
          >
            <span class="pos">{entry.position}</span>
            <span class="player">
              {#if entry.rankImage}<img class="rank-mini" src={entry.rankImage} alt="" />{/if}
              <span class="player-name">{entry.name}</span>
              {#if entry.isSuspicious}
                <span class="vk-badge vk-badge--warning">Sospetto</span>
              {/if}
            </span>
            <span class="num strong">{entry.points.toLocaleString('it-IT')}</span>
            <span class="num">{entry.wins}</span>
            <span class="num">{entry.games}</span>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  {#if selected}
    <aside class="vk-card details vk-rainbow-top">
      <header class="details-head">
        <div>
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
        <div><span class="vk-faint">VR 24 h</span><strong>{selected.vrLast24Hours}</strong></div>
        <div><span class="vk-faint">VR settimana</span><strong>{selected.vrLastWeek}</strong></div>
        <div><span class="vk-faint">VR mese</span><strong>{selected.vrLastMonth}</strong></div>
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
    padding: 20px 16px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    text-align: center;
  }

  /* L'ordine visuale è 2° · 1° · 3°, come nel WPF. */
  .podium .step:nth-child(1) {
    order: 2;
    background: var(--vk-podium-1);
    min-height: 210px;
  }
  .podium .step:nth-child(2) {
    order: 1;
    background: var(--vk-podium-2);
    min-height: 178px;
  }
  .podium .step:nth-child(3) {
    order: 3;
    background: var(--vk-podium-3);
    min-height: 166px;
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
    width: 42px;
    height: 42px;
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
    grid-template-columns: 56px 1fr 96px 84px 84px;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 18px;
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

  .entry.suspicious {
    background: rgb(255 209 102 / 0.06);
  }

  .pos {
    font-weight: 900;
    color: var(--vk-text-secondary);
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

  /* --- Dettagli --- */

  .details {
    position: relative;
    overflow: hidden;
  }

  .details-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  .details-name {
    margin: 4px 0 0;
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
      grid-template-columns: 44px 1fr 80px;
    }
    .row > :nth-child(4),
    .row > :nth-child(5) {
      display: none;
    }
  }
</style>
