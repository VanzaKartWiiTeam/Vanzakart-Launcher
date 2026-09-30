<script lang="ts">
  /**
   * Avanzamento di un'operazione lunga, e poi il suo esito.
   *
   * Lo stesso blocco per la modpack, il music pack e gli addon: fase in
   * parole, barra, file, byte, velocità e tempo che manca, e un "Annulla" che
   * compare solo finché annullare ferma davvero qualcosa. Finita
   * l'operazione resta una riga con com'è andata, con "Riprova" quando è
   * andata male (§D-086).
   *
   * Legge tutto dallo store delle operazioni: la pagina può essere stata
   * chiusa e riaperta a metà download, e la barra è ancora lì.
   */
  import Icon from '$lib/components/Icon.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import {
    isCancellable,
    operations,
    phaseLabel,
    type OperationKind
  } from '$lib/stores/operations.svelte';
  import { formatRemaining } from '$lib/stores/transfer';
  import * as api from '$lib/api';
  import { tooltip } from '$lib/attachments/tooltip';

  interface Props {
    kind: OperationKind;
    /** Riga compatta, per una card stretta. */
    compact?: boolean;
    /** Se c'è, l'esito negativo offre "Riprova". */
    onretry?: () => void;
  }

  const { kind, compact = false, onretry }: Props = $props();

  const running = $derived(operations.busyWith(kind));
  const progress = $derived(operations.progressOf(kind));
  const outcome = $derived(operations.outcomeOf(kind));
  const percent = $derived(progress?.percent ?? null);
  const width = $derived(Math.min(100, Math.max(0, percent ?? 0)));
  const remaining = $derived(formatRemaining(running ? operations.remaining : null));
  let cancelling = $state(false);

  async function cancel() {
    cancelling = true;
    try {
      await operations.cancel();
    } catch (error) {
      app.toast(t('download.cancelFailed'), api.errorMessage(error), 'warning');
    } finally {
      cancelling = false;
    }
  }
</script>

{#if running}
  <div class="transfer" class:compact aria-live="polite">
    <div class="head">
      <span class="dot" aria-hidden="true"></span>
      <strong class="phase">{progress ? phaseLabel(progress.phase) : t('ops.starting')}</strong>
      {#if percent !== null}
        <span class="percent">{Math.round(width)}%</span>
      {/if}
      <span class="vk-spacer"></span>
      {#if isCancellable(progress)}
        <button class="vk-btn vk-btn--ghost small" onclick={cancel} disabled={cancelling}>
          {cancelling ? t('download.cancelling') : t('common.cancel')}
        </button>
      {/if}
    </div>

    <div class="vk-progress bar" class:vk-progress--indeterminate={percent === null}>
      <div class="vk-progress__fill" style="width: {width}%"></div>
    </div>

    <div class="meta">
      {#if progress?.detail && !compact}
        <span class="detail">{progress.detail}</span>
      {/if}
      <span class="vk-spacer"></span>
      {#if progress && progress.filesTotal > 0}
        <span class="vk-faint">
          {t('mods.files', { done: progress.filesDone, total: progress.filesTotal })}
        </span>
      {/if}
      {#if progress?.bytesLabel}
        <span class="vk-faint">{progress.bytesLabel}</span>
      {/if}
      {#if progress?.speedLabel}
        <span class="speed">{progress.speedLabel}</span>
      {/if}
      {#if remaining}
        <span class="eta">{t('ops.remaining', { time: remaining })}</span>
      {/if}
    </div>
  </div>
{:else if outcome}
  <div
    class="outcome"
    data-tone={outcome.ok ? 'ok' : outcome.cancelled ? 'info' : 'warn'}
    role="status"
  >
    <Icon name={outcome.ok ? 'check' : 'warning'} size={14} />
    <span class="text">
      {outcome.ok
        ? outcome.message || t('ops.done')
        : outcome.cancelled
          ? t('ops.cancelled')
          : outcome.message}
    </span>
    {#if !outcome.ok && onretry}
      <button class="vk-btn small" onclick={onretry} disabled={operations.busy}>
        <Icon name="refresh" size={13} />
        {t('common.retry')}
      </button>
    {/if}
    <button
      class="vk-btn vk-btn--ghost small dismiss"
      aria-label={t('common.hide')}
      {@attach tooltip(t('common.hide'))}
      onclick={() => operations.clearOutcome(kind)}
    >
      <Icon name="close" size={12} />
    </button>
  </div>
{/if}

<style>
  .transfer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 16px;
    padding: 12px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
  }

  .transfer.compact {
    margin-top: 12px;
    padding: 10px 12px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--vk-fs-small);
  }

  .phase {
    font-weight: 800;
  }

  .percent {
    font-weight: 900;
    font-variant-numeric: tabular-nums;
    color: var(--vk-cyan-soft);
  }

  .bar {
    width: 100%;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .detail {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60%;
  }

  /* La velocità e il tempo che manca sono le cifre che si guardano mentre si
     aspetta: si staccano. */
  .speed,
  .eta {
    color: var(--vk-cyan-soft);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .small {
    padding: 4px 10px;
    font-size: var(--vk-fs-micro);
  }

  /* Pulsazione: l'operazione è viva anche quando non c'è una percentuale. */
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    background: var(--vk-rainbow-conic);
    animation: pulse 1.1s var(--vk-ease) infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.35;
      transform: scale(0.8);
    }
    50% {
      opacity: 1;
      transform: scale(1);
    }
  }

  .outcome {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
    padding: 9px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
    font-size: var(--vk-fs-small);
  }

  .outcome[data-tone='ok'] {
    border-color: color-mix(in srgb, var(--vk-success) 45%, var(--vk-stroke));
    color: var(--vk-success);
  }

  .outcome[data-tone='warn'] {
    border-color: color-mix(in srgb, var(--vk-warning) 45%, var(--vk-stroke));
    color: var(--vk-warning);
  }

  .outcome .text {
    flex: 1;
    min-width: 0;
  }

  .dismiss {
    color: inherit;
    padding: 4px 6px;
  }

  @media (prefers-reduced-motion: reduce) {
    .dot {
      animation: none;
    }
  }
</style>
