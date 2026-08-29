<script lang="ts">
  /**
   * Aggiornamento del launcher, con l'installazione a vista.
   *
   * Sostituisce lo script PowerShell del launcher legacy, che apriva una
   * console nera perché non poteva disegnare nulla mentre si sovrascriveva da
   * solo. Qui il download e l'installazione avvengono **dentro** l'app: si
   * vedono la fase, i byte e la percentuale, e il riavvio parte da sé.
   *
   * L'updater di Tauri verifica una firma Ed25519 prima di installare
   * qualunque cosa: un pacchetto che non corrisponde viene rifiutato invece di
   * essere eseguito (vedi `docs/release.md` §5).
   */
  import { check, type Update } from '@tauri-apps/plugin-updater';
  import { relaunch } from '@tauri-apps/plugin-process';

  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import { formatBytes } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import type { LauncherUpdateStatus } from '$lib/api/types';

  interface Props {
    /** Ciò che `versions.json` dichiara: serve quando l'updater non risponde. */
    status: LauncherUpdateStatus;
    onclose: () => void;
  }

  const { status, onclose }: Props = $props();

  type Stage = 'checking' | 'ready' | 'downloading' | 'installing' | 'done' | 'unavailable';

  let stage = $state<Stage>('checking');
  let update = $state<Update | null>(null);
  let downloaded = $state(0);
  let total = $state(0);
  let error = $state('');

  const percent = $derived(total > 0 ? Math.min(100, (downloaded / total) * 100) : 0);
  const busy = $derived(stage === 'downloading' || stage === 'installing');

  $effect(() => {
    void lookForUpdate();
  });

  async function lookForUpdate() {
    stage = 'checking';
    error = '';
    try {
      const found = await check();
      if (found) {
        update = found;
        stage = 'ready';
      } else {
        stage = 'unavailable';
      }
    } catch (err) {
      // Manifest assente, firma non configurata, rete: la differenza non
      // cambia cosa può fare l'utente, ma il motivo va detto.
      error = err instanceof Error ? err.message : String(err);
      stage = 'unavailable';
    }
  }

  /**
   * Scarica e installa, poi riavvia.
   *
   * `downloadAndInstall` emette tre eventi: l'inizio con la dimensione totale,
   * ogni blocco ricevuto, e la fine. Sono quelli che riempiono la barra.
   */
  async function installNow() {
    if (!update) return;

    stage = 'downloading';
    downloaded = 0;
    total = 0;
    error = '';

    try {
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? 0;
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
        } else if (event.event === 'Finished') {
          stage = 'installing';
        }
      });

      stage = 'done';
      // Un istante perché si legga "installato" prima che la finestra sparisca.
      setTimeout(() => void relaunch(), 900);
    } catch (err) {
      error = explain(err instanceof Error ? err.message : String(err));
      stage = 'ready';
    }
  }

  /**
   * Su Linux l'updater di Tauri sa aggiornare solo l'AppImage: chi ha
   * installato il `.deb` o l'`.rpm` riceve un errore che parla di formati, non
   * di cosa deve fare (§D-069).
   */
  function explain(message: string): string {
    const packaged = /appimage|not supported|unsupported/i.test(message);
    if (!packaged) return message;

    return `${message}\n\n${t('updater.linuxNote')}`;
  }
</script>

<div class="overlay">
  <div
    class="sheet vk-rainbow-top"
    role="dialog"
    aria-modal="true"
    aria-label={t('home.launcherUpdate')}
  >
    <header>
      <p class="vk-eyebrow">{t('home.launcherUpdate')}</p>
      <h2 class="title">
        {#if stage === 'checking'}
          {t('updater.checking')}
        {:else if stage === 'downloading'}
          {t('updater.downloadingVersion', { version: update?.version ?? '' })}
        {:else if stage === 'installing'}
          {t('updater.installing')}
        {:else if stage === 'done'}
          {t('updater.done')}
        {:else if stage === 'unavailable'}
          {t('updater.available', { version: status.latest })}
        {:else}
          {t('updater.available', { version: update?.version ?? '' })}
        {/if}
      </h2>
    </header>

    {#if stage === 'checking'}
      <p class="vk-subtitle">{t('updater.checkingBody')}</p>
      <div class="vk-skeleton bar"></div>
    {:else if stage === 'unavailable'}
      <p class="vk-subtitle">
        <code>versions.json</code>
        {t('updater.unavailableBody', { latest: status.latest, current: status.current })}
      </p>
      {#if error}
        <p class="vk-error inline">{error}</p>
      {/if}
      <div class="actions">
        {#if status.downloadPage}
          <button
            class="vk-btn vk-btn--primary"
            onclick={() => api.openExternal(status.downloadPage)}
          >
            <Icon name="external" size={14} />
            {t('updater.openDownloadPage')}
          </button>
        {/if}
        <button class="vk-btn" onclick={onclose}>{t('common.close')}</button>
      </div>
    {:else if stage === 'done'}
      <p class="vk-subtitle">{t('updater.doneBody')}</p>
      <div class="progress"><div class="fill" style="width: 100%"></div></div>
    {:else if busy}
      <p class="vk-subtitle">
        {#if stage === 'installing'}
          {t('updater.installingBody')}
        {:else}
          {t('updater.downloadingBody')}
        {/if}
      </p>
      <div class="progress"><div class="fill" style="width: {percent}%"></div></div>
      <p class="vk-mono metrics">
        {#if total > 0}
          {formatBytes(downloaded)} / {formatBytes(total)} · {percent.toFixed(0)}%
        {:else}
          {t('updater.received', { bytes: formatBytes(downloaded) })}
        {/if}
      </p>
    {:else}
      <p class="vk-subtitle">
        {t('updater.readyBody', { latest: update?.version ?? '', current: status.current })}
      </p>
      {#if update?.body?.trim()}
        <p class="notes">{update.body}</p>
      {/if}
      {#if error}
        <p class="vk-error inline">{error}</p>
      {/if}
      <div class="actions">
        <button class="vk-btn vk-btn--primary" onclick={installNow}>
          <Icon name="download" size={14} />
          {t('updater.installAndRestart')}
        </button>
        <button class="vk-btn" onclick={onclose}>{t('notice.later')}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
    padding: 20px;
    background: rgb(4 7 14 / 0.78);
    backdrop-filter: blur(4px);
  }

  .sheet {
    width: min(520px, 100%);
    padding: 22px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel);
    box-shadow: var(--vk-shadow-modal);
  }

  .title {
    margin: 4px 0 10px;
    font-size: 20px;
    font-weight: 900;
  }

  .bar {
    height: 10px;
    margin-top: 14px;
    border-radius: 999px;
  }

  .progress {
    height: 10px;
    margin-top: 14px;
    border-radius: 999px;
    background: var(--vk-input);
    overflow: hidden;
  }

  /* Stessa barra delle altre: arcobaleno intero (§D-048). */
  .fill {
    height: 100%;
    background: var(--vk-progress-gradient);
    box-shadow:
      0 0 12px rgb(255 0 102 / 0.35),
      0 0 12px rgb(0 242 255 / 0.35);
    transition: width var(--vk-dur-fast) linear;
  }

  .metrics {
    margin: 8px 0 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .notes {
    margin: 10px 0 0;
    padding: 10px 12px;
    border-radius: var(--vk-radius-badge);
    background: var(--vk-input);
    font-size: var(--vk-fs-micro);
    white-space: pre-wrap;
  }

  .inline {
    margin-top: 10px;
    padding: 10px 12px;
    font-size: var(--vk-fs-micro);
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 16px;
    flex-wrap: wrap;
  }
</style>
