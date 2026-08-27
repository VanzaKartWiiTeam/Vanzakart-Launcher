<script lang="ts">
  /** Toast in basso a destra, come `ShowToast` del launcher WPF. */
  import { app } from '$lib/stores/app.svelte';
</script>

<div class="stack" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast toast--{toast.tone}">
      <div class="body">
        <p class="title">{toast.title}</p>
        <p class="message">{toast.message}</p>
      </div>
      <button class="dismiss" onclick={() => app.dismissToast(toast.id)} aria-label="Chiudi"
        >×</button
      >
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    right: 24px;
    bottom: 24px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 380px;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel-glass);
    box-shadow: var(--vk-shadow-modal);
    animation: toast-in var(--vk-dur) var(--vk-ease);
  }

  .toast::before {
    content: '';
    position: absolute;
    inset: 0 0 auto;
    height: 2px;
    border-radius: var(--vk-radius-card) var(--vk-radius-card) 0 0;
    background: var(--vk-rainbow);
  }

  .toast {
    position: relative;
    overflow: hidden;
  }

  .toast--success {
    border-color: rgb(77 255 176 / 0.4);
  }
  .toast--warning {
    border-color: rgb(255 209 102 / 0.4);
  }
  .toast--danger {
    border-color: rgb(255 107 130 / 0.45);
  }

  .body {
    min-width: 0;
  }

  .title {
    margin: 0 0 4px;
    font-size: var(--vk-fs-small);
    font-weight: 800;
  }

  .message {
    margin: 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
    overflow-wrap: anywhere;
  }

  .dismiss {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--vk-text-faint);
    font-size: 20px;
    line-height: 1;
  }

  .dismiss:hover {
    color: var(--vk-text);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(12px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>
