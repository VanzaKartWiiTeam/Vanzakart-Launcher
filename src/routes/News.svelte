<script lang="ts">
  /**
   * News.
   *
   * Le voci arrivano da `news.json` (comando `news_fetch`) e il testo è
   * markdown, reso da `Markdown.svelte`. Filtro per categoria, "in evidenza" e
   * ricerca testuale ricalcano `ApplyNewsFilter` del launcher WPF.
   */
  import { onMount } from 'svelte';

  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import Markdown from '$lib/components/Markdown.svelte';
  import { app } from '$lib/stores/app.svelte';
  import type { NewsItem } from '$lib/api/types';

  const ALL = 'Tutte';
  const PINNED = 'In evidenza';

  let items = $state<NewsItem[]>([]);
  let loading = $state(true);
  let query = $state('');
  let filter = $state(ALL);
  /** Media che il server non ha servito: la card resta, senza il riquadro rotto. */
  let broken = $state<string[]>([]);

  const filters = $derived([
    ALL,
    ...(items.some((item) => item.isPinned) ? [PINNED] : []),
    ...Array.from(new Set(items.map((item) => item.category).filter(Boolean)))
  ]);

  const filtered = $derived(
    items.filter((item) => {
      const matchesFilter =
        filter === ALL || (filter === PINNED ? item.isPinned : item.category === filter);
      if (!matchesFilter) return false;

      const needle = query.trim().toLowerCase();
      if (needle === '') return true;

      return [item.title, item.summary, item.version, item.category].some((field) =>
        field.toLowerCase().includes(needle)
      );
    })
  );

  onMount(load);

  async function load() {
    loading = true;
    broken = [];
    try {
      items = await api.fetchNews();
      if (!filters.includes(filter)) filter = ALL;
    } catch (error) {
      app.toast('News non disponibili', api.errorMessage(error), 'warning');
      items = [];
    } finally {
      loading = false;
    }
  }

  function markBroken(path: string) {
    if (!broken.includes(path)) broken = [...broken, path];
  }
</script>

<div class="page">
  <div class="toolbar">
    <input class="vk-input search" bind:value={query} placeholder="Cerca nelle news…" />

    <div class="chips">
      {#each filters as item (item)}
        <button class="chip" class:active={filter === item} onclick={() => (filter = item)}>
          {item}
        </button>
      {/each}
    </div>

    <button class="vk-btn" onclick={load} disabled={loading}>
      <Icon name="refresh" size={14} />
      {loading ? 'Aggiorno…' : 'Aggiorna'}
    </button>
  </div>

  {#if loading}
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
  {:else if filtered.length === 0}
    <div class="vk-card vk-empty">
      <Icon name="news" size={28} />
      <p>
        {items.length === 0
          ? 'Nessuna notizia disponibile.'
          : 'Nessuna notizia corrisponde ai filtri.'}
      </p>
    </div>
  {:else}
    {#each filtered as item, index (index)}
      <article class="vk-card news" class:pinned={item.isPinned}>
        <header class="news-head">
          <div class="labels">
            {#if item.category}<span class="vk-badge">{item.category}</span>{/if}
            {#if item.isPinned}<span class="vk-badge vk-badge--warning">In evidenza</span>{/if}
            {#if item.version}<span class="vk-faint version">{item.version}</span>{/if}
          </div>
          {#if item.dateLabel}<span class="vk-faint date">{item.dateLabel}</span>{/if}
        </header>

        {#if item.title}<h2 class="news-title">{item.title}</h2>{/if}

        {#if item.mediaPath && !broken.includes(item.mediaPath)}
          {#if item.mediaKind === 'image'}
            <img
              class="media"
              src={item.mediaPath}
              alt=""
              loading="lazy"
              onerror={() => markBroken(item.mediaPath!)}
            />
          {:else if item.mediaKind === 'video'}
            <!-- Muto e in loop come il `MediaElement` del launcher WPF: la
                 clip parte da sola, i comandi restano per chi vuole l'audio. -->
            <video
              class="media"
              src={item.mediaPath}
              controls
              autoplay
              loop
              muted
              playsinline
              preload="metadata"
              onerror={() => markBroken(item.mediaPath!)}
            ></video>
          {/if}
        {/if}

        {#if item.summary}
          <Markdown source={item.summary} />
        {/if}
      </article>
    {/each}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 900px;
    margin: 0 auto;
    padding-bottom: 12px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .search {
    flex: 1;
    min-width: 220px;
  }

  .chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .chip {
    padding: 6px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-pill);
    background: transparent;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 700;
  }

  .chip.active {
    background: var(--vk-tab-active);
    border-color: #3a4c74;
    color: var(--vk-text);
  }

  .news {
    position: relative;
    overflow: hidden;
  }

  .news.pinned::before {
    content: '';
    position: absolute;
    inset: 0 0 auto;
    height: 2px;
    background: var(--vk-rainbow);
  }

  .news-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }

  .labels {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .version,
  .date {
    font-size: var(--vk-fs-micro);
  }

  .news-title {
    margin: 0 0 12px;
    font-size: 20px;
    font-weight: 900;
  }

  /*
   * Il media entra intero nel suo riquadro: `cover` su una colonna larga 900
   * tagliava sopra e sotto ogni clip 16:9. Con `auto` più i due tetti
   * l'immagine conserva le sue proporzioni e non viene mai ingrandita oltre
   * la dimensione naturale.
   */
  .media {
    display: block;
    width: auto;
    height: auto;
    max-width: 100%;
    max-height: 360px;
    margin: 0 auto 14px;
    border-radius: var(--vk-radius-input);
    object-fit: contain;
    background: var(--vk-input);
  }

  .skeleton {
    height: 96px;
  }
</style>
