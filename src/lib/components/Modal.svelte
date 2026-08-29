<script lang="ts">
  /**
   * Dialogo modale, sostituisce `ShowCustomDialog` del WPF.
   *
   * Lo sfondo è un vero `<button>`: chiudere cliccando fuori resta possibile
   * anche da tastiera, senza handler `onclick` su elementi non interattivi.
   */
  import type { Snippet } from 'svelte';

  import { t } from '$lib/stores/i18n.svelte';

  interface Props {
    open: boolean;
    title: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    busy?: boolean;
    onconfirm?: () => void;
    oncancel: () => void;
    children: Snippet;
  }

  const {
    open,
    title,
    confirmLabel,
    cancelLabel,
    danger = false,
    busy = false,
    onconfirm,
    oncancel,
    children
  }: Props = $props();

  function onKeydown(event: KeyboardEvent) {
    if (open && event.key === 'Escape' && !busy) oncancel();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <div class="overlay">
    <button class="backdrop" aria-label={t('common.closeWindow')} onclick={oncancel} disabled={busy}
    ></button>

    <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
      <h2 class="title">{title}</h2>
      <div class="content">{@render children()}</div>
      <div class="actions">
        <button class="vk-btn" onclick={oncancel} disabled={busy}>
          {cancelLabel ?? t('common.close')}
        </button>
        {#if onconfirm && confirmLabel}
          <button
            class="vk-btn {danger ? 'vk-btn--danger' : 'vk-btn--primary'}"
            onclick={onconfirm}
            disabled={busy}
          >
            {confirmLabel}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    animation: fade var(--vk-dur) var(--vk-ease);
  }

  .backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: rgb(4 7 14 / 0.72);
    backdrop-filter: blur(3px);
    cursor: default;
  }

  .dialog {
    position: relative;
    width: min(560px, 100%);
    max-height: 80vh;
    overflow: auto;
    padding: 24px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel);
    box-shadow: var(--vk-shadow-modal);
  }

  .dialog::before {
    content: '';
    position: absolute;
    inset: 0 0 auto;
    height: 2px;
    border-radius: var(--vk-radius-card) var(--vk-radius-card) 0 0;
    background: var(--vk-rainbow);
  }

  .title {
    margin: 0 0 12px;
    font-size: 20px;
    font-weight: 900;
  }

  .content {
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-small);
    line-height: 1.6;
  }

  .content :global(p) {
    margin: 0 0 10px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 22px;
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>
