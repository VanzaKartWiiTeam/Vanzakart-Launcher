<script lang="ts">
  /**
   * Debug.
   *
   * Ricalca il `DebugView` del WPF: console con lo stato dell'installazione,
   * coda del log e i pulsanti per aprire le cartelle. Tutto ciò che compare
   * qui è già sanitizzato dal backend.
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import { app } from '$lib/stores/app.svelte';
  import type { BackupSummary, DiagnosticEntry } from '$lib/api/types';

  let entries = $state<DiagnosticEntry[]>([]);
  let log = $state('');
  let backups = $state<BackupSummary[]>([]);
  let loading = $state(true);
  let purgeOpen = $state(false);
  let confirmation = $state('');
  let purging = $state(false);

  $effect(() => {
    void load();
  });

  async function load() {
    loading = true;
    try {
      [entries, log, backups] = await Promise.all([
        api.collectDiagnostics(),
        api.readLog(),
        api.listBackups()
      ]);
    } catch (error) {
      app.toast('Diagnostica non disponibile', api.errorMessage(error), 'warning');
    } finally {
      loading = false;
    }
  }

  async function copyReport() {
    const report = entries.map((entry) => `${entry.label}: ${entry.value}`).join('\n');
    try {
      await navigator.clipboard.writeText(report);
      app.toast('Copiato', 'Rapporto diagnostico negli appunti.', 'success');
    } catch {
      app.toast('Copia non riuscita', 'Gli appunti non sono accessibili.', 'warning');
    }
  }

  async function purge() {
    purging = true;
    try {
      const removed = await api.purgeUserData(confirmation);
      app.toast('Dati cancellati', `${removed.length} cartelle svuotate.`, 'success');
      purgeOpen = false;
      confirmation = '';
      await load();
    } catch (error) {
      app.toast('Cancellazione non riuscita', api.errorMessage(error), 'warning');
    } finally {
      purging = false;
    }
  }
</script>

<div class="page">
  <section class="vk-card">
    <div class="head">
      <p class="vk-eyebrow">Stato dell'installazione</p>
      <div class="vk-row">
        <button class="vk-btn" onclick={load} disabled={loading}>
          <Icon name="refresh" size={14} />
          Aggiorna
        </button>
        <button class="vk-btn" onclick={copyReport}>Copia rapporto</button>
      </div>
    </div>

    <dl class="entries">
      {#each entries as entry (entry.label)}
        <div class="entry">
          <dt>{entry.label}</dt>
          <dd>
            {#if entry.ok === true}
              <span class="dot ok"></span>
            {:else if entry.ok === false}
              <span class="dot bad"></span>
            {/if}
            <span class="value">{entry.value}</span>
          </dd>
        </div>
      {/each}
    </dl>
  </section>

  <section class="vk-card">
    <div class="head">
      <p class="vk-eyebrow">Log del launcher</p>
      <button class="vk-btn" onclick={() => api.openFolder('logs')}>
        <Icon name="folder" size={14} />
        Apri cartella log
      </button>
    </div>
    <pre class="log vk-mono">{log}</pre>
  </section>

  <section class="vk-card">
    <div class="head">
      <p class="vk-eyebrow">Backup dei dati utente</p>
      <button class="vk-btn" onclick={() => api.openFolder('backups')}>
        <Icon name="folder" size={14} />
        Apri cartella
      </button>
    </div>

    {#if backups.length === 0}
      <p class="vk-faint">Nessun backup: verrà creato al primo aggiornamento della modpack.</p>
    {:else}
      <ul class="backups">
        {#each backups as backup (backup.id)}
          <li>
            <span class="vk-mono">{backup.id}</span>
            <span class="vk-faint">{backup.fileCount} file protetti</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="vk-card danger-zone">
    <div>
      <p class="vk-eyebrow">Zona pericolosa</p>
      <p class="vk-subtitle">
        Svuota cache, download e log del launcher. Impostazioni, modpack installate e salvataggi di
        Dolphin non vengono toccati.
      </p>
    </div>
    <button class="vk-btn vk-btn--danger" onclick={() => (purgeOpen = true)}>
      <Icon name="warning" size={14} />
      Cancella dati temporanei
    </button>
  </section>
</div>

<Modal
  open={purgeOpen}
  title="Cancellare i dati temporanei?"
  confirmLabel="Cancella"
  danger
  busy={purging}
  onconfirm={purge}
  oncancel={() => {
    purgeOpen = false;
    confirmation = '';
  }}
>
  <p>Verranno svuotate le cartelle cache, download e log.</p>
  <p>Digita <strong>VanzaKart</strong> per confermare.</p>
  <input class="vk-input" bind:value={confirmation} placeholder="VanzaKart" autocomplete="off" />
</Modal>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-bottom: 12px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  .entries {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 8px 24px;
    margin: 0;
  }

  .entry {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid rgb(42 56 87 / 0.4);
    font-size: var(--vk-fs-small);
  }

  dt {
    color: var(--vk-text-secondary);
  }

  dd {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    min-width: 0;
  }

  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
  }

  .dot.ok {
    background: var(--vk-success);
    box-shadow: 0 0 8px var(--vk-success);
  }

  .dot.bad {
    background: var(--vk-danger);
    box-shadow: 0 0 8px var(--vk-danger);
  }

  .log {
    max-height: 320px;
    margin: 0;
    padding: 14px;
    overflow: auto;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-input);
    background: var(--vk-input);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-micro);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .backups {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .backups li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid rgb(42 56 87 / 0.4);
    font-size: var(--vk-fs-small);
  }

  .danger-zone {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    border-color: rgb(255 107 130 / 0.35);
  }
</style>
