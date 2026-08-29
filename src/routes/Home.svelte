<script lang="ts">
  /**
   * Home / Play.
   *
   * Ricalca il `PlayView` del WPF: hero con gradiente prismatico e pulsante
   * PLAY da 440×118, logo a destra, poi le due card "Game stats" e
   * "MOD UPDATE" affiancate.
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import logo from '$lib/assets/logo.png';
  import { app, formatDate, formatPlayTime } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';

  let launching = $state(false);
  let installing = $state(false);
  let checking = $state(false);
  let verifying = $state(false);
  let confirmOutdated = $state(false);

  /**
   * Aggiornamento del launcher: lo stato sta nello store, perché lo guardano
   * anche l'avviso d'avvio e la finestra che scarica (§D-075).
   */
  const launcherUpdate = $derived(app.launcherUpdate);

  $effect(() => {
    void app.refreshLauncherUpdate();
  });

  const mod = $derived(app.modState);
  const stats = $derived(app.status?.stats ?? null);
  const percent = $derived(app.progress.percent ?? 0);
  const showProgress = $derived(
    app.progress.phase !== 'Idle' && app.progress.phase !== 'Completed'
  );

  const badgeTone = $derived(
    !mod?.checked
      ? ''
      : !mod.installed || mod.needsRepair
        ? 'vk-badge--danger'
        : mod.updateAvailable
          ? 'vk-badge--warning'
          : 'vk-badge--success'
  );

  const badgeText = $derived(
    !mod?.checked
      ? t('home.badge.idle')
      : !mod.installed
        ? t('home.badge.notInstalled')
        : mod.needsRepair
          ? t('home.badge.needsRepair')
          : mod.updateAvailable
            ? t('home.badge.update')
            : t('home.badge.upToDate')
  );

  async function play() {
    if (launching) return;

    // Una modpack da riparare viene fermata dal preflight con un messaggio
    // preciso: inutile chiedere prima se avviare una versione non aggiornata.
    if (mod?.installed && mod.updateAvailable && !mod.needsRepair) {
      confirmOutdated = true;
      return;
    }
    await doLaunch();
  }

  async function doLaunch() {
    confirmOutdated = false;
    launching = true;
    try {
      const blocker = await api.launchPreflight();
      if (blocker) {
        app.toast(t('home.blocked'), blocker.message, 'warning');
        app.navigate(blocker.navigateTo as never);
        return;
      }

      await api.launchGame();
      app.setStatusKey('home.launched', {}, 'success');
      app.toast(t('home.raceStarted'), t('home.raceStartedBody'), 'success');
      await app.refresh();
    } catch (error) {
      app.toast(t('home.launchFailed'), api.errorMessage(error), 'danger');
    } finally {
      launching = false;
    }
  }

  async function checkUpdates() {
    checking = true;
    try {
      await api.checkUpdates();
      await app.refresh();
    } catch (error) {
      app.toast(t('home.checkFailed'), api.errorMessage(error), 'warning');
    } finally {
      checking = false;
      void app.refreshLauncherUpdate();
    }
  }

  async function install() {
    if (installing) return;
    installing = true;
    app.resetProgress();
    try {
      const outcome = await api.installMods();
      app.toast(
        outcome.wasUpdate ? t('home.updateDone') : t('home.installDone'),
        outcome.summary,
        'success'
      );
      for (const warning of outcome.warnings) app.toast(t('common.warning'), warning, 'warning');
      await app.refresh();
    } catch (error) {
      app.toast(t('home.operationFailed'), api.errorMessage(error), 'danger');
    } finally {
      installing = false;
    }
  }

  /**
   * Verifica i file installati contro il manifest.
   *
   * È il controllo che si fa prima di riparare: dice se manca davvero
   * qualcosa. Riparare — che riscarica — resta in Mods, dove c'è anche il
   * dettaglio dei file che non tornano.
   */
  async function verify() {
    if (verifying || installing) return;
    verifying = true;
    try {
      const report = await api.verifyMods();
      const broken = report.mismatched.length > 0;
      app.toast(
        broken ? t('home.verifyBroken') : t('home.verifyDone'),
        report.message,
        broken ? 'warning' : 'success'
      );
    } catch (error) {
      app.toast(t('home.verifyFailed'), api.errorMessage(error), 'warning');
    } finally {
      verifying = false;
    }
  }
</script>

<div class="page">
  <!-- HERO -->
  <section class="vk-card vk-card--flush hero">
    <div class="hero-wash" aria-hidden="true"></div>

    <div class="hero-main">
      <h2 class="hero-title">VANZAKART</h2>

      <button class="vk-play" onclick={play} disabled={launching || installing}>
        {launching ? t('home.launching') : t('home.play')}
      </button>

      <p class="status-line" data-tone={app.statusTone}>{app.statusLine}</p>

      {#if showProgress}
        <div
          class="vk-progress progress"
          class:vk-progress--indeterminate={app.progress.percent === null}
        >
          <div class="vk-progress__fill" style="width: {percent}%"></div>
        </div>
      {/if}

      <p class="progress-line">
        <span>{app.progress.phase}</span>
        <span class="sep">/</span>
        <strong>{Math.round(percent)}%</strong>
        {#if app.progress.bytesLabel}
          <span class="vk-faint">{app.progress.bytesLabel}</span>
        {/if}
        {#if app.progress.speedLabel}
          <span class="speed">{app.progress.speedLabel}</span>
        {/if}
      </p>
    </div>

    <div class="hero-art">
      <img src={logo} alt={t('home.logoAlt')} />
    </div>
  </section>

  <!-- CARD AFFIANCATE -->
  <section class="cards">
    <div class="vk-card stats-card">
      <p class="vk-eyebrow">{t('home.stats')}</p>

      <!--
        Tre dati, tre colonne: etichetta sopra e numero sotto, come nella card
        accanto. Su una riga sola le etichette e i valori si alternavano e le
        distanze cambiavano a ogni partita giocata.
      -->
      <div class="stats">
        <div class="stat">
          <p class="vk-faint label">{t('home.lastPlayed')}</p>
          <p class="value">{formatDate(stats?.lastPlayedUtc ?? null)}</p>
        </div>
        <div class="stat">
          <p class="vk-faint label">{t('home.playTime')}</p>
          <p class="value">{formatPlayTime(stats?.totalPlayTimeMinutes ?? 0)}</p>
        </div>
        <div class="stat">
          <p class="vk-faint label">{t('home.launches')}</p>
          <p class="value">{stats?.launchCount ?? 0}</p>
        </div>
      </div>

      <div class="folder">
        <p class="vk-faint label">{t('home.modFolder')}</p>
        <p class="vk-faint path" title={mod?.modFolder ?? ''}>{mod?.modFolder || '—'}</p>
      </div>

      <button class="vk-btn" onclick={() => app.navigate('mods')}>
        <Icon name="package" size={14} />
        {t('home.openMods')}
      </button>
    </div>

    <div class="vk-card update-card">
      <div class="update-head">
        <div>
          <p class="vk-eyebrow">{t('home.modUpdate')}</p>
          <h3 class="update-title">
            {#if !mod?.checked}
              {t('home.state.checking')}
            {:else if !mod.installed}
              {t('home.state.notInstalled')}
            {:else if mod.needsRepair}
              {t('home.state.needsRepair')}
            {:else if mod.updateAvailable}
              {t('home.state.update')}
            {:else}
              {t('home.state.upToDate')}
            {/if}
          </h3>
        </div>
        <span class="vk-badge {badgeTone}">{badgeText}</span>
      </div>

      <div class="versions">
        <div>
          <p class="vk-faint label">{t('home.installedLabel')}</p>
          <p class="version">{mod?.installedVersion || t('common.none')}</p>
        </div>
        <div>
          <p class="vk-faint label">{t('home.availableLabel')}</p>
          <p class="version">{mod?.latestVersion || t('common.unknown')}</p>
        </div>
      </div>

      {#if mod?.needsRepair}
        <p class="repair">{t('home.repairNotice', { reason: mod.repairReason })}</p>
      {/if}

      <p class="check vk-muted">{mod?.checkMessage || t('home.noCheck')}</p>

      <div class="actions">
        <button
          class="vk-btn"
          onclick={checkUpdates}
          disabled={checking || installing || verifying}
        >
          <Icon name="refresh" size={14} />
          {checking ? t('home.checking') : t('home.checkUpdates')}
        </button>
        <button class="vk-btn vk-btn--primary" onclick={install} disabled={installing || verifying}>
          <Icon name="download" size={14} />
          {installing
            ? t('common.working')
            : mod?.installed
              ? t('home.updateMods')
              : t('home.installMods')}
        </button>
        <button
          class="vk-btn"
          onclick={verify}
          disabled={verifying || installing || !mod?.installed}
        >
          <Icon name="check" size={14} />
          {verifying ? t('home.verifying') : t('home.verify')}
        </button>
      </div>
    </div>
  </section>

  {#if launcherUpdate?.available}
    <section class="vk-card launcher-update">
      <div>
        <p class="vk-eyebrow">{t('home.launcherUpdate')}</p>
        <p class="vk-subtitle">
          {t('home.launcherUpdateBody', {
            latest: launcherUpdate.latest,
            current: launcherUpdate.current
          })}
          {#if launcherUpdate.changelog.filter((line) => line.trim()).length > 0}
            {launcherUpdate.changelog.filter((line) => line.trim()).join(' · ')}
          {/if}
        </p>
      </div>
      {#if launcherUpdate.downloadPage}
        <button class="vk-btn vk-btn--primary" onclick={() => (app.updaterOpen = true)}>
          <Icon name="download" size={14} />
          {t('home.launcherUpdateAction')}
        </button>
      {/if}
    </section>
  {/if}
</div>

<Modal
  open={confirmOutdated}
  title={t('home.outdatedTitle')}
  confirmLabel={t('home.outdatedConfirm')}
  cancelLabel={t('home.outdatedCancel')}
  onconfirm={doLaunch}
  oncancel={() => {
    confirmOutdated = false;
    app.navigate('mods');
  }}
>
  {t('home.outdatedBody', {
    installed: mod?.installedVersion ?? '',
    latest: mod?.latestVersion ?? ''
  })}
</Modal>

<style>
  .launcher-update {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    border-color: color-mix(in srgb, var(--vk-cyan) 35%, var(--vk-stroke));
  }

  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-bottom: 8px;
  }

  /* --- Hero --- */

  .hero {
    position: relative;
    display: grid;
    grid-template-columns: 1.18fr 0.82fr;
    min-height: 350px;
    isolation: isolate;
  }

  .hero-wash {
    position: absolute;
    inset: 0;
    background: var(--vk-hero-gradient);
    opacity: 0.18;
    z-index: -1;
  }

  .hero-main {
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 32px 34px;
    min-width: 0;
  }

  .hero-title {
    margin: 0 0 18px;
    font-size: var(--vk-fs-hero);
    font-weight: 900;
    letter-spacing: -0.02em;
    line-height: 1;
  }

  .status-line {
    margin: 24px 0 10px 2px;
    font-size: var(--vk-fs-body);
    font-weight: 600;
  }

  .status-line[data-tone='success'] {
    color: var(--vk-success);
  }
  .status-line[data-tone='warning'] {
    color: var(--vk-warning);
  }
  .status-line[data-tone='danger'] {
    color: var(--vk-danger);
  }

  .repair {
    margin: 14px 0 0;
    font-size: var(--vk-fs-small);
    color: var(--vk-danger);
  }

  .progress {
    width: min(440px, 100%);
  }

  .progress-line {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 9px 0 0 2px;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .sep {
    color: var(--vk-text-faint);
  }

  /* La velocità è l'unica cifra che si guarda mentre si aspetta: si stacca. */
  .speed {
    color: var(--vk-cyan-soft);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .hero-art {
    display: grid;
    place-items: center;
    padding: 18px 34px 18px 12px;
  }

  .hero-art img {
    width: 200px;
    height: 200px;
    object-fit: contain;
    opacity: 0.96;
    animation: float 6s ease-in-out infinite;
  }

  @keyframes float {
    0%,
    100% {
      transform: translateY(-6px);
    }
    50% {
      transform: translateY(6px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .hero-art img {
      animation: none;
    }
  }

  /* --- Card --- */

  .cards {
    display: grid;
    grid-template-columns: 1.05fr 0.95fr;
    gap: 20px;
    align-items: start;
  }

  .stats-card {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    margin-top: 14px;
  }

  .stat {
    min-width: 0;
  }

  .stat .value {
    margin: 2px 0 0;
    font-size: 15px;
    font-weight: 900;
    font-variant-numeric: tabular-nums;
  }

  .folder {
    margin-top: auto;
  }

  /* Il percorso è lungo per natura: una riga sola, e per esteso nel tooltip. */
  .folder .path {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stats-card .vk-btn {
    align-self: flex-start;
  }

  @media (max-width: 1320px) {
    .stats {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .update-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .update-title {
    margin: 5px 0 0;
    font-size: var(--vk-fs-card-title);
    font-weight: 900;
  }

  .versions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin-top: 14px;
  }

  .label {
    margin: 0;
    font-size: var(--vk-fs-eyebrow);
    font-weight: 700;
  }

  .version {
    margin: 2px 0 0;
    font-size: 15px;
    font-weight: 900;
  }

  .check {
    margin: 12px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 14px;
  }

  @media (max-width: 1100px) {
    .hero {
      grid-template-columns: 1fr;
    }
    .hero-art {
      display: none;
    }
    .cards {
      grid-template-columns: 1fr;
    }
  }
</style>
