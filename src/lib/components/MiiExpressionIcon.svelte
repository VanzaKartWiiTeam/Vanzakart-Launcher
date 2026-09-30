<script lang="ts">
  /**
   * Faccina di un'espressione del Mii, per la fila sotto l'anteprima.
   *
   * Disegnate a tratto, come emoji essenziali: si riconoscono a 18 px senza
   * leggere il nome, che resta nel suggerimento.
   */
  import type { MiiExpression } from '$lib/api';

  interface Props {
    expression: MiiExpression;
    size?: number;
  }

  const { expression, size = 18 }: Props = $props();

  /** Sopracciglia, occhi e bocca di ogni espressione, in un viewBox da 24. */
  const FACES: Record<
    MiiExpression,
    { brows?: string; eyes: string; mouth: string; fill?: boolean }
  > = {
    normal: { eyes: 'dots', mouth: 'M9 15.5h6' },
    smile: { eyes: 'dots', mouth: 'M8.5 14q3.5 3.6 7 0' },
    anger: {
      brows: 'M7.6 8.4l3 1.6M16.4 8.4l-3 1.6',
      eyes: 'dots',
      mouth: 'M9 16.6q3-2.6 6 0'
    },
    sorrow: {
      brows: 'M7.6 9.8l3-1.4M16.4 9.8l-3-1.4',
      eyes: 'dots',
      mouth: 'M9 16.6q3-2.6 6 0'
    },
    surprise: { eyes: 'round', mouth: 'M12 14.2a1.9 1.9 0 1 0 0 3.8 1.9 1.9 0 1 0 0-3.8z' },
    blink: { eyes: 'closed', mouth: 'M8.5 14q3.5 3.6 7 0' },
    open_mouth: { eyes: 'dots', mouth: 'M8.6 13.6h6.8q0 3.8-3.4 3.8t-3.4-3.8z', fill: true }
  };

  const face = $derived(FACES[expression]);
</script>

<svg
  class="face"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <circle cx="12" cy="12" r="9.2" />
  {#if face.brows}<path d={face.brows} />{/if}
  {#if face.eyes === 'dots'}
    <circle cx="9" cy="11" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="15" cy="11" r="1.1" fill="currentColor" stroke="none" />
  {:else if face.eyes === 'round'}
    <circle cx="9" cy="10.6" r="1.5" />
    <circle cx="15" cy="10.6" r="1.5" />
  {:else}
    <path d="M7.6 11q1.4 1.3 2.8 0M13.6 11q1.4 1.3 2.8 0" />
  {/if}
  <path d={face.mouth} fill={face.fill ? 'currentColor' : 'none'} />
</svg>

<style>
  .face {
    display: block;
    flex: none;
  }
</style>
