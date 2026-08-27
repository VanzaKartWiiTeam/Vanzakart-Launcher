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
  import LauncherUpdate from '$lib/components/LauncherUpdate.svelte';
  import logo from '$lib/assets/logo.png';
  import { app, formatDate, formatPlayTime } from '$lib/stores/app.svelte';
  import type { LauncherUpdateStatus } from '$lib/api/types';

  let launching = $state(false);
  let installing = $state(false);
  let checking = $state(false);
  let confirmOutdated = $state(false);

  /** Aggiornamento del launcher, distinto da quello della modpack. */
  let launcherUpdate = $state<LauncherUpdateStatus | null>(null);
  let updaterOpen = $state(false);

  $effect(() => {
    void refreshLauncherUpdate();
  });

  async function refreshLauncherUpdate() {
    try {
      launcherUpdate = await api.getLauncherUpdateStatus();
    } catch {
      // Non è un errore da mostrare: la card semplicemente non compare.
    }
  }

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
      ? 'Idle'
      : !mod.installed
        ? 'Non installata'
        : mod.needsRepair
          ? 'Da riparare'
          : mod.updateAvailable
            ? 'Aggiornamento'
            : 'Aggiornata'
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
        app.toast('Non si può ancora partire', blocker.message, 'warning');
        app.navigate(blocker.navigateTo as never);
        return;
      }

      await api.launchGame();
      app.setStatusLine('Gioco avviato. Buona gara.', 'success');
      app.toast('Gara iniziata', 'VanzaKart è in avvio.', 'success');
      await app.refresh();
    } catch (error) {
      app.toast('Avvio non riuscito', api.errorMessage(error), 'danger');
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
      app.toast('Controllo non riuscito', api.errorMessage(error), 'warning');
    } finally {
      checking = false;
      void refreshLauncherUpdate();
    }
  }

  async function install() {
    if (installing) return;
    installing = true;
    app.resetProgress();
    try {
      const outcome = await api.installMods();
      app.toast(
        outcome.wasUpdate ? 'Aggiornamento completato' : 'Installazione completata',
        outcome.summary,
        'success'
      );
      for (const warning of outcome.warnings) app.toast('Avviso', warning, 'warning');
      await app.refresh();
    } catch (error) {
      app.toast('Operazione non riuscita', api.errorMessage(error), 'danger');
    } finally {
      installing = false;
    }
  }

  async function repair() {
    if (installing) return;
    installing = true;
    app.resetProgress();
    try {
      const outcome = await api.repairMods();
      app.toast('Riparazione completata', outcome.summary, 'success');
      await app.refresh();
    } catch (error) {
      app.toast('Riparazione non riuscita', api.errorMessage(error), 'danger');
    } finally {
      installing = false;
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
        {launching ? 'AVVIO…' : 'PLAY'}
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
      <img src={logo} alt="Logo VanzaKart" />
    </div>
  </section>

  <!-- CARD AFFIANCATE -->
  <section class="cards">
    <div class="vk-card stats-card">
      <p class="vk-eyebrow">Game stats</p>
      <div class="stats">
        <span class="vk-faint">Ultima partita</span>
        <strong>{formatDate(stats?.lastPlayedUtc ?? null)}</strong>
        <span class="vk-faint">Tempo</span>
        <strong>{formatPlayTime(stats?.totalPlayTimeMinutes ?? 0)}</strong>
        <span class="vk-faint">Avvii</span>
        <strong>{stats?.launchCount ?? 0}</strong>
      </div>
      <p class="folder vk-faint">{mod?.modFolder ?? ''}</p>
      <button class="vk-btn" onclick={() => app.navigate('mods')}>Mods</button>
    </div>

    <div class="vk-card update-card">
      <div class="update-head">
        <div>
          <p class="vk-eyebrow">Mod update</p>
          <h3 class="update-title">
            {#if !mod?.checked}
              Controllo dello stato locale
            {:else if !mod.installed}
              Modpack non installata
            {:else if mod.needsRepair}
              Modpack da riparare
            {:else if mod.updateAvailable}
              Aggiornamento disponibile
            {:else}
              Tutto aggiornato
            {/if}
          </h3>
        </div>
        <span class="vk-badge {badgeTone}">{badgeText}</span>
      </div>

      <div class="versions">
        <div>
          <p class="vk-faint label">Installata</p>
          <p class="version">{mod?.installedVersion || 'Nessuna'}</p>
        </div>
        <div>
          <p class="vk-faint label">Disponibile</p>
          <p class="version">{mod?.latestVersion || 'Sconosciuta'}</p>
        </div>
      </div>

      {#if mod?.needsRepair}
        <p class="repair">
          I file della modpack non sono utilizzabili: {mod.repairReason}. Premi
          <strong>Aggiorna mod</strong> per riscaricarli: finché non lo fai, Dolphin avvia Mario Kart
          Wii originale.
        </p>
      {/if}

      <p class="check vk-muted">{mod?.checkMessage || 'Nessun controllo eseguito.'}</p>

      <div class="actions">
        <button class="vk-btn" onclick={checkUpdates} disabled={checking || installing}>
          <Icon name="refresh" size={14} />
          {checking ? 'Controllo…' : 'Controlla aggiornamenti'}
        </button>
        <button class="vk-btn vk-btn--primary" onclick={install} disabled={installing}>
          <Icon name="download" size={14} />
          {installing ? 'In corso…' : mod?.installed ? 'Aggiorna mod' : 'Installa mod'}
        </button>
        <button class="vk-btn" onclick={repair} disabled={installing || !mod?.installed}>
          <Icon name="repair" size={14} />
          Ripara
        </button>
      </div>
    </div>
  </section>

  {#if launcherUpdate?.available}
    <section class="vk-card launcher-update">
      <div>
        <p class="vk-eyebrow">Aggiornamento del launcher</p>
        <p class="vk-subtitle">
          È disponibile la versione <strong>{launcherUpdate.latest}</strong>; questa è la
          {launcherUpdate.current}.
          {#if launcherUpdate.changelog.filter((line) => line.trim()).length > 0}
            {launcherUpdate.changelog.filter((line) => line.trim()).join(' · ')}
          {/if}
        </p>
      </div>
      {#if launcherUpdate.downloadPage}
        <button class="vk-btn vk-btn--primary" onclick={() => (updaterOpen = true)}>
          <Icon name="download" size={14} />
          Aggiorna il launcher
        </button>
      {/if}
    </section>
  {/if}
</div>

{#if updaterOpen && launcherUpdate}
  <LauncherUpdate status={launcherUpdate} onclose={() => (updaterOpen = false)} />
{/if}

<Modal
  open={confirmOutdated}
  title="Aggiornamento disponibile"
  confirmLabel="Avvia comunque"
  cancelLabel="Vai a Mods"
  onconfirm={doLaunch}
  oncancel={() => {
    confirmOutdated = false;
    app.navigate('mods');
  }}
>
  La modpack installata ({mod?.installedVersion}) non è l'ultima disponibile ({mod?.latestVersion}).
  Vuoi avviare comunque?
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
    gap: 10px;
  }

  .stats {
    display: grid;
    grid-template-columns: auto auto auto auto auto auto;
    gap: 6px 10px;
    align-items: baseline;
    font-size: var(--vk-fs-small);
  }

  .stats strong {
    font-weight: 800;
  }

  .folder {
    margin: 0;
    font-size: var(--vk-fs-eyebrow);
    overflow-wrap: anywhere;
  }

  .stats-card .vk-btn {
    align-self: flex-start;
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
