<script lang="ts">
  /**
   * News.
   *
   * Le voci arrivano da `news.json` (comando `news_fetch`) e il testo è
   * markdown, reso da `Markdown.svelte`. Filtro per categoria, "in evidenza" e
   * ricerca testuale ricalcano `ApplyNewsFilter` del launcher WPF.
   *
   * Sulle card si vede l'eccezione, non il default: la categoria che hanno
   * quasi tutte non si ripete su ognuna, "in evidenza" si segna solo quando è
   * una minoranza, e la data compare solo se il server ne manda una (§D-104).
   */
  import { onMount } from 'svelte';

  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import Markdown from '$lib/components/Markdown.svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import { app, formatDate, formatRelative } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import type { NewsItem } from '$lib/api/types';

  /*
   * I due filtri nostri sono sentinelle, non etichette: cambiando lingua il
   * filtro attivo deve restare quello, non diventare una stringa che non
   * corrisponde più a niente.
   */
  const ALL = '\u0000tutte';
  const PINNED = '\u0000evidenza';

  function filterLabel(value: string): string {
    if (value === ALL) return t('news.all');
    if (value === PINNED) return t('news.pinned');
    return value;
  }

  let items = $state<NewsItem[]>([]);
  let loading = $state(true);
  let query = $state('');
  let filter = $state(ALL);
  /** Media che il server non ha servito: la card resta, senza il riquadro rotto. */
  let broken = $state<string[]>([]);

  const categories = $derived(
    items
      .map((item) => item.category)
      .filter((category, index, all) => {
        return category !== '' && all.indexOf(category) === index;
      })
  );

  /** La categoria di quasi tutte: su ogni card sarebbe solo ripetuta. */
  const usual = $derived.by(() => {
    let best = '';
    let most = 0;
    for (const category of categories) {
      const count = items.filter((item) => item.category === category).length;
      if (count > most) [best, most] = [category, count];
    }
    return most > items.length / 2 ? best : '';
  });

  /** "In evidenza" dice qualcosa solo se lo è una parte delle notizie. */
  const pinnedIsRare = $derived(items.filter((item) => item.isPinned).length <= items.length / 2);

  const filters = $derived([
    ALL,
    ...(pinnedIsRare && items.some((item) => item.isPinned) ? [PINNED] : []),
    ...(categories.length > 1 ? categories : [])
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
      app.toast(t('news.unavailable'), api.errorMessage(error), 'warning');
      items = [];
    } finally {
      loading = false;
    }
  }

  /**
   * La data di una notizia, se ne ha una. `news.json` porta un'etichetta
   * libera: una data vera diventa "3 giorni fa" con la data intera nel
   * suggerimento; un testo senza cifre ("Live", "Local") non dice niente a
   * chi legge e non si mostra.
   */
  function when(label: string): { text: string; full: string | undefined } | null {
    const trimmed = label.trim();
    const time = Date.parse(trimmed);
    if (/\d{4}/.test(trimmed) && !Number.isNaN(time)) {
      const iso = new Date(time).toISOString();
      return { text: formatRelative(iso), full: formatDate(iso) };
    }
    return /\d/.test(trimmed) ? { text: trimmed, full: undefined } : null;
  }

  function markBroken(path: string) {
    if (!broken.includes(path)) broken = [...broken, path];
  }
</script>

<div class="page">
  <div class="toolbar">
    <input class="vk-input search" bind:value={query} placeholder={t('news.search')} />

    {#if filters.length > 1}
      <div class="chips">
        {#each filters as item (item)}
          <button class="chip" class:active={filter === item} onclick={() => (filter = item)}>
            {filterLabel(item)}
          </button>
        {/each}
      </div>
    {/if}

    <button
      class="vk-btn icon-only"
      class:spinning={loading}
      onclick={load}
      disabled={loading}
      aria-label={t('common.refresh')}
      {@attach tooltip(t('common.refresh'))}
    >
      <Icon name="refresh" size={15} />
    </button>
  </div>

  {#if loading}
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
  {:else if filtered.length === 0}
    <div class="vk-card vk-empty">
      <Icon name="news" size={28} />
      <p>
        {items.length === 0 ? t('news.empty') : t('news.noMatch')}
      </p>
    </div>
  {:else}
    {#each filtered as item, index (index)}
      {@const date = when(item.dateLabel)}
      {@const pinned = item.isPinned && pinnedIsRare}
      <article class="vk-card news" class:pinned>
        <header class="news-head">
          <div class="labels">
            {#if item.category && item.category !== usual}
              <span class="vk-badge">{item.category}</span>
            {/if}
            {#if pinned}
              <span class="vk-badge vk-badge--warning">{t('news.pinned')}</span>
            {/if}
            {#if item.version}<span class="vk-faint version">{item.version}</span>{/if}
          </div>
          {#if date}
            <span class="vk-faint date" {@attach tooltip(date.full)}>{date.text}</span>
          {/if}
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

  .chip:hover {
    color: var(--vk-text);
  }

  .chip.active {
    border-color: transparent;
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    color: var(--vk-text);
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
