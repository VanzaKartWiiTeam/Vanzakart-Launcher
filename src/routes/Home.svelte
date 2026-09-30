<script lang="ts">
  /**
   * Home / Play.
   *
   * Ricalca il `PlayView` del WPF: hero con gradiente e pulsante PLAY da
   * 440×118, logo a destra. Sotto PLAY una frase sola dice come sta la
   * modpack e, **solo quando serve**, un pulsante fa la cosa da fare:
   * installare, aggiornare, riparare, finire le impostazioni (§D-100).
   *
   * Il resto della modpack — versioni, changelog, verifica, cartella — sta in
   * Mods; l'aggiornamento del launcher nella barra del titolo e nell'avviso
   * all'avvio. Qui ripeterli era rumore.
   */
  import * as api from '$lib/api';
  import Icon, { type IconName } from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import logo from '$lib/assets/logo.png';
  import { app, formatDate, formatPlayTime } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import { operationLabel, operations, phaseLabel } from '$lib/stores/operations.svelte';
  import { formatRemaining } from '$lib/stores/transfer';

  let launching = $state(false);
  let confirmOutdated = $state(false);

  $effect(() => {
    void app.refreshLauncherUpdate();
  });

  const mod = $derived(app.modState);
  const stats = $derived(app.status?.stats ?? null);

  /**
   * L'operazione in corso, qualunque sia. La barra dell'eroe diceva solo
   * "Download 42%" anche quando a scaricare era il music pack: adesso dice
   * cosa (§D-086).
   */
  const active = $derived(operations.active);
  const activeProgress = $derived(active ? operations.progressOf(active) : null);
  const percent = $derived(activeProgress?.percent ?? 0);
  const remaining = $derived(formatRemaining(operations.remaining));
  const installing = $derived(operations.busyWith('mods'));

  /** Con l'opzione attiva e Dolphin aperto, l'avvio comincia chiudendolo. */
  const willCloseDolphin = $derived(
    Boolean(app.status?.dolphinRunning && app.settings?.closeRunningDolphin)
  );

  interface HomeAction {
    label: string;
    icon: IconName;
    hint?: string;
    run: () => void;
  }

  /** La cosa da fare prima di giocare, se ce n'è una. */
  const action = $derived.by((): HomeAction | null => {
    const status = app.status;
    if (!status || !mod) return null;
    if (!status.settingsComplete) {
      return {
        label: t('home.action.settings'),
        icon: 'settings',
        run: () => app.navigate('settings')
      };
    }
    if (!mod.installed) {
      return { label: t('home.installMods'), icon: 'download', run: () => void install() };
    }
    if (mod.needsRepair) {
      return {
        label: t('home.action.repair'),
        icon: 'repair',
        hint: mod.repairReason || t('home.action.repairHint'),
        run: () => void install('repair')
      };
    }
    if (mod.updateAvailable) {
      return {
        label: t('home.action.update', { version: mod.latestVersion }),
        icon: 'download',
        run: () => void install()
      };
    }
    return null;
  });

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

      const result = await api.launchGame();
      app.setStatusKey('home.launched', {}, 'success');
      app.toast(
        t('home.raceStarted'),
        result.closedPrevious ? t('home.dolphinRestarted') : t('home.raceStartedBody'),
        'success'
      );
      await app.refresh();
    } catch (error) {
      app.toast(t('home.launchFailed'), api.errorMessage(error), 'danger');
    } finally {
      launching = false;
    }
  }

  /** Installa, aggiorna o ripara la modpack: la barra la mostra l'eroe. */
  async function install(kind: 'install' | 'repair' = 'install') {
    if (operations.busy) return;
    try {
      const outcome = await operations.run(
        'mods',
        () => (kind === 'repair' ? api.repairMods() : api.installMods()),
        {
          title: 'VanzaKart Modpack',
          describe: (result) => result.summary
        }
      );
      app.toast(
        outcome.wasUpdate ? t('home.updateDone') : t('home.installDone'),
        outcome.summary,
        'success'
      );
      for (const warning of outcome.warnings) app.toast(t('common.warning'), warning, 'warning');
      await app.refresh();
    } catch (error) {
      if (api.errorCode(error) !== 'cancelled') {
        app.toast(t('home.operationFailed'), api.errorMessage(error), 'danger');
      }
    }
  }

  /** Dal dialogo "versione vecchia": prima si aggiorna, poi si gioca. */
  function updateFirst() {
    confirmOutdated = false;
    void install();
  }
</script>

<div class="page">
  <section class="vk-card vk-card--flush hero">
    <div class="hero-wash" aria-hidden="true"></div>

    <div class="hero-main">
      <h2 class="hero-title">VANZAKART</h2>

      <button class="vk-play" onclick={play} disabled={launching || installing}>
        {launching
          ? willCloseDolphin
            ? t('home.closingDolphin')
            : t('home.launching')
          : t('home.play')}
      </button>

      {#if active}
        <div
          class="vk-progress progress"
          class:vk-progress--indeterminate={(activeProgress?.percent ?? null) === null}
        >
          <div class="vk-progress__fill" style="width: {percent}%"></div>
        </div>

        <p class="progress-line">
          <strong class="what">{operationLabel(active)}</strong>
          <span class="sep">·</span>
          <span>{activeProgress ? phaseLabel(activeProgress.phase) : t('ops.starting')}</span>
          <span class="sep">/</span>
          <strong>{Math.round(percent)}%</strong>
          {#if activeProgress?.bytesLabel}
            <span class="vk-faint">{activeProgress.bytesLabel}</span>
          {/if}
          {#if activeProgress?.speedLabel}
            <span class="speed">{activeProgress.speedLabel}</span>
          {/if}
          {#if remaining}
            <span class="speed">{t('ops.remaining', { time: remaining })}</span>
          {/if}
        </p>
      {:else}
        <div class="state">
          <p class="status-line" data-tone={app.statusTone}>
            <span class="status-dot" aria-hidden="true"></span>
            {app.statusLine}
          </p>
          {#if action}
            <button
              class="vk-btn action"
              onclick={action.run}
              disabled={operations.busy || launching}
              {@attach tooltip(action.hint)}
            >
              <Icon name={action.icon} size={14} />
              {action.label}
            </button>
          {/if}
        </div>
        {#if willCloseDolphin}
          <p class="progress-line vk-faint">{t('home.dolphinWillClose')}</p>
        {/if}
      {/if}
    </div>

    <div class="hero-art">
      <img src={logo} alt={t('home.logoAlt')} />
    </div>
  </section>

  <!--
    Tre dati, tre colonne: etichetta sopra e numero sotto. Su una riga sola
    le etichette e i valori si alternavano e le distanze cambiavano a ogni
    partita giocata.
  -->
  <section class="vk-card stats">
    <div class="stat">
      <p class="label">{t('home.lastPlayed')}</p>
      <p class="value">{formatDate(stats?.lastPlayedUtc ?? null)}</p>
    </div>
    <div class="stat">
      <p class="label">{t('home.playTime')}</p>
      <p class="value">{formatPlayTime(stats?.totalPlayTimeMinutes ?? 0)}</p>
    </div>
    <div class="stat">
      <p class="label">{t('home.launches')}</p>
      <p class="value">{stats?.launchCount ?? 0}</p>
    </div>
  </section>
</div>

<Modal
  open={confirmOutdated}
  title={t('home.outdatedTitle')}
  confirmLabel={t('home.outdatedConfirm')}
  cancelLabel={t('home.outdatedCancel')}
  onconfirm={doLaunch}
  oncancel={updateFirst}
>
  {t('home.outdatedBody', {
    installed: mod?.installedVersion ?? '',
    latest: mod?.latestVersion ?? ''
  })}
</Modal>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 100%;
    padding-bottom: 8px;
  }

  /* --- Hero --- */

  /* L'eroe prende l'altezza che avanza: a finestra grande niente vuoto sotto
     le statistiche (§D-108). */
  .hero {
    position: relative;
    display: grid;
    grid-template-columns: 1.18fr 0.82fr;
    flex: 1;
    min-height: 380px;
    isolation: isolate;
  }

  /*
   * Un velo dell'arcobaleno intero, in diagonale e appena accennato. Il WPF
   * usava rosa → ciano → viola: proprio i due colori che il tema non vuole
   * da soli (§D-099).
   */
  .hero-wash {
    position: absolute;
    inset: 0;
    z-index: -1;
    background:
      radial-gradient(ellipse at 20% 0%, transparent 30%, var(--vk-panel) 85%),
      linear-gradient(
        120deg,
        #ff0066 0%,
        #ff8800 18%,
        #ffea00 34%,
        #00ff66 50%,
        #00f2ff 67%,
        #3300ff 84%,
        #b000ff 100%
      );
    opacity: 0.16;
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

  .state {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
    margin: 22px 0 0 2px;
  }

  .status-line {
    display: flex;
    align-items: center;
    gap: 9px;
    margin: 0;
    font-size: var(--vk-fs-body);
    font-weight: 600;
    --tone: var(--vk-text-secondary);
  }

  .status-line[data-tone='success'] {
    --tone: var(--vk-success);
  }
  .status-line[data-tone='warning'] {
    --tone: var(--vk-warning);
  }
  .status-line[data-tone='danger'] {
    --tone: var(--vk-danger);
  }

  .status-dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--tone);
    box-shadow: 0 0 10px var(--tone);
  }

  .action {
    padding: 8px 14px;
    font-size: var(--vk-fs-small);
  }

  .progress {
    width: min(440px, 100%);
    margin-top: 22px;
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

  .what {
    color: var(--vk-text);
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

  /* --- Statistiche --- */

  .stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    padding: 16px 0;
  }

  .stat {
    min-width: 0;
    padding: 0 26px;
  }

  .stat + .stat {
    border-left: 1px solid var(--vk-stroke);
  }

  .label {
    margin: 0;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .value {
    margin: 4px 0 0;
    font-size: 18px;
    font-weight: 900;
    font-variant-numeric: tabular-nums;
  }

  @media (max-width: 1100px) {
    .hero {
      grid-template-columns: 1fr;
    }
    .hero-art {
      display: none;
    }
  }
</style>
