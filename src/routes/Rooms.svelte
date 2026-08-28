<script lang="ts">
  /**
   * Rooms.
   *
   * Ricalca il `RoomsView` del WPF: card riepilogativa in alto con le
   * statistiche globali, poi l'elenco delle stanze con skeleton, stato vuoto
   * e stato di errore distinti.
   *
   * Una cosa che il WPF non mostrava e che qui serve: **chi c'è dentro** una
   * stanza. Il server manda l'elenco insieme alla stanza, prima veniva buttato.
   */
  import { onDestroy } from 'svelte';

  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import MiiAvatar from '$lib/components/MiiAvatar.svelte';
  import type { RoomsSummary, RoomView } from '$lib/api/types';

  /** Lo stesso intervallo di auto-refresh del launcher legacy. */
  const REFRESH_MS = 30_000;

  let summary = $state<RoomsSummary | null>(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state('');
  let expanded = $state<string[]>([]);

  let timer: ReturnType<typeof setInterval> | undefined;

  $effect(() => {
    void load(true);

    timer = setInterval(() => void load(false), REFRESH_MS);
    return () => clearInterval(timer);
  });

  onDestroy(() => clearInterval(timer));

  async function load(showSkeleton: boolean) {
    if (showSkeleton) loading = true;
    refreshing = true;
    try {
      summary = await api.fetchRooms();
      error = '';
    } catch (caught) {
      // Durante l'auto-refresh un errore non svuota l'elenco già mostrato.
      if (showSkeleton) {
        error = api.errorMessage(caught);
        summary = null;
      }
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  function toggle(room: RoomView) {
    expanded = expanded.includes(room.id)
      ? expanded.filter((id) => id !== room.id)
      : [...expanded, room.id];
  }
</script>

<div class="page">
  <section class="vk-card hero vk-rainbow-top">
    <div class="stats">
      <div class="stat">
        <span class="value">{summary?.totalPlayers ?? 0}</span>
        <span class="vk-eyebrow">Giocatori online</span>
      </div>
      <div class="stat">
        <span class="value">{summary?.totalRooms ?? 0}</span>
        <span class="vk-eyebrow">Stanze attive</span>
      </div>
      <div class="stat">
        <span class="value">{summary?.publicRooms ?? 0}</span>
        <span class="vk-eyebrow">Pubbliche</span>
      </div>
      <div class="stat">
        <span class="value">{summary?.privateRooms ?? 0}</span>
        <span class="vk-eyebrow">Private</span>
      </div>
    </div>

    <button class="vk-btn refresh" onclick={() => load(false)} disabled={refreshing}>
      <Icon name="refresh" size={14} />
      {refreshing ? 'Aggiorno…' : 'Aggiorna'}
    </button>
  </section>

  {#if summary?.notice}
    <p class="vk-faint notice">{summary.notice}</p>
  {/if}

  {#if loading}
    {#each [0, 1, 2] as index (index)}
      <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
    {/each}
  {:else if error}
    <div class="vk-error">
      <strong>Impossibile caricare le stanze.</strong>
      <p>{error}</p>
    </div>
  {:else if !summary || summary.rooms.length === 0}
    <div class="vk-card vk-empty">
      <Icon name="rooms" size={28} />
      <p>Nessuna stanza attiva in questo momento.</p>
      <p class="vk-faint">L'elenco si aggiorna da solo ogni 30 secondi.</p>
    </div>
  {:else}
    <div class="rooms">
      {#each summary.rooms as room (room.id)}
        {@const open = expanded.includes(room.id)}
        <article class="vk-card room" class:racing={room.status.toLowerCase() === 'racing'}>
          <header class="room-head">
            <div class="room-id">
              <h3 class="room-name">{room.name}</h3>
              <p class="vk-faint host">Host: {room.host || '—'}</p>
            </div>
            <span
              class="vk-badge {room.status.toLowerCase() === 'racing' ? 'vk-badge--success' : ''}"
            >
              {room.status}
            </span>
          </header>

          <p class="track" title={room.track}>{room.track}</p>

          <div class="fill">
            <div class="fill-track">
              <div
                class="fill-bar"
                style="width: {Math.round(
                  Math.min(1, room.playerCount / Math.max(1, room.maxPlayers)) * 100
                )}%"
              ></div>
            </div>
            <span class="players">{room.playerCount}/{room.maxPlayers}</span>
          </div>

          {#if room.players.length > 0}
            <button class="roster" onclick={() => toggle(room)} aria-expanded={open}>
              <div class="faces">
                {#each room.players.slice(0, 6) as player (player.friendCode || player.name)}
                  <span class="face">
                    <MiiAvatar
                      studioData={player.studioData}
                      initial={player.avatarInitial}
                      accent={player.accentColor}
                      name={player.name}
                      size={26}
                    />
                  </span>
                {/each}
                {#if room.players.length > 6}
                  <span class="more">+{room.players.length - 6}</span>
                {/if}
              </div>
              <span class="vk-faint toggle">
                {open ? 'Nascondi' : 'Giocatori'}
                <Icon name="chevron" size={12} />
              </span>
            </button>

            {#if open}
              <ul class="roster-list">
                {#each room.players as player (player.friendCode || player.name)}
                  <li class="roster-row">
                    <MiiAvatar
                      studioData={player.studioData}
                      initial={player.avatarInitial}
                      accent={player.accentColor}
                      name={player.name}
                      size={30}
                    />
                    <div class="roster-id">
                      <span class="roster-name">
                        {player.name}
                        {#if player.isHost}<span class="vk-badge host-badge">Host</span>{/if}
                      </span>
                      {#if player.friendCode}
                        <span class="vk-mono vk-faint fc">{player.friendCode}</span>
                      {/if}
                    </div>
                    <span class="vk-faint rating">VR {player.vr}</span>
                  </li>
                {/each}
              </ul>
            {/if}
          {/if}

          <footer class="room-foot vk-faint">
            <span>{room.region}</span>
            <span>·</span>
            <span>{room.mode}</span>
          </footer>
        </article>
      {/each}
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

  .hero {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    overflow: hidden;
  }

  .stats {
    display: flex;
    align-items: center;
    gap: 40px;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .value {
    font-size: 32px;
    font-weight: 900;
    line-height: 1;
    background: var(--vk-rainbow);
    background-size: 220% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }

  .refresh {
    flex: none;
  }

  .notice {
    margin: -6px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .rooms {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    align-items: start;
    gap: 14px;
  }

  .room {
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .room:hover {
    border-color: #3a4c74;
  }

  .room.racing {
    border-color: rgb(77 255 176 / 0.35);
  }

  .room-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }

  .room-id {
    min-width: 0;
  }

  .room-name {
    margin: 0;
    font-size: 15px;
    font-weight: 800;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .host {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .track {
    margin: 0;
    font-size: var(--vk-fs-small);
    color: var(--vk-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Il riempimento della stanza si legge prima del numero. */
  .fill {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .fill-track {
    flex: 1;
    height: 4px;
    border-radius: var(--vk-radius-pill);
    background: rgb(255 255 255 / 0.08);
    overflow: hidden;
  }

  .fill-bar {
    height: 100%;
    border-radius: var(--vk-radius-pill);
    background: var(--vk-rainbow);
    transition: width var(--vk-dur) var(--vk-ease);
  }

  .players {
    font-size: var(--vk-fs-micro);
    font-weight: 900;
    color: var(--vk-cyan-soft);
    font-variant-numeric: tabular-nums;
  }

  .roster {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 6px 8px;
    border: 1px solid transparent;
    border-radius: var(--vk-radius-badge);
    background: transparent;
    color: inherit;
    text-align: left;
  }

  .roster:hover {
    border-color: var(--vk-stroke);
    background: rgb(255 255 255 / 0.03);
  }

  .faces {
    display: flex;
    align-items: center;
  }

  /* Le facce si sovrappongono: la fila resta corta anche con 12 giocatori. */
  .face {
    display: inline-flex;
    border-radius: 50%;
    box-shadow: 0 0 0 2px var(--vk-panel);
  }

  .face + .face {
    margin-left: -8px;
  }

  .more {
    margin-left: 6px;
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    color: var(--vk-text-secondary);
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: var(--vk-fs-eyebrow);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .roster-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .roster-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
  }

  .roster-id {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .roster-name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--vk-fs-small);
    font-weight: 700;
  }

  .host-badge {
    padding: 1px 6px;
    font-size: 9px;
  }

  .fc {
    font-size: var(--vk-fs-eyebrow);
  }

  .rating {
    margin-left: auto;
    font-size: var(--vk-fs-micro);
    font-variant-numeric: tabular-nums;
  }

  .room-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: auto;
    font-size: var(--vk-fs-micro);
  }

  .skeleton {
    height: 64px;
  }

  @media (max-width: 900px) {
    .hero {
      flex-wrap: wrap;
      gap: 20px;
    }

    .stats {
      gap: 24px;
    }
  }
</style>
