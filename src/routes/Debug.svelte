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
  import Switch from '$lib/components/Switch.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { effects } from '$lib/stores/effects.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import type { BackupSummary, DiagnosticEntry } from '$lib/api/types';

  let entries = $state<DiagnosticEntry[]>([]);
  let log = $state('');
  let backups = $state<BackupSummary[]>([]);
  let loading = $state(true);
  let purgeOpen = $state(false);
  let confirmation = $state('');
  let purging = $state(false);

  /*
   * Contatore di fotogrammi.
   *
   * Serve a rispondere a una domanda sola: la finestra sta disegnando alla
   * velocità dello schermo, o no? Gira solo finché questa pagina è aperta —
   * un `requestAnimationFrame` perenne sarebbe esso stesso un costo — e la
   * media si aggiorna una volta al secondo (§D-082).
   */
  let fps = $state(0);
  const pixelRatio = typeof window === 'undefined' ? 1 : window.devicePixelRatio;

  $effect(() => {
    let frames = 0;
    let since = performance.now();
    let handle = requestAnimationFrame(function tick(now: number) {
      frames += 1;
      const elapsed = now - since;
      if (elapsed >= 1000) {
        fps = Math.round((frames * 1000) / elapsed);
        frames = 0;
        since = now;
      }
      handle = requestAnimationFrame(tick);
    });

    return () => cancelAnimationFrame(handle);
  });

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
      app.toast(t('debug.unavailable'), api.errorMessage(error), 'warning');
    } finally {
      loading = false;
    }
  }

  async function copyReport() {
    const report = entries.map((entry) => `${entry.label}: ${entry.value}`).join('\n');
    try {
      await navigator.clipboard.writeText(report);
      app.toast(t('debug.copied'), t('debug.copiedBody'), 'success');
    } catch {
      app.toast(t('debug.copyFailed'), t('debug.copyFailedBody'), 'warning');
    }
  }

  async function purge() {
    purging = true;
    try {
      const removed = await api.purgeUserData(confirmation);
      app.toast(t('debug.purged'), t('debug.purgedBody', { count: removed.length }), 'success');
      purgeOpen = false;
      confirmation = '';
      await load();
    } catch (error) {
      app.toast(t('debug.purgeFailed'), api.errorMessage(error), 'warning');
    } finally {
      purging = false;
    }
  }
</script>

<div class="page">
  <section class="vk-card">
    <div class="head">
      <p class="vk-eyebrow">{t('debug.installState')}</p>
      <div class="vk-row">
        <button class="vk-btn" onclick={load} disabled={loading}>
          <Icon name="refresh" size={14} />
          {t('common.refreshAction')}
        </button>
        <button class="vk-btn" onclick={copyReport}>{t('debug.copyReport')}</button>
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
      <div>
        <p class="vk-eyebrow">{t('debug.performance')}</p>
        <p class="vk-subtitle">{t('debug.performanceBody')}</p>
      </div>
      <p class="fps" class:fps--low={fps > 0 && fps < 45}>
        {t('debug.fps', { fps })}
      </p>
    </div>

    <dl class="entries">
      <div class="entry">
        <dt>{t('debug.pixelRatio')}</dt>
        <dd><span class="value">{pixelRatio}×</span></dd>
      </div>
    </dl>

    <label class="effects">
      <span>
        <strong>{t('debug.reducedEffects')}</strong>
        <span class="vk-faint">{t('debug.reducedEffectsBody')}</span>
      </span>
      <Switch
        checked={effects.reduced}
        label={t('debug.reducedEffects')}
        onchange={(next) => effects.set(next)}
      />
    </label>
  </section>

  <section class="vk-card">
    <div class="head">
      <p class="vk-eyebrow">{t('debug.log')}</p>
      <button class="vk-btn" onclick={() => api.openFolder('logs')}>
        <Icon name="folder" size={14} />
        {t('settings.openLogs')}
      </button>
    </div>
    <pre class="log vk-mono">{log}</pre>
  </section>

  <section class="vk-card">
    <div class="head">
      <p class="vk-eyebrow">{t('debug.backups')}</p>
      <button class="vk-btn" onclick={() => api.openFolder('backups')}>
        <Icon name="folder" size={14} />
        {t('debug.openFolder')}
      </button>
    </div>

    {#if backups.length === 0}
      <p class="vk-faint">{t('debug.noBackups')}</p>
    {:else}
      <ul class="backups">
        {#each backups as backup (backup.id)}
          <li>
            <span class="vk-mono">{backup.id}</span>
            <span class="vk-faint">{t('debug.protectedFiles', { count: backup.fileCount })}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="vk-card danger-zone">
    <div>
      <p class="vk-eyebrow">{t('debug.dangerZone')}</p>
      <p class="vk-subtitle">{t('debug.dangerBody')}</p>
    </div>
    <button class="vk-btn vk-btn--danger" onclick={() => (purgeOpen = true)}>
      <Icon name="warning" size={14} />
      {t('debug.purgeAction')}
    </button>
  </section>
</div>

<Modal
  open={purgeOpen}
  title={t('debug.purgeTitle')}
  confirmLabel={t('debug.purgeConfirm')}
  danger
  busy={purging}
  onconfirm={purge}
  oncancel={() => {
    purgeOpen = false;
    confirmation = '';
  }}
>
  <p>{t('debug.purgeBody')}</p>
  <p>{t('debug.purgeType')}</p>
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

  .fps {
    margin: 0;
    font-family: var(--vk-font-mono);
    font-size: var(--vk-fs-card-title);
    font-weight: 800;
    color: var(--vk-success);
    white-space: nowrap;
  }

  /* Sotto i 45 fps la finestra non sta al passo con lo schermo. */
  .fps--low {
    color: var(--vk-warning);
  }

  .effects {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-top: var(--vk-gap-md);
    padding-top: var(--vk-gap);
    border-top: 1px solid var(--vk-stroke);
    font-size: var(--vk-fs-small);
    cursor: pointer;
  }

  .effects span {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
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
