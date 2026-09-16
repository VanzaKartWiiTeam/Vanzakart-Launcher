<script lang="ts">
  /**
   * Aggiornamento del launcher, con l'installazione a vista.
   *
   * Sostituisce lo script PowerShell del launcher legacy, che apriva una
   * console nera perché non poteva disegnare nulla mentre si sovrascriveva da
   * solo. Qui il download e l'installazione avvengono **dentro** l'app: si
   * vedono la fase, i byte e la percentuale, e il riavvio parte da sé.
   *
   * Il pacchetto è lo stesso che scarica l'installer, e viene srotolato nella
   * cartella in cui il launcher è già installato: niente seconda copia in
   * `%LOCALAPPDATA%`, niente secondo disinstallatore, niente scorciatoie
   * nuove. Impronta e firma si verificano prima che un solo file venga
   * toccato (vedi `docs/decisions.md` §D-084).
   */
  import { relaunch } from '@tauri-apps/plugin-process';

  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import type { LauncherUpdateOffer, LauncherUpdateStatus } from '$lib/api/types';

  interface Props {
    /** Ciò che `versions.json` dichiara: si vede finché il manifest arriva. */
    status: LauncherUpdateStatus;
    onclose: () => void;
  }

  const { status, onclose }: Props = $props();

  type Stage = 'checking' | 'ready' | 'installing' | 'upToDate' | 'blocked' | 'done';

  let stage = $state<Stage>('checking');
  let offer = $state<LauncherUpdateOffer | null>(null);
  let error = $state('');

  /** I progressi dell'operazione "launcher", e solo quelli. */
  const progress = $derived(app.progress.operation === 'launcher' ? app.progress : null);
  const percent = $derived(
    progress?.percent ??
      (progress && progress.bytesTotal > 0 ? (progress.bytesDone / progress.bytesTotal) * 100 : 0)
  );
  const replacing = $derived(progress?.phase === 'Installing');

  $effect(() => {
    void lookForUpdate();
  });

  async function lookForUpdate() {
    stage = 'checking';
    error = '';
    try {
      const found = await api.checkLauncherUpdate();
      offer = found;

      if (found.blocked) stage = 'blocked';
      else if (found.canInstall) stage = 'ready';
      else stage = 'upToDate';
    } catch (err) {
      // Manifest assente, rete, pacchetto mancante per questa piattaforma:
      // la differenza non cambia cosa può fare l'utente, ma il motivo va detto.
      error = api.errorMessage(err);
      stage = 'blocked';
    }
  }

  /**
   * Scarica, sostituisce, riavvia.
   *
   * L'unico passo che l'utente non può annullare è il terzo, e dura una
   * frazione di secondo: tutto il resto — download, impronta, firma — avviene
   * con l'installazione ancora intatta.
   */
  async function installNow() {
    if (!offer?.canInstall) return;

    stage = 'installing';
    error = '';

    try {
      await api.installLauncherUpdate();
      stage = 'done';
      // Un istante perché si legga "installato" prima che la finestra sparisca.
      setTimeout(() => void relaunch(), 900);
    } catch (err) {
      error = api.errorMessage(err);
      stage = 'ready';
    }
  }

  const latest = $derived(offer?.latest || status.latest);
  const current = $derived(offer?.current || status.current);
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
        {:else if stage === 'installing'}
          {replacing
            ? t('updater.installing')
            : t('updater.downloadingVersion', { version: latest })}
        {:else if stage === 'done'}
          {t('updater.done')}
        {:else if stage === 'upToDate'}
          {t('updater.upToDate')}
        {:else if stage === 'blocked'}
          {t('updater.blockedTitle')}
        {:else}
          {t('updater.available', { version: latest })}
        {/if}
      </h2>
    </header>

    {#if stage === 'checking'}
      <p class="vk-subtitle">{t('updater.checkingBody')}</p>
      <div class="vk-skeleton bar"></div>
    {:else if stage === 'upToDate'}
      <p class="vk-subtitle">{t('updater.upToDateBody', { current })}</p>
      <div class="actions">
        <button class="vk-btn" onclick={lookForUpdate}>{t('updater.retry')}</button>
        <button class="vk-btn vk-btn--primary" onclick={onclose}>{t('common.close')}</button>
      </div>
    {:else if stage === 'blocked'}
      <p class="vk-subtitle">{t('updater.blockedBody')}</p>
      {#if offer?.blocked || error}
        <p class="vk-error inline">{offer?.blocked || error}</p>
      {/if}
      <div class="actions">
        {#if offer?.downloadPage || status.downloadPage}
          <button
            class="vk-btn vk-btn--primary"
            onclick={() => api.openExternal(offer?.downloadPage || status.downloadPage)}
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
    {:else if stage === 'installing'}
      <p class="vk-subtitle">
        {replacing ? t('updater.installingBody') : t('updater.downloadingBody')}
      </p>
      <div class="progress"><div class="fill" style="width: {percent}%"></div></div>
      <p class="vk-mono metrics">
        {#if progress?.bytesLabel}
          {progress.bytesLabel} · {percent.toFixed(0)}%{progress.speedLabel
            ? ` · ${progress.speedLabel}`
            : ''}
        {:else}
          {t('updater.received', { bytes: progress?.bytesLabel ?? '—' })}
        {/if}
      </p>
    {:else}
      <p class="vk-subtitle">{t('updater.readyBody', { latest, current })}</p>

      {#if offer}
        <dl class="facts">
          <div>
            <dt>{t('updater.installsInto')}</dt>
            <dd class="vk-mono path" title={offer.installDir}>{offer.installDir}</dd>
          </div>
          {#if offer.sizeBytes > 0}
            <div>
              <dt>{t('updater.packageSize')}</dt>
              <dd class="vk-mono">{offer.sizeLabel}</dd>
            </div>
          {/if}
        </dl>

        <p class="assurance" class:warning={!offer.signed}>
          <Icon name={offer.signed ? 'check' : 'warning'} size={14} />
          {offer.signed ? t('updater.checkedPackage') : t('updater.unsignedPackage')}
        </p>
      {/if}

      {#if offer?.notes?.trim()}
        <p class="notes">{offer.notes}</p>
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

  .facts {
    display: grid;
    gap: 8px;
    margin: 14px 0 0;
    padding: 10px 12px;
    border-radius: var(--vk-radius-badge);
    background: var(--vk-input);
    font-size: var(--vk-fs-micro);
  }

  .facts div {
    display: flex;
    gap: 10px;
    align-items: baseline;
    justify-content: space-between;
  }

  .facts dt {
    color: var(--vk-text-secondary);
    white-space: nowrap;
  }

  .facts dd {
    margin: 0;
    min-width: 0;
    text-align: right;
  }

  /* Il percorso è la cosa che l'utente deve poter leggere: se non ci sta, si
     taglia davanti, dove i pezzi contano meno. */
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
  }

  .assurance {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 10px 0 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .assurance.warning {
    color: var(--vk-warning);
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
