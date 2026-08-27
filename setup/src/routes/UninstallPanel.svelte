<script lang="ts">
  /**
   * La disinstallazione.
   *
   * A differenza del disinstallatore legacy, che chiedeva due sì o no e poi
   * andava a memoria, qui l'elenco di ciò che sparisce è visibile prima di
   * premere il pulsante, con le dimensioni. Tutto ciò che è dell'utente —
   * impostazioni, modpack, salvataggi — resta se non lo si sceglie.
   */
  import { onDestroy } from 'svelte';
  import { confirm } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import Icon from '$lib/components/Icon.svelte';
  import StepProgress from '$setup/lib/components/StepProgress.svelte';
  import * as api from '$setup/lib/api';
  import type {
    ProgressEvent,
    RemovalPlan,
    UninstallOptions,
    UninstallReport
  } from '$setup/lib/api';
  import { formatBytes } from '$setup/lib/format';

  let { onBusyChange }: { onBusyChange: (busy: boolean) => void } = $props();

  type Phase = 'choose' | 'running' | 'done' | 'missing';

  let phase = $state<Phase>('choose');
  let plan = $state<RemovalPlan | null>(null);
  let planError = $state('');
  let report = $state<UninstallReport | null>(null);
  let progress = $state<ProgressEvent | null>(null);
  let log = $state<string[]>([]);
  let busy = $state(false);

  const options = $state<UninstallOptions>({
    removeLauncherData: false,
    removeCacheAndLogs: true,
    removeModpacks: false,
    removeModpackUserData: false
  });

  let unlisten: (() => void) | undefined;

  void api
    .onProgress((event) => {
      progress = event;
      const line = `${event.phase} · ${event.detail}`;
      if (log[log.length - 1] !== line) log = [...log, line];
    })
    .then((stop) => {
      unlisten = stop;
    });

  onDestroy(() => unlisten?.());

  async function refreshPlan() {
    try {
      plan = await api.uninstallPlan($state.snapshot(options));
      planError = '';
      phase = 'choose';
    } catch (error) {
      plan = null;
      planError = api.errorMessage(error);
      if (api.errorCode(error) === 'not-installed') phase = 'missing';
    }
  }

  // Ogni opzione cambia l'elenco: si rilegge a ogni modifica.
  $effect(() => {
    void options.removeLauncherData;
    void options.removeCacheAndLogs;
    void options.removeModpacks;
    void options.removeModpackUserData;
    if (phase === 'choose' || phase === 'missing') void refreshPlan();
  });

  const irreversible = $derived(
    options.removeLauncherData || options.removeModpacks || options.removeModpackUserData
  );

  async function run() {
    if (irreversible) {
      const confirmed = await confirm(
        'Verranno rimossi anche dati che non si possono recuperare: impostazioni, modpack o salvataggi, secondo quanto hai scelto. Procedo?',
        { title: 'Disinstalla VanzaKart Launcher', kind: 'warning' }
      );
      if (!confirmed) return;
    }

    busy = true;
    onBusyChange(true);
    phase = 'running';
    log = [];
    progress = null;

    try {
      report = await api.uninstallRun($state.snapshot(options));
      phase = 'done';
    } catch (error) {
      planError = api.errorMessage(error);
      phase = 'choose';
    } finally {
      busy = false;
      onBusyChange(false);
    }
  }

  async function close() {
    await getCurrentWindow().close();
  }
</script>

<div class="panel">
  <main>
    {#if phase === 'running'}
      <StepProgress title="Rimozione in corso" {progress} {log} />
    {:else if phase === 'done' && report}
      <div class="vk-view-enter view">
        <header>
          <p class="vk-eyebrow">Fatto</p>
          <h1 class="vk-title">VanzaKart Launcher è stato rimosso</h1>
        </header>

        <section class="vk-card">
          <p class="vk-muted">
            {report.removed.length} elementi rimossi · {formatBytes(report.bytesFreed)} liberati
          </p>
          {#if report.deferred}
            <p class="vk-faint">
              La cartella d'installazione viene cancellata alla chiusura di questa finestra:
              contiene il programma che stai usando adesso.
            </p>
          {/if}
          {#if report.failed.length > 0}
            <ul class="failures">
              {#each report.failed as failure (failure.path)}
                <li>
                  <Icon name="warning" size={12} />
                  <span><span class="vk-mono">{failure.path}</span> — {failure.reason}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <p class="vk-faint">
          Grazie per aver corso con noi. Puoi reinstallare quando vuoi dalla pagina dei download.
        </p>
      </div>
    {:else if phase === 'missing'}
      <div class="vk-empty">
        <Icon name="package" size={28} />
        <p>Nessuna installazione di VanzaKart Launcher trovata su questo computer.</p>
        <p class="vk-faint">{planError}</p>
      </div>
    {:else}
      <div class="vk-view-enter view">
        <header>
          <p class="vk-eyebrow">Disinstallazione</p>
          <h1 class="vk-title">Rimuovi VanzaKart Launcher</h1>
          {#if plan}
            <p class="vk-subtitle">
              {#if plan.version}Versione {plan.version} ·{/if}
              <span class="vk-mono">{plan.installDir}</span>
            </p>
          {/if}
        </header>

        {#if planError}
          <p class="vk-error">{planError}</p>
        {/if}

        <section class="vk-card">
          <p class="vk-eyebrow">Cosa rimuovere oltre al programma</p>

          <label class="check">
            <input type="checkbox" bind:checked={options.removeCacheAndLogs} />
            <span>
              <strong>Cache, log e download interrotti</strong>
              <span class="vk-faint">Si rigenerano da soli. Non contengono nulla di tuo.</span>
            </span>
          </label>

          <label class="check">
            <input type="checkbox" bind:checked={options.removeLauncherData} />
            <span>
              <strong>Impostazioni e dati del launcher</strong>
              <span class="vk-faint">
                Percorsi di Dolphin, preferenze, Mii importati. Reinstallando dovrai riconfigurare
                tutto.
              </span>
            </span>
          </label>

          <label class="check" class:check--off={!plan?.hasModpacks}>
            <input
              type="checkbox"
              bind:checked={options.removeModpacks}
              disabled={!plan?.hasModpacks}
            />
            <span>
              <strong>Modpack installate in Dolphin</strong>
              <span class="vk-faint">
                {plan?.hasModpacks
                  ? 'Le cartelle VanzaKart e VKBeta dentro Load/Riivolution.'
                  : 'Nessuna modpack trovata: non c’è niente da togliere.'}
              </span>
            </span>
          </label>

          <label class="check" class:check--off={!plan?.hasModpacks}>
            <input
              type="checkbox"
              bind:checked={options.removeModpackUserData}
              disabled={!plan?.hasModpacks}
            />
            <span>
              <strong>Salvataggi e personalizzazioni della modpack</strong>
              <span class="vk-faint">
                I dati di gioco in <span class="vk-mono">*_UserData</span>: licenze, tempi, addon
                locali. Non si recuperano.
              </span>
            </span>
          </label>
        </section>

        <section class="vk-card vk-card--flush">
          <div class="plan-head">
            <p class="vk-eyebrow">Verrà rimosso</p>
            <p class="total">{formatBytes(plan?.totalBytes ?? 0)}</p>
          </div>
          <ul class="plan">
            {#each plan?.items ?? [] as item (item.path)}
              <li>
                <span class="dot" class:dot--optional={item.optional}></span>
                <span class="label">
                  <strong>{item.label}</strong>
                  <span class="vk-mono">{item.path}</span>
                </span>
                <span class="size">{item.bytes > 0 ? formatBytes(item.bytes) : '—'}</span>
              </li>
            {:else}
              <li class="vk-faint empty">Niente da rimuovere.</li>
            {/each}
          </ul>
        </section>
      </div>
    {/if}
  </main>

  <footer>
    <p class="status">
      {#if plan && !plan.managed && phase === 'choose'}
        Installazione non registrata: verranno tolti la cartella e le scorciatoie nei percorsi noti.
      {/if}
    </p>

    {#if phase === 'choose'}
      <button class="vk-btn" onclick={close}>Annulla</button>
      <button class="vk-btn vk-btn--danger" onclick={run} disabled={busy || !plan}>
        <Icon name="trash" size={14} />
        Disinstalla
      </button>
    {:else if phase !== 'running'}
      <button class="vk-btn vk-btn--primary" onclick={close}>Chiudi</button>
    {/if}
  </footer>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  main {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--vk-content-pad-top) var(--vk-content-pad-x) var(--vk-gap-md);
  }

  .view {
    display: flex;
    flex-direction: column;
    gap: var(--vk-gap-md);
  }

  .check {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 9px 0;
    cursor: pointer;
    font-size: var(--vk-fs-small);
  }

  .check span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .check--off {
    opacity: 0.5;
    cursor: not-allowed;
  }

  input[type='checkbox'] {
    width: 16px;
    height: 16px;
    margin-top: 2px;
    accent-color: var(--vk-cyan);
    flex: none;
  }

  .plan-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px var(--vk-gap-md) 10px;
  }

  .total {
    margin: 0;
    font-weight: 800;
  }

  .plan {
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 240px;
    overflow-y: auto;
  }

  .plan li {
    display: grid;
    grid-template-columns: 10px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 10px var(--vk-gap-md);
    border-top: 1px solid var(--vk-stroke);
    font-size: var(--vk-fs-small);
  }

  .plan .label {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .plan .label span {
    color: var(--vk-text-faint);
    word-break: break-all;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--vk-radius-pill);
    background: var(--vk-cyan);
  }

  .dot--optional {
    background: var(--vk-warning);
  }

  .size {
    font-weight: 700;
    color: var(--vk-text-secondary);
  }

  .empty {
    display: block;
    padding: var(--vk-gap-md);
  }

  .failures {
    margin: var(--vk-gap) 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--vk-warning);
    font-size: var(--vk-fs-micro);
  }

  .failures li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  footer {
    display: flex;
    align-items: center;
    gap: var(--vk-gap);
    padding: var(--vk-gap) var(--vk-content-pad-x);
    border-top: 1px solid var(--vk-stroke);
    background: var(--vk-titlebar-bg);
  }

  .status {
    flex: 1;
    margin: 0;
    min-width: 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }
</style>
