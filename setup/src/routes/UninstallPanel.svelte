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
  import { t } from '$setup/lib/i18n/store.svelte';

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
      const confirmed = await confirm(t('uninstall.confirm.body'), {
        title: t('uninstall.confirm.title'),
        kind: 'warning'
      });
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
      <StepProgress title={t('progress.removing')} {progress} {log} />
    {:else if phase === 'done' && report}
      <div class="vk-view-enter view">
        <header>
          <p class="vk-eyebrow">{t('done.eyebrow')}</p>
          <h1 class="vk-title">{t('uninstall.done.title')}</h1>
        </header>

        <section class="vk-card">
          <p class="vk-muted">
            {t('uninstall.done.summary', {
              count: report.removed.length,
              size: formatBytes(report.bytesFreed)
            })}
          </p>
          {#if report.deferred}
            <p class="vk-faint">{t('uninstall.done.deferred')}</p>
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

        <p class="vk-faint">{t('uninstall.done.thanks')}</p>
      </div>
    {:else if phase === 'missing'}
      <div class="vk-empty">
        <Icon name="package" size={28} />
        <p>{t('uninstall.missing')}</p>
        <p class="vk-faint">{planError}</p>
      </div>
    {:else}
      <div class="vk-view-enter view">
        <header>
          <p class="vk-eyebrow">{t('uninstall.eyebrow')}</p>
          <h1 class="vk-title">{t('uninstall.title')}</h1>
          {#if plan}
            <p class="vk-subtitle">
              {#if plan.version}{t('uninstall.version', { version: plan.version })}{/if}
              <span class="vk-mono">{plan.installDir}</span>
            </p>
          {/if}
        </header>

        {#if planError}
          <p class="vk-error">{planError}</p>
        {/if}

        <section class="vk-card">
          <p class="vk-eyebrow">{t('uninstall.what')}</p>

          <label class="check">
            <input type="checkbox" bind:checked={options.removeCacheAndLogs} />
            <span>
              <strong>{t('uninstall.cache')}</strong>
              <span class="vk-faint">{t('uninstall.cache.note')}</span>
            </span>
          </label>

          <label class="check">
            <input type="checkbox" bind:checked={options.removeLauncherData} />
            <span>
              <strong>{t('uninstall.data')}</strong>
              <span class="vk-faint">{t('uninstall.data.note')}</span>
            </span>
          </label>

          <label class="check" class:check--off={!plan?.hasModpacks}>
            <input
              type="checkbox"
              bind:checked={options.removeModpacks}
              disabled={!plan?.hasModpacks}
            />
            <span>
              <strong>{t('uninstall.modpacks')}</strong>
              <span class="vk-faint">
                {plan?.hasModpacks ? t('uninstall.modpacks.note') : t('uninstall.modpacks.none')}
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
              <strong>{t('uninstall.userData')}</strong>
              <span class="vk-faint">
                {t('uninstall.userData.before')}
                <span class="vk-mono">*_UserData</span>{t('uninstall.userData.after')}
              </span>
            </span>
          </label>
        </section>

        <section class="vk-card vk-card--flush">
          <div class="plan-head">
            <p class="vk-eyebrow">{t('uninstall.willRemove')}</p>
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
                <span class="size">
                  {item.bytes > 0 ? formatBytes(item.bytes) : t('common.dash')}
                </span>
              </li>
            {:else}
              <li class="vk-faint empty">{t('uninstall.nothing')}</li>
            {/each}
          </ul>
        </section>
      </div>
    {/if}
  </main>

  <footer>
    <p class="status">
      {#if plan && !plan.managed && phase === 'choose'}
        {t('uninstall.unmanaged')}
      {/if}
    </p>

    {#if phase === 'choose'}
      <button class="vk-btn" onclick={close}>{t('common.cancel')}</button>
      <button class="vk-btn vk-btn--danger" onclick={run} disabled={busy || !plan}>
        <Icon name="trash" size={14} />
        {t('uninstall.run')}
      </button>
    {:else if phase !== 'running'}
      <button class="vk-btn vk-btn--primary" onclick={close}>{t('common.close')}</button>
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
