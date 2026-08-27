<script lang="ts">
  /**
   * La procedura guidata, con gli stessi passi del setup legacy in WPF:
   * benvenuto, cartella, verifiche, download e installazione, fine.
   *
   * Lo stato sta tutto qui; i passi sono componenti che leggono e scrivono
   * `options`, che è un oggetto reattivo.
   */
  import { onDestroy, untrack } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import Icon from '$lib/components/Icon.svelte';
  import StepRail from '$setup/lib/components/StepRail.svelte';
  import StepWelcome from '$setup/lib/components/StepWelcome.svelte';
  import StepFolder from '$setup/lib/components/StepFolder.svelte';
  import StepChecks from '$setup/lib/components/StepChecks.svelte';
  import StepProgress from '$setup/lib/components/StepProgress.svelte';
  import StepDone from '$setup/lib/components/StepDone.svelte';
  import * as api from '$setup/lib/api';
  import type {
    Bootstrap,
    InstallOptionsInput,
    InstallReport,
    Preflight,
    ProgressEvent
  } from '$setup/lib/api';

  let { boot, onBusyChange }: { boot: Bootstrap; onBusyChange: (busy: boolean) => void } = $props();

  type StepKey = 'welcome' | 'folder' | 'checks' | 'running' | 'done';

  const STEPS: { key: StepKey; label: string; hint: string }[] = [
    { key: 'welcome', label: 'Benvenuto', hint: 'Cosa verrà installato' },
    { key: 'folder', label: 'Cartella', hint: 'Dove e con quali scorciatoie' },
    { key: 'checks', label: 'Verifiche', hint: 'Spazio, permessi, launcher' },
    { key: 'running', label: 'Installazione', hint: 'Download ed estrazione' },
    { key: 'done', label: 'Fine', hint: 'Riepilogo e avvio' }
  ];

  let step = $state<StepKey>('welcome');
  let busy = $state(false);
  let footerMessage = $state('');
  let footerTone = $state<'info' | 'danger'>('info');

  let preflight = $state<Preflight | null>(null);
  let checking = $state(false);
  let checkError = $state('');

  let progress = $state<ProgressEvent | null>(null);
  let log = $state<string[]>([]);
  let report = $state<InstallReport | null>(null);
  let launchAfter = $state(true);
  let release = $state(untrack(() => boot.release));
  let releaseError = $state(untrack(() => boot.releaseError));

  // I valori iniziali si leggono una volta sola: `boot` arriva dal backend
  // all'avvio e non cambia più, ma senza `untrack` Svelte segnalerebbe una
  // lettura reattiva usata come costante.
  const options = $state<InstallOptionsInput>(
    untrack(() => ({
      installDir: boot.defaultInstallDir,
      mode: boot.existing ? ('update' as const) : ('fresh' as const),
      backupData: Boolean(boot.existing),
      backupDir: boot.defaultBackupDir,
      desktopShortcut: true,
      startMenuShortcut: true,
      quickLaunchShortcut: false,
      uninstallEntry: true,
      pathSymlink: boot.supportsPathSymlink
    }))
  );

  const stepIndex = $derived(STEPS.findIndex((entry) => entry.key === step));
  const canGoBack = $derived(step === 'folder' || step === 'checks');
  const nextLabel = $derived(step === 'checks' ? 'Installa' : step === 'done' ? 'Fine' : 'Avanti');
  const nextEnabled = $derived.by(() => {
    if (busy) return false;
    if (step === 'welcome') return Boolean(release);
    if (step === 'folder') return options.installDir.trim().length > 0;
    if (step === 'checks')
      return Boolean(preflight?.enoughSpace && preflight?.writable && !preflight?.launcherRunning);
    return true;
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

  function setBusy(value: boolean) {
    busy = value;
    onBusyChange(value);
  }

  function fail(error: unknown) {
    footerMessage = api.errorMessage(error);
    footerTone = 'danger';
  }

  async function retryRelease() {
    setBusy(true);
    footerMessage = 'Rilettura dal server…';
    footerTone = 'info';
    try {
      release = await api.refreshRelease();
      releaseError = null;
      footerMessage = `Versione disponibile: ${release.version}.`;
    } catch (error) {
      releaseError = api.errorMessage(error);
      fail(error);
    } finally {
      setBusy(false);
    }
  }

  async function browse(current: string, title: string): Promise<string | null> {
    const chosen = await open({ directory: true, multiple: false, defaultPath: current, title });
    return typeof chosen === 'string' ? chosen : null;
  }

  async function browseInstall() {
    const chosen = await browse(options.installDir, "Scegli la cartella d'installazione");
    if (chosen) options.installDir = chosen;
  }

  async function browseBackup() {
    const chosen = await browse(options.backupDir, 'Scegli la cartella del backup');
    if (chosen) options.backupDir = chosen;
  }

  async function runChecks() {
    checking = true;
    checkError = '';
    try {
      preflight = await api.preflight(options.installDir);
      footerMessage = preflight.enoughSpace ? 'Verifiche completate.' : 'Spazio insufficiente.';
      footerTone = preflight.enoughSpace ? 'info' : 'danger';
    } catch (error) {
      preflight = null;
      checkError = api.errorMessage(error);
      fail(error);
    } finally {
      checking = false;
    }
  }

  async function runInstall() {
    setBusy(true);
    step = 'running';
    log = [];
    progress = null;
    footerMessage = 'Installazione in corso…';
    footerTone = 'info';

    try {
      report = await api.install($state.snapshot(options));
      step = 'done';
      footerMessage = 'Installazione completata.';
    } catch (error) {
      if (api.errorCode(error) === 'cancelled') {
        footerMessage = 'Installazione annullata.';
        footerTone = 'info';
      } else {
        fail(error);
      }
      // Si torna alle verifiche: la cartella non è stata toccata se
      // l'errore è arrivato prima dell'estrazione, e in ogni caso l'utente
      // può correggere e riprovare.
      step = 'checks';
      await runChecks();
    } finally {
      setBusy(false);
    }
  }

  async function finish() {
    if (launchAfter && report) {
      try {
        await api.launch(report.executable);
      } catch (error) {
        fail(error);
        return;
      }
    }
    await getCurrentWindow().close();
  }

  async function next() {
    if (step === 'welcome') {
      step = 'folder';
      return;
    }
    if (step === 'folder') {
      step = 'checks';
      await runChecks();
      return;
    }
    if (step === 'checks') {
      await runInstall();
      return;
    }
    if (step === 'done') {
      await finish();
    }
  }

  function back() {
    if (step === 'checks') step = 'folder';
    else if (step === 'folder') step = 'welcome';
  }

  async function cancel() {
    footerMessage = 'Annullamento…';
    await api.cancel();
  }
</script>

<div class="wizard">
  <StepRail steps={STEPS} current={stepIndex} />

  <div class="pane">
    <main>
      {#if step === 'welcome'}
        <StepWelcome
          boot={{ ...boot, release, releaseError }}
          {busy}
          onRetry={retryRelease}
          onOpenDownloadPage={() => api.openDownloadPage()}
        />
      {:else if step === 'folder'}
        <StepFolder
          {boot}
          {options}
          onBrowseInstall={browseInstall}
          onBrowseBackup={browseBackup}
        />
      {:else if step === 'checks'}
        <StepChecks {preflight} {checking} error={checkError} onRecheck={runChecks} />
      {:else if step === 'running'}
        <StepProgress title="Installazione in corso" {progress} {log} />
      {:else if report}
        <StepDone {report} bind:launchAfter />
      {/if}
    </main>

    <footer>
      <p class="status" class:status--danger={footerTone === 'danger'}>{footerMessage}</p>

      {#if step === 'running'}
        <button class="vk-btn" onclick={cancel}>Annulla</button>
      {:else}
        {#if canGoBack}
          <button class="vk-btn" onclick={back} disabled={busy}>Indietro</button>
        {/if}
        <button class="vk-btn vk-btn--primary" onclick={next} disabled={!nextEnabled}>
          {nextLabel}
          {#if step === 'checks'}
            <Icon name="download" size={14} />
          {:else}
            <span class="arrow"><Icon name="chevron" size={14} /></span>
          {/if}
        </button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .wizard {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .pane {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }

  main {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--vk-content-pad-top) var(--vk-content-pad-x) var(--vk-gap-md);
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
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status--danger {
    color: var(--vk-danger);
  }

  /* Il chevron guarda a destra solo quando il pulsante dice "avanti". */
  .arrow {
    display: inline-flex;
    transform: rotate(-90deg);
  }
</style>
