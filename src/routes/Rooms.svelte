<script lang="ts">
  /**
   * Rooms.
   *
   * Ricalca il `RoomsView` del WPF: card riepilogativa in alto con le
   * statistiche globali, poi l'elenco delle stanze con skeleton, stato vuoto
   * e stato di errore distinti.
   */
  import { onDestroy } from 'svelte';

  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import type { RoomsSummary } from '$lib/api/types';

  /** Lo stesso intervallo di auto-refresh del launcher legacy. */
  const REFRESH_MS = 30_000;

  let summary = $state<RoomsSummary | null>(null);
  let loading = $state(true);
  let error = $state('');

  let timer: ReturnType<typeof setInterval> | undefined;

  $effect(() => {
    void load(true);

    timer = setInterval(() => void load(false), REFRESH_MS);
    return () => clearInterval(timer);
  });

  onDestroy(() => clearInterval(timer));

  async function load(showSkeleton: boolean) {
    if (showSkeleton) loading = true;
    try {
      summary = await api.fetchRooms();
      error = '';
    } catch (caught) {
      // Durante l'auto-refresh non si svuota l'elenco già mostrato.
      if (showSkeleton) {
        error = api.errorMessage(caught);
        summary = null;
      }
    } finally {
      loading = false;
    }
  }
</script>

<div class="page">
  <section class="vk-card hero vk-rainbow-top">
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

    <button class="vk-btn refresh" onclick={() => load(true)} disabled={loading}>
      <Icon name="refresh" size={14} />
      {loading ? 'Aggiorno…' : 'Aggiorna'}
    </button>
  </section>

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
        <article class="vk-card room" class:racing={room.status.toLowerCase() === 'racing'}>
          <header class="room-head">
            <h3 class="room-name">{room.name}</h3>
            <span
              class="vk-badge {room.status.toLowerCase() === 'racing' ? 'vk-badge--success' : ''}"
            >
              {room.status}
            </span>
          </header>

          <p class="track">{room.track}</p>

          <footer class="room-foot">
            <span class="vk-faint">{room.region}</span>
            <span class="vk-faint">{room.mode}</span>
            <span class="vk-spacer"></span>
            <span class="players">{room.playerCount}/{room.maxPlayers}</span>
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
    gap: 40px;
    overflow: hidden;
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
    margin-left: auto;
  }

  .rooms {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 14px;
  }

  .room {
    display: flex;
    flex-direction: column;
    gap: 8px;
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
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .room-name {
    margin: 0;
    font-size: 15px;
    font-weight: 800;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track {
    margin: 0;
    font-size: var(--vk-fs-small);
    color: var(--vk-text-secondary);
  }

  .room-foot {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--vk-fs-micro);
  }

  .players {
    font-weight: 900;
    color: var(--vk-cyan-soft);
  }

  .skeleton {
    height: 64px;
  }

  @media (max-width: 900px) {
    .hero {
      flex-wrap: wrap;
      gap: 24px;
    }
  }
</style>
