<script lang="ts" module>
  import type { IconName } from '$lib/components/Icon.svelte';

  /** Una scelta del menu. */
  export interface SelectOption<T extends string | number = string | number> {
    value: T;
    label: string;
    /** Spiegazione breve, in piccolo sotto il nome. */
    hint?: string;
    icon?: IconName;
  }

  let instances = 0;
</script>

<script lang="ts" generics="T extends string | number">
  /**
   * Menu a tendina del design system, al posto di `<select>`.
   *
   * Il `<select>` nativo apre un elenco disegnato dal sistema operativo, con
   * i suoi colori e il suo font: l'unico pezzo dell'interfaccia che non
   * sembra del launcher. Questo segue lo schema "select-only combobox" di
   * WAI-ARIA: il fuoco resta sul pulsante e l'opzione attiva la indica
   * `aria-activedescendant`.
   *
   * Tastiera: frecce, Home, Fine, Pag su/giù, lettere per saltare a una
   * voce, Invio o spazio per scegliere, Esc per chiudere senza cambiare, Tab
   * per uscire.
   *
   * L'elenco è `position: fixed`, calcolato dal pulsante: dentro un pannello
   * che scorre non viene tagliato. Si apre verso l'alto se sotto non c'è
   * posto, e si chiude se la pagina scorre.
   */
  import { tick } from 'svelte';

  import Icon from '$lib/components/Icon.svelte';
  import { tooltip } from '$lib/attachments/tooltip';

  interface Props {
    value: T;
    options: SelectOption<T>[];
    /** Nome del controllo, per chi non lo vede. */
    label: string;
    /** Spiegazione, nel suggerimento. */
    hint?: string | undefined;
    disabled?: boolean;
    /** Largo quanto il contenitore. */
    block?: boolean;
    onchange: (value: T) => void;
  }

  const {
    value,
    options,
    label,
    hint,
    disabled = false,
    block = false,
    onchange
  }: Props = $props();

  const id = `vk-select-${++instances}`;
  const ROW = 36;
  const MAX_HEIGHT = 288;

  let open = $state(false);
  let active = $state(0);
  let trigger = $state<HTMLButtonElement | null>(null);
  let list = $state<HTMLElement | null>(null);
  let box = $state({ left: 0, top: 0, width: 0, maxHeight: MAX_HEIGHT, up: false });

  const selectedIndex = $derived(options.findIndex((option) => option.value === value));
  const selected = $derived(options[selectedIndex]);

  function optionId(index: number): string {
    return `${id}-${index}`;
  }

  function place() {
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const below = window.innerHeight - rect.bottom - 12;
    const above = rect.top - 12;
    const wanted = Math.min(MAX_HEIGHT, options.length * ROW + 12);
    const up = below < wanted && above > below;
    box = {
      left: rect.left,
      width: rect.width,
      top: up ? rect.top - 6 : rect.bottom + 6,
      maxHeight: Math.max(120, Math.min(MAX_HEIGHT, up ? above : below)),
      up
    };
  }

  async function show(index = selectedIndex) {
    if (disabled || options.length === 0) return;
    place();
    active = Math.min(options.length - 1, Math.max(0, index));
    open = true;
    await tick();
    reveal();
  }

  function close(focus = true) {
    open = false;
    if (focus) trigger?.focus();
  }

  function choose(index: number) {
    const option = options[index];
    close();
    if (option && option.value !== value) onchange(option.value);
  }

  function move(index: number) {
    active = Math.min(options.length - 1, Math.max(0, index));
    reveal();
  }

  function reveal() {
    list
      ?.querySelector<HTMLElement>(`#${CSS.escape(optionId(active))}`)
      ?.scrollIntoView({ block: 'nearest' });
  }

  // --- Lettere: salta alla voce che comincia così --------------------------

  let typed = '';
  let typedAt = 0;

  function typeahead(key: string): number {
    const now = performance.now();
    typed = now - typedAt < 700 ? typed + key.toLowerCase() : key.toLowerCase();
    typedAt = now;
    // Una lettera ripetuta ("m", "mm") scorre le voci che cominciano con
    // quella; più lettere diverse cercano la voce che comincia così.
    const cycling = [...typed].every((char) => char === typed[0]);
    const query = cycling ? typed.charAt(0) : typed;
    const from = cycling ? active + 1 : active;
    for (let offset = 0; offset < options.length; offset += 1) {
      const index = (from + offset) % options.length;
      if (options[index]?.label.toLowerCase().startsWith(query)) return index;
    }
    return -1;
  }

  function printable(event: KeyboardEvent): boolean {
    return event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey;
  }

  function onkeydown(event: KeyboardEvent) {
    const { key } = event;

    if (!open) {
      if (key === 'ArrowDown' || key === 'ArrowUp' || key === 'Enter' || key === ' ') void show();
      else if (key === 'Home') void show(0);
      else if (key === 'End') void show(options.length - 1);
      else if (printable(event) && key !== ' ') {
        const index = typeahead(key);
        if (index < 0) return;
        void show(index);
      } else return;
      event.preventDefault();
      return;
    }

    if (key === 'ArrowDown') move(active + 1);
    else if (key === 'ArrowUp' && event.altKey) choose(active);
    else if (key === 'ArrowUp') move(active - 1);
    else if (key === 'Home') move(0);
    else if (key === 'End') move(options.length - 1);
    else if (key === 'PageDown') move(active + 8);
    else if (key === 'PageUp') move(active - 8);
    else if (key === 'Enter' || key === ' ') choose(active);
    else if (key === 'Escape') {
      // Esc chiude il menu e basta: non deve arrivare a chi sta sotto (un
      // dialogo, l'editor) e chiudere anche quello.
      event.stopPropagation();
      close();
    } else if (key === 'Tab') {
      close(false);
      return;
    } else if (printable(event)) {
      const index = typeahead(key);
      if (index >= 0) move(index);
    } else return;
    event.preventDefault();
  }

  // --- Chiusura da fuori ----------------------------------------------------

  function onWindowPointer(event: PointerEvent) {
    const target = event.target as Node;
    if (open && !trigger?.contains(target) && !list?.contains(target)) close(false);
  }

  $effect(() => {
    if (!open) return;
    // Scorre la pagina, non l'elenco: il pulsante si è spostato.
    const onScroll = (event: Event) => {
      if (event.target !== list) close(false);
    };
    const onResize = () => close(false);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', onResize);
    };
  });
</script>

<svelte:window onpointerdown={onWindowPointer} />

<button
  bind:this={trigger}
  type="button"
  class="vk-input select"
  class:block
  class:open
  role="combobox"
  aria-label={label}
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-controls={`${id}-list`}
  aria-activedescendant={open ? optionId(active) : undefined}
  {disabled}
  {@attach tooltip(hint)}
  onclick={() => (open ? close() : void show())}
  {onkeydown}
>
  {#if selected?.icon}<Icon name={selected.icon} size={15} />{/if}
  <span class="current">{selected?.label ?? ''}</span>
  <span class="chevron"><Icon name="chevron" size={14} /></span>
</button>

{#if open}
  <div
    bind:this={list}
    id={`${id}-list`}
    class="list"
    class:up={box.up}
    role="listbox"
    aria-label={label}
    tabindex="-1"
    style:left="{box.left}px"
    style:top="{box.top}px"
    style:min-width="{box.width}px"
    style:max-height="{box.maxHeight}px"
  >
    {#each options as option, index (option.value)}
      <div
        id={optionId(index)}
        class="option"
        class:active={index === active}
        class:chosen={index === selectedIndex}
        role="option"
        tabindex="-1"
        aria-selected={index === selectedIndex}
        onpointerdown={(event) => event.preventDefault()}
        onpointermove={() => (active = index)}
        onclick={() => choose(index)}
        {onkeydown}
      >
        {#if option.icon}<Icon name={option.icon} size={15} />{/if}
        <span class="option-text">
          <span>{option.label}</span>
          {#if option.hint}<span class="option-hint">{option.hint}</span>{/if}
        </span>
        {#if index === selectedIndex}
          <span class="tick" aria-hidden="true"></span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .select {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding-right: 10px;
    color: var(--vk-text);
    font: inherit;
    font-size: var(--vk-fs-small);
    font-weight: 700;
    text-align: left;
    cursor: pointer;
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .select.block {
    display: flex;
    width: 100%;
  }

  .select:hover:not(:disabled) {
    border-color: #3a4c74;
  }

  .select.open {
    border-color: #4c5c8c;
  }

  .select:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .current {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chevron {
    display: grid;
    color: var(--vk-text-secondary);
    transition: transform var(--vk-dur-fast) var(--vk-ease);
  }

  .open .chevron {
    transform: rotate(180deg);
  }

  /* ---- Elenco ---- */

  .list {
    position: fixed;
    z-index: 900;
    display: flex;
    flex-direction: column;
    max-width: min(360px, calc(100vw - 16px));
    padding: 6px;
    overflow-y: auto;
    border: 1px solid #33456b;
    border-radius: var(--vk-radius-badge);
    /* Un tono sopra le card: l'elenco galleggia su di loro e deve staccarsi. */
    background: #1a2338;
    box-shadow: var(--vk-shadow-modal);
    animation: pop var(--vk-dur-fast) var(--vk-ease);
    overscroll-behavior: contain;
  }

  .list.up {
    transform: translateY(-100%);
    animation-name: pop-up;
  }

  .option {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    min-height: 32px;
    padding: 7px 10px;
    border-radius: 8px;
    color: var(--vk-text);
    font-size: var(--vk-fs-small);
    font-weight: 600;
    cursor: pointer;
  }

  .option.active {
    background: rgb(255 255 255 / 0.08);
  }

  .option.chosen {
    font-weight: 800;
  }

  .option-text {
    display: grid;
    flex: 1;
    gap: 1px;
    min-width: 0;
  }

  .option-hint {
    color: var(--vk-text-faint);
    font-size: var(--vk-fs-micro);
    font-weight: 600;
  }

  /* La spunta porta l'arcobaleno: è il colore del tema su ciò che è scelto. */
  .tick {
    display: grid;
    width: 14px;
    height: 14px;
    background: var(--vk-rainbow);
    mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'%3E%3Cpath d='M9.6 16.2 5.4 12l-1.4 1.4 5.6 5.6L20.4 8.2 19 6.8z'/%3E%3C/svg%3E")
      center / contain no-repeat;
  }

  @keyframes pop {
    from {
      opacity: 0;
      translate: 0 -4px;
    }
  }

  @keyframes pop-up {
    from {
      opacity: 0;
      translate: 0 4px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .list,
    .chevron {
      animation: none;
      transition: none;
    }
  }
</style>
