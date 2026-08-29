<script lang="ts">
  /**
   * Sfoglia GameBanana e installa una mod come addon.
   *
   * Porta `GameBananaFilePickerDialog` + `AddonDownloadDialog` del legacy, ma
   * senza dialoghi annidati: la mod si espande in linea e mostra i suoi file.
   *
   * Nessun URL di download passa da qui. Per installare si mandano al backend
   * solo gli identificativi di mod e file; l'indirizzo lo rilegge e lo valida
   * lui (vedi `docs/decisions.md` §D-030).
   */
  import * as api from '$lib/api';
  import DownloadOverlay from '$lib/components/DownloadOverlay.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import { app, formatBytes } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import type { GameBananaFile, GameBananaMod } from '$lib/api/types';

  interface Props {
    /** Chiamata dopo un'installazione riuscita, per ricaricare gli addon. */
    oninstalled: () => void;
  }

  const { oninstalled }: Props = $props();

  const SORTS = $derived([
    { value: 'Generic_Newest', label: t('gb.sort.newest') },
    { value: 'Generic_MostLiked', label: t('gb.sort.liked') },
    { value: 'Generic_MostViewed', label: t('gb.sort.viewed') },
    { value: 'Generic_MostDownloaded', label: t('gb.sort.downloaded') },
    { value: 'Generic_Alphabetically', label: t('gb.sort.alphabetical') }
  ]);

  let query = $state('');
  let sort = $state('Generic_Newest');
  let page = $state(1);
  let loading = $state(false);
  let error = $state('');
  let mods = $state<GameBananaMod[]>([]);
  let total = $state(0);
  let hasMore = $state(false);
  let truncated = $state(false);
  let expanded = $state<number | null>(null);
  let installing = $state('');
  /** Cosa si sta scaricando, per il pannello del download. */
  let downloading = $state<{ mod: string; file: string } | null>(null);
  /** Anteprime che il server non ha servito: al loro posto la sagoma. */
  let broken = $state<string[]>([]);

  // La scheda si apre gia' sui risultati: aprirla per poi premere "Sfoglia"
  // era un passaggio in piu' che non decideva niente.
  $effect(() => {
    if (mods.length === 0 && !loading && !error) void run(1);
  });

  async function run(target: number) {
    loading = true;
    error = '';
    try {
      const result = await api.searchGameBanana(query.trim(), sort, target);
      mods = result.mods;
      total = result.totalAvailable;
      hasMore = result.hasMore;
      truncated = result.catalogTruncated;
      page = target;
      expanded = null;
    } catch (err) {
      error = api.errorMessage(err);
      mods = [];
    } finally {
      loading = false;
    }
  }

  function onSearchKey(event: KeyboardEvent) {
    if (event.key === 'Enter') void run(1);
  }

  /**
   * Un file solo si installa subito; più file si scelgono.
   *
   * È il passaggio che il legacy faceva aprire un dialogo anche quando non
   * c'era niente da decidere.
   */
  function primary(item: GameBananaMod) {
    if (item.files.length === 1) {
      void install(item, item.files[0]!);
      return;
    }
    expanded = expanded === item.id ? null : item.id;
  }

  async function install(item: GameBananaMod, file: GameBananaFile) {
    installing = `${item.id}-${file.fileId}`;
    downloading = { mod: item.name, file: file.fileName };
    app.resetProgress();
    try {
      const addon = await api.installGameBananaFile(item.id, file.fileId);
      app.toast(
        t('gb.installed'),
        t('gb.installedBody', { name: addon.name, count: addon.fileCount }),
        'success'
      );
      oninstalled();
    } catch (err) {
      const message = api.errorMessage(err);
      app.toast(
        api.errorCode(err) === 'cancelled' ? t('gb.cancelled') : t('gb.downloadFailed'),
        message,
        'warning'
      );
    } finally {
      installing = '';
      downloading = null;
      app.resetProgress();
    }
  }

  function busy(item: GameBananaMod): boolean {
    return installing.startsWith(`${item.id}-`);
  }
</script>

<section class="vk-card">
  <div class="controls">
    <input
      class="vk-input search"
      placeholder={t('gb.search')}
      bind:value={query}
      onkeydown={onSearchKey}
      disabled={loading}
    />
    <select class="vk-input sort" bind:value={sort} onchange={() => run(1)} disabled={loading}>
      {#each SORTS as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
    <button class="vk-btn" onclick={() => run(1)} disabled={loading}>
      <Icon name="refresh" size={14} />
      {t('gb.searchAction')}
    </button>
  </div>

  {#if truncated}
    <p class="vk-faint note">{t('gb.truncated')}</p>
  {/if}

  {#if error}
    <p class="vk-error inline">{error}</p>
  {:else if loading}
    <div class="vk-skeleton skeleton"></div>
  {:else if mods.length === 0}
    <p class="vk-faint note">
      {query.trim() ? t('gb.noMatch') : t('gb.noResults')}
    </p>
  {:else}
    <p class="vk-faint note">{t('gb.count', { count: total, page })}</p>

    <ul class="mods">
      {#each mods as item (item.id)}
        <li class="mod" class:open={expanded === item.id}>
          <div class="mod-head">
            {#if item.previewUrl && !broken.includes(item.previewUrl)}
              <img
                class="thumb"
                src={item.previewUrl}
                alt=""
                loading="lazy"
                onerror={() => (broken = [...broken, item.previewUrl])}
              />
            {:else}
              <span class="thumb empty"><Icon name="package" size={20} /></span>
            {/if}

            <div class="mod-id">
              <p class="mod-name">{item.name}</p>
              <p class="vk-faint mod-meta">
                {t('gb.modMeta', {
                  author: item.author || t('gb.unknownAuthor'),
                  likes: item.likes,
                  files: item.files.length
                })}
              </p>
              {#if item.description}
                <p class="vk-faint mod-desc">{item.description.slice(0, 190)}</p>
              {/if}
            </div>

            <div class="mod-actions">
              <button
                class="vk-btn vk-btn--primary act"
                onclick={() => primary(item)}
                disabled={installing !== ''}
              >
                <Icon name="download" size={14} />
                {busy(item) ? t('gb.downloading') : t('gb.install')}
                {#if item.files.length > 1}
                  <span class="caret" class:up={expanded === item.id}>
                    <Icon name="chevron" size={13} />
                  </span>
                {/if}
              </button>

              {#if item.profileUrl}
                <button
                  class="vk-btn act"
                  title={t('mods.openOnGameBanana')}
                  aria-label={t('mods.openOnGameBanana')}
                  onclick={() => api.openExternal(item.profileUrl)}
                >
                  <Icon name="external" size={14} />
                </button>
              {/if}
            </div>
          </div>

          {#if expanded === item.id}
            <ul class="files">
              <li class="files-hint vk-faint">{t('gb.multipleFiles')}</li>
              {#each item.files as file (file.fileId)}
                <li class="file">
                  <div class="file-id">
                    <p class="file-name vk-mono">{file.fileName}</p>
                    {#if file.description}
                      <p class="vk-faint file-desc">{file.description.slice(0, 160)}</p>
                    {/if}
                  </div>
                  <span class="vk-faint file-size">{formatBytes(file.sizeBytes)}</span>
                  <button
                    class="vk-btn vk-btn--primary act"
                    onclick={() => install(item, file)}
                    disabled={installing !== ''}
                  >
                    <Icon name="download" size={14} />
                    {installing === `${item.id}-${file.fileId}`
                      ? t('gb.downloading')
                      : t('gb.install')}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>

    <div class="pager">
      <button class="vk-btn" onclick={() => run(page - 1)} disabled={loading || page <= 1}>
        {t('gb.previous')}
      </button>
      <button class="vk-btn" onclick={() => run(page + 1)} disabled={loading || !hasMore}>
        {t('gb.next')}
      </button>
    </div>
  {/if}
</section>

<DownloadOverlay
  open={downloading !== null}
  title={downloading?.mod ?? ''}
  subtitle={downloading?.file ?? ''}
/>

<style>
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin: 0 0 12px;
  }

  .search {
    flex: 1;
    min-width: 200px;
  }

  .sort {
    width: auto;
    min-width: 160px;
  }

  .note {
    margin: 0 0 12px;
    font-size: var(--vk-fs-micro);
  }

  .inline {
    padding: 10px 12px;
    margin: 0 0 12px;
    font-size: var(--vk-fs-micro);
  }

  .skeleton {
    height: 140px;
  }

  .mods,
  .files {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .mod {
    padding: 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-input);
    background: var(--vk-panel-soft);
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .mod:hover,
  .mod.open {
    border-color: #3a4c74;
  }

  .mod-head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  /* 16:9, la proporzione degli screenshot di GameBanana. */
  .thumb {
    flex: none;
    width: 112px;
    height: 63px;
    border-radius: var(--vk-radius-badge);
    object-fit: cover;
    background: var(--vk-input);
  }

  .thumb.empty {
    display: grid;
    place-items: center;
    border: 1px solid var(--vk-stroke);
    color: var(--vk-text-faint);
  }

  .mod-id {
    min-width: 0;
    flex: 1;
  }

  .mod-name {
    margin: 0;
    font-weight: 800;
    overflow-wrap: anywhere;
  }

  .mod-meta {
    margin: 2px 0 0;
    font-size: var(--vk-fs-eyebrow);
  }

  .mod-desc {
    margin: 6px 0 0;
    font-size: var(--vk-fs-micro);
    line-height: 1.45;
    /* Due righe: la descrizione orienta, non si legge qui. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .mod-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .caret {
    display: inline-flex;
    transition: transform var(--vk-dur-fast) var(--vk-ease);
  }

  .caret.up {
    transform: rotate(180deg);
  }

  .files {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--vk-stroke);
  }

  .files-hint {
    font-size: var(--vk-fs-eyebrow);
  }

  .file {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
  }

  .file-id {
    min-width: 0;
    flex: 1;
  }

  .file-name {
    margin: 0;
    font-size: var(--vk-fs-micro);
    overflow-wrap: anywhere;
  }

  .file-desc {
    margin: 2px 0 0;
    font-size: var(--vk-fs-eyebrow);
  }

  .file-size {
    font-size: var(--vk-fs-micro);
    white-space: nowrap;
  }

  .act {
    padding: 8px 12px;
    font-size: var(--vk-fs-micro);
    gap: 6px;
  }

  .pager {
    display: flex;
    justify-content: center;
    gap: 10px;
    margin-top: 14px;
  }

  @media (max-width: 720px) {
    .thumb,
    .file-size,
    .mod-desc {
      display: none;
    }
  }
</style>
