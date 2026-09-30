<script lang="ts">
  /**
   * Rooms.
   *
   * Ricalca il `RoomsView` del WPF: l'elenco delle stanze con skeleton,
   * stato vuoto e stato di errore distinti. Le quattro statistiche globali in
   * cima sono diventate una frase — «4 giocatori in 1 stanza» — col pallino
   * dello stato del server (§D-103).
   *
   * Una cosa che il WPF non mostrava e che qui serve: **chi c'è dentro** una
   * stanza. Il server manda l'elenco insieme alla stanza, prima veniva buttato.
   */
  import { onDestroy } from 'svelte';

  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import MiiAvatar from '$lib/components/MiiAvatar.svelte';
  import RankBadge from '$lib/components/RankBadge.svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import { formatRelative } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
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

  /** «Online» è il caso normale; qualunque altra cosa va segnalata. */
  const online = $derived(summary !== null && /^online$/i.test(summary.status.trim() || 'online'));

  const tone = $derived(
    loading ? 'idle' : error ? 'danger' : online ? 'success' : summary ? 'warning' : 'idle'
  );

  /** Lo stato in una frase: chi c'è, in quante stanze, e le private solo se ci sono. */
  const headline = $derived.by(() => {
    if (loading) return t('rooms.checking');
    if (error || !summary) return t('rooms.offline');
    if (summary.rooms.length === 0) return online ? t('rooms.serverOnline') : summary.status;
    const players =
      summary.totalPlayers === 1
        ? t('rooms.playersOne')
        : t('rooms.playersMany', { count: summary.totalPlayers });
    const rooms =
      summary.totalRooms === 1
        ? t('rooms.inRoomsOne')
        : t('rooms.inRoomsMany', { count: summary.totalRooms });
    const hidden =
      summary.privateRooms === 0
        ? ''
        : summary.privateRooms === 1
          ? ` · ${t('rooms.privateOne')}`
          : ` · ${t('rooms.privateMany', { count: summary.privateRooms })}`;
    return `${players} ${rooms}${hidden}`;
  });

  /** Quanto è vecchio l'elenco: se il server smette di scriverlo, lo si vede qui (§D-057). */
  const updated = $derived(
    summary?.lastUpdated
      ? t('rooms.updated', { time: formatRelative(summary.lastUpdated) })
      : undefined
  );

  /** La pista, più modalità e regione solo quando non sono quelle di sempre. */
  function detail(room: RoomView): string {
    return [
      room.track,
      room.mode !== 'Versus' ? room.mode : '',
      room.region !== 'Worldwide' ? room.region : ''
    ]
      .filter(Boolean)
      .join(' · ');
  }

  function toggle(room: RoomView) {
    expanded = expanded.includes(room.id)
      ? expanded.filter((id) => id !== room.id)
      : [...expanded, room.id];
  }
</script>

<div class="page">
  <div class="bar">
    <p class="status" data-tone={tone} {@attach tooltip(updated)}>
      <span class="status-dot" aria-hidden="true"></span>
      {headline}
    </p>
    <button
      class="vk-btn icon-only"
      class:spinning={refreshing}
      onclick={() => load(false)}
      disabled={refreshing}
      aria-label={t('common.refresh')}
      {@attach tooltip(t('rooms.refreshHint'))}
    >
      <Icon name="refresh" size={15} />
    </button>
  </div>

  {#if summary?.notice}
    <p class="vk-faint notice">{summary.notice}</p>
  {/if}

  {#if loading}
    {#each [0, 1, 2] as index (index)}
      <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
    {/each}
  {:else if error}
    <div class="vk-error">
      <strong>{t('rooms.loadFailed')}</strong>
      <p>{error}</p>
    </div>
  {:else if !summary || summary.rooms.length === 0}
    <div class="vk-card vk-empty">
      <Icon name="rooms" size={28} />
      <p>{t('rooms.empty')}</p>
      <p class="vk-faint">{t('rooms.emptyHint')}</p>
    </div>
  {:else}
    <div class="rooms">
      {#each summary.rooms as room (room.id)}
        {@const open = expanded.includes(room.id)}
        <article class="vk-card room" class:racing={room.status.toLowerCase() === 'racing'}>
          <header class="room-head">
            <h3
              class="room-name"
              {@attach tooltip(room.host ? t('rooms.host', { name: room.host }) : undefined)}
            >
              {room.name}
            </h3>
            <span
              class="vk-badge {room.status.toLowerCase() === 'racing' ? 'vk-badge--success' : ''}"
            >
              {room.status}
            </span>
          </header>

          <p class="track" {@attach tooltip(room.track)}>{detail(room)}</p>

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
                {open ? t('common.hide') : t('rooms.players')}
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
                        <RankBadge
                          image={player.rankImage}
                          rank={player.prestigeRank}
                          label={player.rankLabel}
                          staff={player.staff}
                          size={18}
                        />
                        {#if player.isHost}
                          <span class="vk-badge host-badge">{t('rooms.hostBadge')}</span>
                        {/if}
                      </span>
                      {#if player.friendCode}
                        <span class="vk-mono vk-faint fc">{player.friendCode}</span>
                      {/if}
                    </div>
                    <span class="vk-faint rating">{t('rooms.vr', { points: player.vr })}</span>
                  </li>
                {/each}
              </ul>
            {/if}
          {/if}
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

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-size: var(--vk-fs-body);
    font-weight: 700;
    --tone: var(--vk-text-faint);
  }

  .status[data-tone='success'] {
    --tone: var(--vk-success);
  }
  .status[data-tone='warning'] {
    --tone: var(--vk-warning);
  }
  .status[data-tone='danger'] {
    --tone: var(--vk-danger);
  }

  .status-dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--tone);
    box-shadow: 0 0 10px var(--tone);
  }

  .icon-only {
    padding: 9px 10px;
  }

  .spinning :global(.vk-icon) {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spinning :global(.vk-icon) {
      animation: none;
    }
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
    align-items: center;
    gap: 6px;
  }

  .room-name {
    flex: 1;
    min-width: 0;
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

  .skeleton {
    height: 64px;
  }
</style>
