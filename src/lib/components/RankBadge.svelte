<script lang="ts">
  /**
   * Il rank di un giocatore, accanto al suo nome.
   *
   * Come sul sito: l'immagine del grado — o lo stemma che il server assegna
   * allo staff — subito dopo il nome. Quando l'immagine manca resta il numero
   * del grado in un cerchietto, lo stesso ripiego del sito: un giocatore di
   * grado 5 non deve sembrare senza grado solo perché il disegno non c'è
   * (§D-094).
   */
  import { t } from '$lib/stores/i18n.svelte';

  interface Props {
    /** Miniatura come data URI. */
    image?: string | null | undefined;
    /** Grado del gioco; 0 se non ne ha. */
    rank?: number;
    /** Nome di un rank speciale (staff); vuoto per i gradi del gioco. */
    label?: string | undefined;
    /** Lato in pixel CSS. */
    size?: number;
  }

  const { image = null, rank = 0, label = '', size = 22 }: Props = $props();

  const title = $derived(label || (rank > 0 ? t('board.rank', { rank }) : ''));
</script>

{#if image}
  <img
    class="rank-badge"
    src={image}
    alt={title}
    {title}
    draggable="false"
    style="--size: {size}px"
  />
{:else if rank > 0}
  <span class="rank-badge number" role="img" aria-label={title} {title} style="--size: {size}px">
    {rank}
  </span>
{/if}

<style>
  .rank-badge {
    flex: none;
    width: var(--size);
    height: var(--size);
    object-fit: contain;
    filter: drop-shadow(0 0 5px rgb(0 0 0 / 0.4));
  }

  .number {
    display: inline-grid;
    place-items: center;
    border: 1px solid rgb(255 255 255 / 0.18);
    border-radius: 50%;
    background: rgb(255 255 255 / 0.08);
    color: var(--vk-cyan-soft);
    font-size: calc(var(--size) * 0.5);
    font-weight: 900;
    line-height: 1;
    filter: none;
  }
</style>
