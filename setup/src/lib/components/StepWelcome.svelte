<script lang="ts">
  /** Primo passo: chi sei, cosa sto per installare, cosa c'è già. */
  import Icon from '$lib/components/Icon.svelte';
  import type { Bootstrap } from '$setup/lib/api';
  import { formatBytes, formatDate } from '$setup/lib/format';
  import { t } from '$setup/lib/i18n/store.svelte';

  let {
    boot,
    busy,
    onRetry,
    onOpenDownloadPage
  }: {
    boot: Bootstrap;
    busy: boolean;
    onRetry: () => void;
    onOpenDownloadPage: () => void;
  } = $props();
</script>

<div class="vk-view-enter view">
  <header>
    <p class="vk-eyebrow">{t('welcome.eyebrow')}</p>
    <h1 class="vk-title">VanzaKart Launcher</h1>
    <p class="vk-subtitle">{t('welcome.subtitle', { platform: boot.platform })}</p>
  </header>

  {#if boot.release}
    <section class="vk-card vk-rainbow-top">
      <div class="release">
        <div>
          <p class="vk-eyebrow">{t('welcome.releaseTitle')}</p>
          <p class="version">{boot.release.version}</p>
          {#if boot.release.pubDate}
            <p class="vk-faint">
              {t('welcome.published', { date: formatDate(boot.release.pubDate) })}
            </p>
          {/if}
        </div>
        <dl class="facts">
          <div>
            <dt>{t('welcome.package')}</dt>
            <dd>{boot.release.packageKey}</dd>
          </div>
          <div>
            <dt>{t('welcome.size')}</dt>
            <dd>
              {boot.release.sizeBytes > 0
                ? formatBytes(boot.release.sizeBytes)
                : t('welcome.sizeUnknown')}
            </dd>
          </div>
          <div>
            <dt>{t('welcome.verify')}</dt>
            <dd class={boot.release.verifiable ? 'ok' : 'warn'}>
              {boot.release.verifiable ? t('welcome.verify.sha') : t('welcome.verify.none')}
            </dd>
          </div>
        </dl>
      </div>

      {#if boot.release.notes}
        <p class="notes">{boot.release.notes}</p>
      {/if}
    </section>
  {:else}
    <section class="vk-error">
      <p class="strong">{t('welcome.releaseFailed')}</p>
      <p class="reason">{boot.releaseError ?? t('welcome.unknownCause')}</p>
      <div class="vk-row actions">
        <button class="vk-btn" onclick={onRetry} disabled={busy}>
          <Icon name="refresh" size={14} />
          {t('common.retry')}
        </button>
        <button class="vk-btn" onclick={onOpenDownloadPage}>
          <Icon name="external" size={14} />
          {t('welcome.openDownloads')}
        </button>
      </div>
    </section>
  {/if}

  {#if boot.existing}
    <section class="vk-card existing">
      <div class="vk-row">
        <Icon name="package" size={18} />
        <p class="strong">{t('welcome.existing')}</p>
      </div>
      <p class="vk-muted">
        {#if boot.existing.version}
          {t('welcome.existing.version', { version: boot.existing.version })}
        {:else}
          {t('welcome.existing.unknownVersion')}
        {/if}
        · {formatBytes(boot.existing.bytes)}
      </p>
      <p class="vk-mono path">{boot.existing.installDir}</p>
      <p class="vk-faint">
        {#if boot.existing.managed}
          {t('welcome.existing.managed')}
        {:else}
          {t('welcome.existing.foreign')}
        {/if}
      </p>
    </section>
  {/if}

  {#if boot.legacyInstallDir}
    <section class="vk-card legacy">
      <div class="vk-row">
        <Icon name="package" size={16} />
        <p class="strong">{t('welcome.legacy')}</p>
      </div>
      <p class="vk-mono path">{boot.legacyInstallDir}</p>
      <p class="vk-faint">{t('welcome.legacy.note')}</p>
    </section>
  {/if}

  <p class="vk-faint footnote">{t('welcome.footnote')}</p>
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    gap: var(--vk-gap-md);
  }

  .release {
    display: flex;
    flex-wrap: wrap;
    gap: var(--vk-gap-lg);
    align-items: flex-start;
    justify-content: space-between;
  }

  .version {
    font-size: var(--vk-fs-section);
    font-weight: 900;
    margin: 4px 0 2px;
  }

  .facts {
    display: flex;
    gap: var(--vk-gap-lg);
    margin: 0;
  }

  .facts div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  dt {
    font-size: var(--vk-fs-eyebrow);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--vk-text-secondary);
    font-weight: 700;
  }

  dd {
    margin: 0;
    font-size: var(--vk-fs-small);
    font-weight: 700;
  }

  dd.ok {
    color: var(--vk-success);
  }

  dd.warn {
    color: var(--vk-warning);
  }

  .notes {
    margin: var(--vk-gap-md) 0 0;
    padding-top: var(--vk-gap);
    border-top: 1px solid var(--vk-stroke);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-small);
    white-space: pre-wrap;
  }

  .existing,
  .legacy {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .strong {
    margin: 0;
    font-weight: 800;
  }

  .path {
    margin: 0;
    color: var(--vk-text-secondary);
    word-break: break-all;
    user-select: text;
  }

  .reason {
    margin: 6px 0 0;
    font-size: var(--vk-fs-small);
    color: var(--vk-text-secondary);
    user-select: text;
  }

  .actions {
    margin-top: var(--vk-gap);
  }

  .footnote {
    margin: 0;
    font-size: var(--vk-fs-micro);
  }
</style>
