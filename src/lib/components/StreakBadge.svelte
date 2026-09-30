<script lang="ts">
  /**
   * La streak di un giocatore, come la mostra il sito: una fiamma arancione
   * con i giorni consecutivi di gioco, un ombrellone azzurro quando il
   * giocatore è in vacanza e la streak è congelata, un trattino quando non
   * c'è (§D-085).
   */
  import Icon from '$lib/components/Icon.svelte';
  import { formatNumber, t } from '$lib/stores/i18n.svelte';
  import { tooltip } from '$lib/attachments/tooltip';

  interface Props {
    days: number;
    vacation?: boolean;
    size?: 'sm' | 'md' | 'lg';
    /** Mostra anche la parola "giorni": serve dove il numero è da solo. */
    withLabel?: boolean;
  }

  const { days, vacation = false, size = 'sm', withLabel = false }: Props = $props();

  const alive = $derived(Number.isFinite(days) && days > 0);
  const title = $derived(
    vacation ? t('streak.vacation') : t('streak.tooltip', { days: formatNumber(days) })
  );
  const iconSize = $derived(size === 'lg' ? 18 : size === 'md' ? 15 : 13);
</script>

{#if alive}
  <span class="streak {size}" class:vacation {title} aria-label={title} role="img">
    <Icon name={vacation ? 'umbrella' : 'fire'} size={iconSize} />
    <span class="days">{formatNumber(days)}</span>
    {#if withLabel}
      <span class="unit">{days === 1 ? t('streak.day') : t('streak.days')}</span>
    {/if}
  </span>
{:else}
  <span class="streak none {size}" {@attach tooltip(t('streak.none'))}>{t('common.dash')}</span>
{/if}

<style>
  /* Gli stessi colori del sito: arancione per la fiamma, azzurro per la
     vacanza. */
  .streak {
    --streak-color: #ff8a3d;

    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-weight: 800;
    color: var(--streak-color);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }

  .streak :global(svg) {
    filter: drop-shadow(0 0 6px color-mix(in srgb, var(--streak-color) 55%, transparent));
  }

  .vacation {
    --streak-color: #3dc7ff;
  }

  .none {
    color: var(--vk-text-faint);
    font-weight: 400;
  }

  .sm {
    font-size: var(--vk-fs-small);
  }

  .md {
    font-size: 14px;
  }

  .lg {
    gap: 6px;
    font-size: 18px;
  }

  .unit {
    font-size: 0.75em;
    font-weight: 700;
    color: var(--vk-text-secondary);
  }
</style>
