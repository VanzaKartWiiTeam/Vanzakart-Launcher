<script lang="ts">
  /**
   * Guscio dell'installer: sfondo, barra del titolo e scelta della modalità.
   *
   * È il backend a dire se questo binario è stato avviato come installer o
   * come disinstallatore: la UI non lo indovina dal nome del file.
   */
  import AmbientBackdrop from '$lib/components/AmbientBackdrop.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import SetupTitleBar from '$setup/lib/components/SetupTitleBar.svelte';
  import InstallWizard from './InstallWizard.svelte';
  import UninstallPanel from './UninstallPanel.svelte';
  import * as api from '$setup/lib/api';
  import type { Bootstrap } from '$setup/lib/api';

  let boot = $state<Bootstrap | null>(null);
  let bootError = $state('');
  let busy = $state(false);

  const subtitle = $derived(
    boot
      ? boot.mode === 'uninstall'
        ? `Disinstallazione · v${boot.setupVersion}`
        : `Setup · v${boot.setupVersion}`
      : ''
  );

  async function load() {
    bootError = '';
    try {
      boot = await api.bootstrap();
    } catch (error) {
      bootError = api.errorMessage(error);
    }
  }

  void load();
</script>

<div class="shell">
  <AmbientBackdrop />
  <SetupTitleBar {subtitle} {busy} />

  {#if boot}
    {#if boot.mode === 'uninstall'}
      <UninstallPanel onBusyChange={(value) => (busy = value)} />
    {:else}
      <InstallWizard {boot} onBusyChange={(value) => (busy = value)} />
    {/if}
  {:else if bootError}
    <div class="vk-empty">
      <Icon name="warning" size={28} />
      <p>Non riesco ad avviare l'installer.</p>
      <p class="vk-faint">{bootError}</p>
      <button class="vk-btn" onclick={load}>
        <Icon name="refresh" size={14} />
        Riprova
      </button>
    </div>
  {:else}
    <div class="vk-empty">
      <p class="vk-muted">Preparazione…</p>
    </div>
  {/if}
</div>

<style>
  .shell {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    border: 1px solid var(--vk-window-border);
  }

  /* Lo sfondo animato sta dietro a tutto, ma dentro il bordo della finestra. */
  .shell > :global(.backdrop) {
    z-index: 0;
  }

  .shell > :global(:not(.backdrop)) {
    position: relative;
    z-index: 1;
  }
</style>
