<script lang="ts" module>
  import type { IconName } from '$lib/components/Icon.svelte';

  /** Una voce del menu. */
  export interface MenuItem {
    label: string;
    icon?: IconName;
    /** Spiegazione breve, mostrata come suggerimento. */
    hint?: string;
    disabled?: boolean;
    /** Azione distruttiva: in rosso. */
    danger?: boolean;
    onselect: () => void;
  }
</script>

<script lang="ts">
  /**
   * Pulsante `⋯` con un menu di azioni secondarie.
   *
   * Serve a tenere pulite le pagine: le azioni che si usano ogni tanto —
   * verificare, riparare, aprire una cartella, esportare — stanno qui invece
   * di allinearsi come pulsanti accanto a quella che conta.
   *
   * Tastiera: frecce su e giù fra le voci, Esc chiude e riporta il fuoco sul
   * pulsante, Tab esce dal menu chiudendolo.
   */
  import { tick } from 'svelte';

  import Icon from '$lib/components/Icon.svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import { t } from '$lib/stores/i18n.svelte';

  interface Props {
    items: MenuItem[];
    /** Descrizione del pulsante per chi non lo vede. */
    label?: string;
    /** Il menu si apre verso l'alto: per i pulsanti in fondo a una pagina. */
    up?: boolean;
    disabled?: boolean;
    /** Pulsante piccolo, per stare sopra una tessera. */
    compact?: boolean;
  }

  const { items, label, up = false, disabled = false, compact = false }: Props = $props();

  let open = $state(false);
  let root = $state<HTMLElement | null>(null);
  let trigger = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLElement | null>(null);

  // Fuori dai contenitori con overflow/transform, anche nelle tessere Mii.
  function attachMenu(node: HTMLElement) {
    document.body.append(node);

    function place() {
      if (!trigger) return;
      const rect = trigger.getBoundingClientRect();
      const margin = 8;
      const gap = 6;
      const { offsetWidth: width, offsetHeight: height } = node;
      const below = window.innerHeight - rect.bottom - gap - margin;
      const above = rect.top - gap - margin;
      const opensUp = up ? above >= height || above > below : below < height && above > below;
      const top = opensUp ? rect.top - gap - height : rect.bottom + gap;
      node.style.left = `${Math.max(margin, Math.min(rect.right - width, window.innerWidth - width - margin))}px`;
      node.style.top = `${Math.max(margin, Math.min(top, window.innerHeight - height - margin))}px`;
    }

    function onScroll(event: Event) {
      if (!node.contains(event.target as Node)) close();
    }

    place();
    const observer = new ResizeObserver(place);
    observer.observe(node);
    window.addEventListener('resize', place);
    window.addEventListener('scroll', onScroll, true);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', onScroll, true);
      node.remove();
    };
  }

  async function toggle() {
    open = !open;
    if (open) {
      await tick();
      focusItem(0);
    }
  }

  function close(returnFocus = false) {
    open = false;
    if (returnFocus) trigger?.focus();
  }

  function enabledItems(): HTMLButtonElement[] {
    return [...(menu?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
  }

  function focusItem(index: number) {
    const buttons = enabledItems();
    if (buttons.length === 0) return;
    buttons[(index + buttons.length) % buttons.length]?.focus();
  }

  function onMenuKey(event: KeyboardEvent) {
    const buttons = enabledItems();
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === 'ArrowDown') focusItem(current + 1);
    else if (event.key === 'ArrowUp') focusItem(current - 1);
    else if (event.key === 'Escape') {
      // Esc chiude il menu e basta: non deve arrivare a un dialogo o
      // all'editor Mii sotto e chiudere anche quelli.
      event.stopPropagation();
      close(true);
    } else if (event.key === 'Tab') close();
    else return;
    event.preventDefault();
  }

  function select(item: MenuItem) {
    close(true);
    item.onselect();
  }

  function onWindowPointer(event: PointerEvent) {
    if (
      open &&
      root &&
      !root.contains(event.target as Node) &&
      !menu?.contains(event.target as Node)
    ) {
      close();
    }
  }
</script>

<svelte:window onpointerdown={onWindowPointer} />

<div class="menu-root" bind:this={root}>
  <button
    bind:this={trigger}
    class="vk-btn trigger"
    class:compact
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label={label ?? t('common.more')}
    {@attach tooltip(open ? undefined : (label ?? t('common.more')))}
    {disabled}
    onclick={toggle}
  >
    <Icon name="more" size={compact ? 14 : 16} />
  </button>

  {#if open}
    <div
      class="menu"
      role="menu"
      tabindex="-1"
      bind:this={menu}
      {@attach attachMenu}
      onkeydown={onMenuKey}
    >
      {#each items as item (item.label)}
        <button
          class="item"
          class:danger={item.danger}
          role="menuitem"
          {@attach tooltip(item.hint)}
          disabled={item.disabled}
          onclick={() => select(item)}
        >
          {#if item.icon}<Icon name={item.icon} size={14} />{/if}
          <span>{item.label}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .menu-root {
    position: relative;
    display: inline-flex;
  }

  .trigger {
    padding: 8px 10px;
  }

  .trigger.compact {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border-radius: 8px;
  }

  .menu {
    position: fixed;
    z-index: 100;
    display: flex;
    flex-direction: column;
    min-width: 220px;
    max-width: calc(100vw - 16px);
    max-height: calc(100vh - 16px);
    overflow-y: auto;
    box-sizing: border-box;
    padding: 6px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel);
    box-shadow: var(--vk-shadow-modal);
    animation: pop var(--vk-dur-fast) var(--vk-ease);
  }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--vk-text);
    font: inherit;
    font-size: var(--vk-fs-small);
    font-weight: 700;
    text-align: left;
    cursor: pointer;
  }

  .item:hover:not(:disabled),
  .item:focus-visible {
    outline: none;
    background: rgb(255 255 255 / 0.06);
  }

  .item :global(.vk-icon) {
    color: var(--vk-text-secondary);
  }

  .item.danger,
  .item.danger :global(.vk-icon) {
    color: var(--vk-danger);
  }

  .item:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .menu {
      animation: none;
    }
  }
</style>
