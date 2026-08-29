<script lang="ts">
  /**
   * Pannello del download in corso.
   *
   * Il legacy apriva `AddonDownloadDialog` durante lo scaricamento; qui la
   * forma è la stessa ma il contenuto arriva dai progressi che il backend
   * spinge già per ogni operazione: fase, percentuale, byte e velocità.
   *
   * Non si chiude cliccando fuori: l'unica uscita è annullare, perché chiudere
   * un pannello non ferma un download e lasciarlo credere sarebbe peggio che
   * non mostrarlo.
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';

  interface Props {
    open: boolean;
    /** Cosa si sta scaricando, mostrato in grande. */
    title: string;
    /** Riga sotto il titolo: il nome del file, di solito. */
    subtitle?: string;
  }

  const { open, title, subtitle = '' }: Props = $props();

  const percent = $derived(app.progress.percent);
  const indeterminate = $derived(percent === null);
  const width = $derived(Math.min(100, Math.max(0, percent ?? 0)));
  let cancelling = $state(false);

  async function cancel() {
    cancelling = true;
    try {
      await api.cancelOperation();
    } catch (error) {
      app.toast(t('download.cancelFailed'), api.errorMessage(error), 'warning');
    } finally {
      cancelling = false;
    }
  }
</script>

{#if open}
  <div class="overlay" role="dialog" aria-modal="true" aria-label={title}>
    <div class="panel">
      <div class="head">
        <span class="glyph"><Icon name="download" size={18} /></span>
        <div class="id">
          <p class="title">{title}</p>
          {#if subtitle}<p class="vk-faint subtitle">{subtitle}</p>{/if}
        </div>
        {#if !indeterminate}<span class="percent">{Math.round(width)}%</span>{/if}
      </div>

      <div class="vk-progress bar" class:vk-progress--indeterminate={indeterminate}>
        <div class="vk-progress__fill" style="width: {width}%"></div>
      </div>

      <div class="meta">
        <span class="phase">{app.progress.detail || app.progress.phase}</span>
        <span class="vk-spacer"></span>
        {#if app.progress.bytesLabel}
          <span class="vk-faint">{app.progress.bytesLabel}</span>
        {/if}
        {#if app.progress.speedLabel}
          <span class="speed">{app.progress.speedLabel}</span>
        {/if}
      </div>

      <div class="actions">
        <button class="vk-btn" onclick={cancel} disabled={cancelling}>
          {cancelling ? t('download.cancelling') : t('common.cancel')}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(4 7 15 / 0.62);
    backdrop-filter: blur(3px);
  }

  .panel {
    width: min(440px, 100%);
    padding: 20px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel);
    box-shadow: var(--vk-shadow-modal);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 14px;
  }

  .glyph {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    flex: none;
    border-radius: 50%;
    background: var(--vk-input);
    color: var(--vk-cyan-soft);
  }

  .id {
    min-width: 0;
    flex: 1;
  }

  .title {
    margin: 0;
    font-size: var(--vk-fs-card-title);
    font-weight: 900;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .subtitle {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .percent {
    font-size: 20px;
    font-weight: 900;
    font-variant-numeric: tabular-nums;
  }

  .bar {
    height: 10px;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .phase {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .speed {
    color: var(--vk-cyan-soft);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 16px;
  }
</style>
