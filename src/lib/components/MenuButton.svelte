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
  import { t } from '$lib/stores/i18n.svelte';

  interface Props {
    items: MenuItem[];
    /** Descrizione del pulsante per chi non lo vede. */
    label?: string;
    /** Il menu si apre verso l'alto: per i pulsanti in fondo a una pagina. */
    up?: boolean;
    disabled?: boolean;
  }

  const { items, label, up = false, disabled = false }: Props = $props();

  let open = $state(false);
  let root = $state<HTMLElement | null>(null);
  let trigger = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLElement | null>(null);

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
    else if (event.key === 'Escape') close(true);
    else if (event.key === 'Tab') close();
    else return;
    event.preventDefault();
  }

  function select(item: MenuItem) {
    close(true);
    item.onselect();
  }

  function onWindowPointer(event: PointerEvent) {
    if (open && root && !root.contains(event.target as Node)) close();
  }
</script>

<svelte:window onpointerdown={onWindowPointer} />

<div class="menu-root" bind:this={root}>
  <button
    bind:this={trigger}
    class="vk-btn trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label={label ?? t('common.more')}
    title={label ?? t('common.more')}
    {disabled}
    onclick={toggle}
  >
    <Icon name="more" size={16} />
  </button>

  {#if open}
    <div class="menu" class:up role="menu" tabindex="-1" bind:this={menu} onkeydown={onMenuKey}>
      {#each items as item (item.label)}
        <button
          class="item"
          class:danger={item.danger}
          role="menuitem"
          title={item.hint}
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

  .menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 40;
    display: flex;
    flex-direction: column;
    min-width: 220px;
    padding: 6px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel);
    box-shadow: var(--vk-shadow-modal);
    animation: pop var(--vk-dur-fast) var(--vk-ease);
  }

  .menu.up {
    top: auto;
    bottom: calc(100% + 6px);
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
