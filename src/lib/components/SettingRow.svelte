<script lang="ts">
  /**
   * Una riga di impostazione: il nome a sinistra, il controllo a destra.
   *
   * La spiegazione non sta più sotto il nome, in piccolo, su ogni riga: si
   * legge dal suggerimento della «i», per chi la cerca. Il pallino dice che
   * il valore è diverso da quello consigliato, e il suo suggerimento dice
   * quale sarebbe (§D-102).
   */
  import type { Snippet } from 'svelte';

  import Icon from '$lib/components/Icon.svelte';
  import { tooltip } from '$lib/attachments/tooltip';

  interface Props {
    label: string;
    /** Spiegazione, nel suggerimento della «i». */
    hint?: string | undefined;
    /** Diverso dal consigliato: il testo dice il valore consigliato. */
    note?: string | undefined;
    children: Snippet;
  }

  const { label, hint, note, children }: Props = $props();
</script>

<div class="setting">
  <div class="text">
    <span class="name">{label}</span>
    {#if hint}
      <button type="button" class="info" aria-label={hint} {@attach tooltip(hint)}>
        <Icon name="info" size={14} />
      </button>
    {/if}
    {#if note}
      <span class="changed" role="img" aria-label={note} {@attach tooltip(note)}></span>
    {/if}
  </div>
  <div class="control">
    {@render children()}
  </div>
</div>

<style>
  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 52px;
    padding: 8px 0;
  }

  /* Svelte non vede i fratelli di un componente: il selettore è globale, e
     `.setting` c'è solo qui. */
  :global(.setting + .setting) {
    border-top: 1px solid rgb(255 255 255 / 0.05);
  }

  .text {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .name {
    font-size: var(--vk-fs-small);
    font-weight: 700;
  }

  .info {
    display: grid;
    flex: none;
    place-items: center;
    padding: 2px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--vk-text-faint);
    cursor: help;
  }

  .info:hover {
    color: var(--vk-text-secondary);
  }

  /* Diverso dal consigliato: un avviso, non un errore. */
  .changed {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--vk-warning);
    box-shadow: 0 0 8px rgb(255 209 102 / 0.6);
    cursor: help;
  }

  .control {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: flex-end;
    width: min(300px, 45%);
  }
</style>
