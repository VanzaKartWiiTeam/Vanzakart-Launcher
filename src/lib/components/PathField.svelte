<script lang="ts">
  /**
   * Un percorso configurabile: si sceglie con «Sfoglia» oppure si scrive.
   *
   * Scriverlo a mano serve più spesso di quanto sembri — un percorso copiato
   * dal terminale, una cartella su un disco di rete, una installazione che il
   * selettore di sistema non attraversa — e finché il campo era in sola
   * lettura l'unica strada era il dialogo (§D-076).
   *
   * Si salva con Invio o uscendo dal campo, e solo se il testo è cambiato:
   * ogni salvataggio passa dal backend, che verifica che il percorso esista
   * davvero. Esc rimette quello salvato.
   */
  import { untrack } from 'svelte';

  import { t } from '$lib/stores/i18n.svelte';

  interface Props {
    label: string;
    /** Percorso salvato, quello che il backend conosce. */
    value: string;
    valid: boolean;
    placeholder: string;
    /** Apre il selettore di sistema. */
    onbrowse: () => void;
    /** Salva. Restituisce il messaggio d'errore, oppure `null` se è andata. */
    onsave: (value: string) => Promise<string | null>;
  }

  const { label, value, valid, placeholder, onbrowse, onsave }: Props = $props();

  // Il primo valore si prende una volta e basta: gli aggiornamenti successivi
  // li porta l'`$effect` qui sotto, che sa se il campo è in uso.
  let draft = $state(untrack(() => value));
  let editing = $state(false);
  let saving = $state(false);
  let error = $state('');

  // Il valore salvato può cambiare da fuori — «Sfoglia», rilevamento
  // automatico, ricarica — ma non mentre lo si sta scrivendo.
  $effect(() => {
    const saved = value;
    if (!editing) {
      draft = saved;
      error = '';
    }
  });

  const changed = $derived(draft.trim() !== value.trim());

  async function commit() {
    editing = false;
    if (!changed || saving) {
      error = '';
      return;
    }

    saving = true;
    try {
      const failure = await onsave(draft.trim());
      error = failure ?? '';
      // Un percorso rifiutato resta scritto: si corregge, non si riscrive.
      if (!failure) draft = draft.trim();
    } finally {
      saving = false;
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      void commit();
    } else if (event.key === 'Escape') {
      draft = value;
      error = '';
      editing = false;
      (event.currentTarget as HTMLInputElement).blur();
    }
  }
</script>

<div class="path-row">
  <div class="path-label">
    <span>{label}</span>
    <span class="vk-badge {valid ? 'vk-badge--success' : 'vk-badge--danger'}">
      {valid ? t('path.ok') : t('path.missing')}
    </span>
    {#if changed && !saving}
      <span class="vk-faint hint">{t('path.hint')}</span>
    {/if}
  </div>

  <input
    class="vk-input"
    class:wrong={error !== ''}
    bind:value={draft}
    {placeholder}
    spellcheck="false"
    autocapitalize="off"
    autocomplete="off"
    disabled={saving}
    onfocus={() => (editing = true)}
    onblur={commit}
    {onkeydown}
  />

  <button class="vk-btn" onclick={onbrowse} disabled={saving}>{t('path.browse')}</button>

  {#if error}
    <p class="vk-error inline">{error}</p>
  {/if}
</div>

<style>
  .path-row {
    display: grid;
    grid-template-columns: 210px 1fr auto;
    align-items: center;
    gap: 12px;
  }

  .path-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--vk-fs-small);
  }

  .hint {
    font-size: var(--vk-fs-eyebrow);
    white-space: nowrap;
  }

  .wrong {
    border-color: var(--vk-danger);
  }

  /* L'errore sta sotto il campo, incolonnato con esso. */
  .inline {
    grid-column: 2 / 4;
    padding: 8px 10px;
    margin: 0;
    font-size: var(--vk-fs-micro);
  }

  @media (max-width: 900px) {
    .path-row {
      grid-template-columns: 1fr auto;
    }

    .path-label {
      grid-column: 1 / 3;
    }

    .inline {
      grid-column: 1 / 3;
    }
  }
</style>
