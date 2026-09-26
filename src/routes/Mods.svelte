<script lang="ts">
  /**
   * Mods.
   *
   * Ricalca il `ModsView` del WPF e ne stringe il disordine: la card della
   * modpack, quella del music pack, e sotto **due schede** — gli addon
   * installati e GameBanana — che nel legacy sono i due pulsanti larghi
   * `InstalledAddonsTabButton` / `GameBananaTabButton`.
   *
   * Ogni download della pagina passa dallo store delle operazioni (§D-086):
   * la barra con fase, byte, velocità e tempo che manca è la stessa per la
   * modpack, per il music pack e per gli addon di GameBanana, resta al suo
   * posto se si cambia pagina a metà, e mentre gira un download gli altri
   * pulsanti dicono che cosa stanno aspettando invece di fallire con
   * "occupato".
   *
   * Il canale di rilascio non sta qui: si sceglie una volta, in Impostazioni →
   * Percorsi, e non ha ragione di occupare spazio in una pagina che si usa a
   * ogni aggiornamento (vedi `docs/decisions.md` §D-039).
   */
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWebview } from '@tauri-apps/api/webview';

  import * as api from '$lib/api';
  import DownloadOverlay from '$lib/components/DownloadOverlay.svelte';
  import GameBananaBrowser from '$lib/components/GameBananaBrowser.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import Switch from '$lib/components/Switch.svelte';
  import TransferProgress from '$lib/components/TransferProgress.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import { operationLabel, operations } from '$lib/stores/operations.svelte';
  import type { AddonView, ConflictView, IntegrityReport, MusicPackStatus } from '$lib/api/types';

  type Tab = 'addons' | 'gamebanana';

  let tab = $state<Tab>('addons');
  let verifying = $state(false);
  let checking = $state(false);
  /** Un archivio sta passando sopra la finestra: la zona di rilascio si accende. */
  let dropping = $state(false);
  let integrity = $state<IntegrityReport | null>(null);
  let conflicts = $state<ConflictView[]>([]);
  let addons = $state<AddonView[]>([]);
  let addonBusy = $state('');
  let musicPack = $state<MusicPackStatus | null>(null);
  /** Levetta o rimozione del music pack: operazioni brevi, senza barra. */
  let musicBusy = $state<'' | 'toggle' | 'uninstall'>('');
  let confirmMusicRemoval = $state(false);
  /** Anteprime che il server non ha servito: al loro posto la sagoma. */
  let brokenPreviews = $state<string[]>([]);

  /**
   * Esito della verifica, che non è un download e quindi non passa dallo
   * store: dice se ci sono file da ripristinare.
   */
  type Activity = { tone: 'busy' | 'ok' | 'warn'; text: string };
  let activity = $state<Activity | null>(null);

  const mod = $derived(app.modState);

  const modRunning = $derived(operations.busyWith('mods'));
  const musicRunning = $derived(operations.busyWith('music-pack'));

  /**
   * Perché un pulsante di download è spento: c'è già un'altra operazione.
   * Vuoto quando si può partire.
   */
  const waitingFor = $derived(
    operations.active ? t('ops.waiting', { operation: operationLabel(operations.active) }) : ''
  );

  /**
   * Il music pack è un addon gestito: compare nella sua card e non fra gli
   * addon locali, altrimenti sarebbe elencato due volte.
   */
  const localAddons = $derived(
    addons.filter((addon) => addon.id !== 'official-vanzakart-music-pack')
  );

  /**
   * Provenienza mostrata nell'elenco. Il filtro compare solo quando c'e'
   * davvero qualcosa da separare: con addon di una sola provenienza sarebbe
   * un controllo che non cambia niente.
   */
  let source = $state<'all' | 'Local' | 'GameBanana'>('all');

  const fromGameBanana = $derived(
    localAddons.filter((addon) => addon.source === 'GameBanana').length
  );
  const showSourceFilter = $derived(fromGameBanana > 0 && fromGameBanana < localAddons.length);

  const SOURCES: { value: typeof source; label: string }[] = $derived([
    { value: 'all', label: t('mods.filterAll') },
    { value: 'Local', label: t('mods.filterLocal') },
    { value: 'GameBanana', label: 'GameBanana' }
  ]);

  const shownAddons = $derived(
    source === 'all' || !showSourceFilter
      ? localAddons
      : localAddons.filter((addon) => addon.source === source)
  );

  const health = $derived(
    !mod?.installed
      ? { tone: 'vk-badge--danger', label: t('home.badge.notInstalled') }
      : mod.needsRepair
        ? { tone: 'vk-badge--danger', label: t('home.badge.needsRepair') }
        : mod.updateAvailable
          ? { tone: 'vk-badge--warning', label: t('home.badge.update') }
          : { tone: 'vk-badge--success', label: t('home.badge.upToDate') }
  );

  /** Stato del music pack in una parola, per il badge. */
  const musicHealth = $derived(
    !musicPack || musicPack.blocker
      ? null
      : !musicPack.installed
        ? { tone: '', label: t('mods.musicNotInstalled') }
        : musicPack.updateAvailable
          ? { tone: 'vk-badge--warning', label: t('home.badge.update') }
          : { tone: 'vk-badge--success', label: t('home.badge.upToDate') }
  );

  const musicChangelog = $derived(
    (musicPack?.changelog ?? []).filter((line) => line.trim() !== '')
  );

  $effect(() => {
    void loadAddons();
  });

  /**
   * Archivi trascinati dentro la finestra.
   *
   * Con `dragDropEnabled` il rilascio lo intercetta Tauri, non la webview:
   * gli eventi HTML5 non arrivano mai, ma in cambio si ottengono i percorsi
   * veri dei file, che è l'unica forma che il backend può importare.
   */
  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const stop = await getCurrentWebview().onDragDropEvent((event) => {
        if (tab !== 'addons') return;

        if (event.payload.type === 'over') {
          dropping = true;
        } else if (event.payload.type === 'drop') {
          dropping = false;
          void importArchives(event.payload.paths);
        } else {
          dropping = false;
        }
      });

      if (disposed) stop();
      else unlisten = stop;
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  /**
   * Ricontrolla il manifest remoto senza installare niente.
   *
   * Aggiorna anche il music pack: la sua versione disponibile arriva dallo
   * stesso `versions.json`, e la card restava ferma a prima del controllo.
   */
  async function checkUpdates() {
    checking = true;
    try {
      await api.checkUpdates();
      await app.refresh();
      await loadAddons();
      const state = app.modState;
      const musicUpdate = musicPack?.installed && musicPack.updateAvailable;
      app.toast(
        t('mods.checkTitle'),
        state?.updateAvailable
          ? t('mods.checkAvailable', { version: state.latestVersion })
          : musicUpdate
            ? t('mods.checkMusicAvailable', { version: musicPack?.latestVersion ?? '' })
            : t('mods.checkUpToDate'),
        state?.updateAvailable || musicUpdate ? 'info' : 'success'
      );
    } catch (error) {
      app.toast(t('home.checkFailed'), api.errorMessage(error), 'warning');
    } finally {
      checking = false;
    }
  }

  /** Apre una cartella e riporta l'errore invece di ingoiarlo. */
  async function openFolder(key: 'mod' | 'addons') {
    try {
      await api.openFolder(key);
    } catch (error) {
      app.toast(t('mods.folderFailed'), api.errorMessage(error), 'warning');
    }
  }

  async function loadAddons() {
    try {
      [addons, conflicts, musicPack] = await Promise.all([
        api.listAddons(),
        api.getConflicts(),
        api.getMusicPackStatus()
      ]);
    } catch {
      addons = [];
      conflicts = [];
      musicPack = null;
    }
  }

  /**
   * Installa o aggiorna il music pack.
   *
   * L'esito finisce nella card — sotto la barra — e in un avviso: chi è
   * andato su un'altra pagina durante il download lo vede lo stesso.
   */
  async function installMusicPack() {
    try {
      const outcome = await operations.run('music-pack', () => api.installMusicPack(), {
        title: 'VanzaKart Music Pack',
        describe: (result) => result.summary
      });
      app.toast('VanzaKart Music Pack', outcome.summary, 'success');
    } catch (error) {
      if (api.errorCode(error) !== 'cancelled') {
        app.toast(t('mods.musicFailed'), api.errorMessage(error), 'warning');
      }
    } finally {
      await loadAddons();
    }
  }

  async function toggleMusicPack() {
    if (!musicPack || musicBusy || operations.busy) return;
    musicBusy = 'toggle';
    try {
      musicPack = await api.setMusicPackEnabled(!musicPack.enabled);
    } catch (error) {
      app.toast(t('home.operationFailed'), api.errorMessage(error), 'warning');
    } finally {
      musicBusy = '';
    }
  }

  async function uninstallMusicPack() {
    confirmMusicRemoval = false;
    musicBusy = 'uninstall';
    try {
      musicPack = await api.uninstallMusicPack();
      operations.clearOutcome('music-pack');
      app.toast('VanzaKart Music Pack', t('mods.musicRemoved'), 'success');
      await loadAddons();
    } catch (error) {
      app.toast(t('home.operationFailed'), api.errorMessage(error), 'warning');
    } finally {
      musicBusy = '';
    }
  }

  /** Il nome proposto è quello del file, senza estensione. */
  function suggestedName(path: string): string {
    return (
      path
        .split(/[\\/]/)
        .pop()
        ?.replace(/\.zip$/i, '') ?? 'Addon'
    );
  }

  /**
   * Importa uno o più archivi.
   *
   * Uno alla volta, e un archivio che fallisce non ferma gli altri: chi
   * trascina dentro cinque zip vuole i quattro buoni, non zero.
   */
  async function importArchives(paths: string[]) {
    const archives = paths.filter((path) => path.toLowerCase().endsWith('.zip'));
    if (archives.length === 0) {
      app.toast(t('mods.nothingToImport'), t('mods.zipOnly'), 'warning');
      return;
    }

    addonBusy = 'import';
    let imported = 0;
    try {
      for (const archive of archives) {
        try {
          const addon = await api.importAddon(archive, suggestedName(archive));
          imported += 1;
          app.toast(
            t('mods.addonImported'),
            t('mods.addonImportedBody', { name: addon.name, count: addon.fileCount }),
            'success'
          );
        } catch (error) {
          app.toast(t('mods.importFailed'), api.errorMessage(error), 'warning');
        }
      }
    } finally {
      addonBusy = '';
    }

    if (imported > 0) await loadAddons();
  }

  async function importAddon() {
    const selected = await open({
      multiple: true,
      directory: false,
      title: t('mods.pickArchives'),
      filters: [{ name: t('mods.zipFilter'), extensions: ['zip'] }]
    });
    if (selected === null) return;

    await importArchives(Array.isArray(selected) ? selected : [selected]);
  }

  async function withAddon(addon: AddonView, run: () => Promise<unknown>, done?: string) {
    addonBusy = addon.id;
    try {
      await run();
      await loadAddons();
      if (done) app.toast(t('mods.addon'), done, 'success');
    } catch (error) {
      app.toast(t('home.operationFailed'), api.errorMessage(error), 'warning');
    } finally {
      addonBusy = '';
    }
  }

  async function run(action: 'install' | 'repair') {
    if (operations.busy) return;
    integrity = null;
    activity = null;

    try {
      const outcome = await operations.run(
        'mods',
        () => (action === 'install' ? api.installMods() : api.repairMods()),
        {
          title: 'VanzaKart Modpack',
          describe: (result) => result.summary
        }
      );
      for (const warning of outcome.warnings) app.toast(t('common.warning'), warning, 'warning');
      await app.refresh();
      await loadAddons();
    } catch (error) {
      if (api.errorCode(error) !== 'cancelled') {
        app.toast(t('home.operationFailed'), api.errorMessage(error), 'danger');
      }
    }
  }

  async function verify() {
    verifying = true;
    integrity = null;
    activity = { tone: 'busy', text: t('mods.verifyBusy') };
    try {
      integrity = await api.verifyMods();
      activity = {
        tone: integrity.mismatched.length > 0 ? 'warn' : 'ok',
        text: integrity.message
      };
    } catch (error) {
      const message = api.errorMessage(error);
      activity = { tone: 'warn', text: message };
      app.toast(t('home.verifyFailed'), message, 'warning');
    } finally {
      verifying = false;
    }
  }
</script>

<div class="page">
  <!-- ── MODPACK ─────────────────────────────────────────────────────── -->
  <section class="vk-card vk-rainbow-top modpack">
    <header class="modpack-head">
      <div class="identity">
        <h2 class="vk-title">VanzaKart Modpack</h2>
        <span class="vk-badge {mod?.channel === 'Beta' ? 'vk-badge--beta' : 'vk-badge--stable'}">
          {(mod?.channel ?? 'Stable').toUpperCase()}
        </span>
        <span class="vk-badge {health.tone}">{health.label}</span>
      </div>

      <div class="versions">
        <div class="version">
          <span class="vk-eyebrow">{t('home.installedLabel')}</span>
          <strong>{mod?.installedVersion || '—'}</strong>
        </div>
        <span class="arrow" class:pending={mod?.updateAvailable} aria-hidden="true">
          {mod?.updateAvailable ? '→' : '·'}
        </span>
        <div class="version" class:next={mod?.updateAvailable}>
          <span class="vk-eyebrow">{t('home.availableLabel')}</span>
          <strong>{mod?.latestVersion || '—'}</strong>
        </div>
      </div>
    </header>

    {#if mod?.needsRepair}
      <p class="alert">{t('mods.repairAlert', { reason: mod.repairReason })}</p>
    {/if}

    <div class="actions">
      <button
        class="vk-btn vk-btn--primary main"
        onclick={() => run('install')}
        disabled={operations.busy || verifying}
        title={operations.blockedBy('mods') ? waitingFor : undefined}
      >
        <Icon name="download" size={15} />
        {modRunning
          ? t('common.working')
          : !mod?.installed
            ? t('mods.install')
            : mod.updateAvailable || mod.needsRepair
              ? t('mods.update')
              : t('mods.reinstall')}
      </button>

      <div class="secondary">
        <button
          class="vk-btn"
          onclick={checkUpdates}
          disabled={operations.busy || verifying || checking}
        >
          <Icon name="refresh" size={14} />
          {checking ? t('home.checking') : t('mods.updates')}
        </button>
        <button
          class="vk-btn"
          onclick={() => run('repair')}
          disabled={operations.busy || verifying || !mod?.installed}
        >
          <Icon name="repair" size={14} />
          {t('mods.repair')}
        </button>
        <button
          class="vk-btn"
          onclick={verify}
          disabled={verifying || operations.busy || !mod?.installed}
        >
          <Icon name="check" size={14} />
          {verifying ? t('home.verifying') : t('home.verify')}
        </button>
        <button
          class="vk-btn"
          title={t('mods.modFolderTitle')}
          onclick={() => openFolder('mod')}
          disabled={modRunning}
        >
          <Icon name="folder" size={14} />
          {t('mods.modFolder')}
        </button>
      </div>
    </div>

    {#if operations.blockedBy('mods')}
      <p class="waiting"><Icon name="warning" size={13} /> {waitingFor}</p>
    {/if}

    <TransferProgress kind="mods" onretry={() => run('install')} />

    {#if verifying}
      <div class="progress-block">
        <div class="vk-progress vk-progress--indeterminate">
          <div class="vk-progress__fill"></div>
        </div>
      </div>
    {/if}

    {#if activity && !verifying}
      <div class="activity" data-tone={activity.tone}>
        <Icon name={activity.tone === 'ok' ? 'check' : 'warning'} size={14} />
        <span>{activity.text}</span>
        <button
          class="vk-btn vk-btn--ghost small dismiss"
          aria-label={t('common.hide')}
          onclick={() => {
            activity = null;
            integrity = null;
          }}
        >
          ✕
        </button>
      </div>
    {/if}

    {#if integrity && integrity.mismatched.length > 0}
      <details class="report">
        <summary>{t('mods.mismatched', { count: integrity.mismatched.length })}</summary>
        <ul class="file-list vk-mono">
          {#each integrity.mismatched.slice(0, 60) as path (path)}
            <li>{path}</li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if mod?.changelog?.length}
      <details class="changelog-block">
        <summary>
          {t('mods.changelogTitle', {
            version: mod.latestVersion || t('mods.versionAvailable')
          })}
        </summary>
        <ul class="changelog">
          {#each mod.changelog as line, index (index)}
            <li>{line}</li>
          {/each}
        </ul>
      </details>
    {/if}
  </section>

  <!-- ── MUSIC PACK ──────────────────────────────────────────────────── -->
  <section class="vk-card music">
    <header class="modpack-head">
      <div class="identity">
        <span class="music-glyph" aria-hidden="true">♪</span>
        <div>
          <h2 class="music-name">VanzaKart Music Pack</h2>
          <p class="vk-faint music-note">{t('mods.musicDescription')}</p>
        </div>
        {#if musicHealth}
          <span class="vk-badge {musicHealth.tone}">{musicHealth.label}</span>
        {/if}
        {#if musicPack?.installed}
          <span class="vk-badge {musicPack.enabled ? 'vk-badge--success' : ''}">
            {musicPack.enabled ? t('mods.musicActive') : t('mods.musicInactive')}
          </span>
        {/if}
      </div>

      {#if musicPack && !musicPack.blocker}
        <div class="versions">
          <div class="version">
            <span class="vk-eyebrow">{t('home.installedLabel')}</span>
            <strong>{musicPack.installedVersion || '—'}</strong>
          </div>
          <span class="arrow" class:pending={musicPack.updateAvailable} aria-hidden="true">
            {musicPack.updateAvailable ? '→' : '·'}
          </span>
          <div class="version" class:next={musicPack.updateAvailable}>
            <span class="vk-eyebrow">{t('home.availableLabel')}</span>
            <strong>{musicPack.latestVersion || '—'}</strong>
          </div>
        </div>
      {/if}
    </header>

    {#if musicPack?.blocker}
      <div class="blocker">
        <Icon name="warning" size={16} />
        <p>{t('mods.musicNeedsModpack')}</p>
        <button
          class="vk-btn vk-btn--primary"
          onclick={() => run('install')}
          disabled={operations.busy}
        >
          <Icon name="download" size={14} />
          {t('mods.installModpackFirst')}
        </button>
      </div>
    {:else}
      <div class="actions">
        {#if !musicPack?.installed || musicPack.updateAvailable || musicRunning}
          <button
            class="vk-btn vk-btn--primary main"
            onclick={installMusicPack}
            disabled={operations.busy || musicBusy !== ''}
            title={operations.blockedBy('music-pack') ? waitingFor : undefined}
          >
            <Icon name="download" size={15} />
            {musicRunning
              ? t('common.working')
              : musicPack?.installed
                ? t('mods.musicUpdate', { version: musicPack.latestVersion })
                : t('mods.musicInstall')}
          </button>
        {/if}

        {#if musicPack?.installed}
          <div class="music-toggle">
            <Switch
              checked={musicPack.enabled}
              label={musicPack.enabled ? t('mods.musicDisable') : t('mods.musicEnable')}
              busy={musicBusy === 'toggle'}
              disabled={musicBusy !== '' || operations.busy}
              onchange={toggleMusicPack}
            />
            <span class="vk-faint">
              {musicPack.enabled ? t('mods.musicOnHint') : t('mods.musicOffHint')}
            </span>
          </div>

          <div class="secondary">
            <span class="vk-faint tracks">{t('mods.tracks', { count: musicPack.fileCount })}</span>
            <button
              class="vk-btn vk-btn--danger"
              onclick={() => (confirmMusicRemoval = true)}
              disabled={musicBusy !== '' || operations.busy}
            >
              <Icon name="trash" size={14} />
              {musicBusy === 'uninstall' ? t('mods.wait') : t('common.remove')}
            </button>
          </div>
        {/if}
      </div>

      {#if operations.blockedBy('music-pack') && (!musicPack?.installed || musicPack.updateAvailable)}
        <p class="waiting"><Icon name="warning" size={13} /> {waitingFor}</p>
      {/if}

      <TransferProgress kind="music-pack" onretry={installMusicPack} />

      {#if musicChangelog.length > 0}
        <details class="changelog-block">
          <summary>
            {t('mods.changelogTitle', {
              version: musicPack?.latestVersion || t('mods.versionAvailable')
            })}
          </summary>
          <ul class="changelog">
            {#each musicChangelog as line, index (index)}
              <li>{line}</li>
            {/each}
          </ul>
        </details>
      {/if}
    {/if}
  </section>

  <!-- ── SCHEDE ──────────────────────────────────────────────────────── -->
  <nav class="tabs" aria-label={t('mods.tabsAria')}>
    <button class="tab" class:active={tab === 'addons'} onclick={() => (tab = 'addons')}>
      <Icon name="package" size={15} />
      {t('mods.installedAddons')}
      {#if localAddons.length > 0}<span class="count">{localAddons.length}</span>{/if}
    </button>
    <button class="tab" class:active={tab === 'gamebanana'} onclick={() => (tab = 'gamebanana')}>
      <Icon name="external" size={15} />
      GameBanana
      {#if operations.busyWith('gamebanana')}
        <span class="count live">{Math.round(operations.percent ?? 0)}%</span>
      {/if}
    </button>
  </nav>

  {#if tab === 'addons'}
    <section class="vk-card">
      <div class="section-head">
        <p class="vk-eyebrow">{t('mods.installedAddons')}</p>
        <div class="vk-row">
          <button
            class="vk-btn"
            title={t('mods.addonFolderTitle')}
            onclick={() => openFolder('addons')}
          >
            <Icon name="folder" size={14} />
            {t('mods.addonFolder')}
          </button>
          <button class="vk-btn vk-btn--primary" onclick={importAddon} disabled={addonBusy !== ''}>
            <Icon name="download" size={14} />
            {addonBusy === 'import' ? t('mods.importing') : t('mods.importZip')}
          </button>
        </div>
      </div>

      <button
        class="dropzone"
        class:hot={dropping}
        onclick={importAddon}
        disabled={addonBusy !== ''}
      >
        <Icon name="download" size={22} />
        <span class="drop-title">
          {dropping ? t('mods.dropHere') : t('mods.dragHere')}
        </span>
        <span class="vk-faint drop-hint">{t('mods.dropHint')}</span>
      </button>

      {#if localAddons.length === 0}
        <div class="vk-empty">
          <Icon name="package" size={28} />
          <p>{t('mods.noAddons')}</p>
          <p class="vk-faint">{t('mods.noAddonsHint')}</p>
        </div>
      {:else}
        {#if showSourceFilter}
          <div class="filters">
            {#each SOURCES as item (item.value)}
              <button
                class="chip"
                class:active={source === item.value}
                onclick={() => (source = item.value)}
              >
                {item.label}
              </button>
            {/each}
          </div>
        {/if}

        <ul class="addons">
          {#each shownAddons as addon (addon.id)}
            <li class="addon" class:off={!addon.enabled}>
              {#if addon.previewUrl && !brokenPreviews.includes(addon.previewUrl)}
                <img
                  class="addon-thumb"
                  src={addon.previewUrl}
                  alt=""
                  loading="lazy"
                  onerror={() => (brokenPreviews = [...brokenPreviews, addon.previewUrl])}
                />
              {:else}
                <span class="addon-thumb empty"><Icon name="package" size={18} /></span>
              {/if}

              <div class="addon-info">
                <strong>{addon.name}</strong>
                <span class="vk-faint">
                  {t('mods.addonMeta', {
                    author: addon.author || addon.source,
                    count: addon.fileCount
                  })}{addon.managed ? '' : t('mods.unmanaged')}
                </span>
              </div>

              <!-- Prima lo stato, poi le azioni: la levetta è ciò che si
                   guarda scorrendo l'elenco, non un pulsante fra i pulsanti. -->
              <Switch
                checked={addon.enabled}
                label={addon.enabled
                  ? t('mods.disableAddon', { name: addon.name })
                  : t('mods.enableAddon', { name: addon.name })}
                disabled={!addon.managed || (addonBusy !== '' && addonBusy !== addon.id)}
                busy={addonBusy === addon.id}
                onchange={() =>
                  withAddon(addon, () => api.setAddonEnabled(addon.id, !addon.enabled))}
              />

              {#if addonBusy === addon.id}
                <span class="working"
                  ><span class="dot busy" aria-hidden="true"></span>{t('mods.wait')}</span
                >
              {:else}
                {#if addon.sourceUrl}
                  <button
                    class="vk-btn icon-btn"
                    title={t('mods.openOnGameBanana')}
                    aria-label={t('mods.openOnGameBanana')}
                    onclick={() => api.openExternal(addon.sourceUrl)}
                  >
                    <Icon name="external" size={14} />
                  </button>
                {/if}
                <button
                  class="vk-btn vk-btn--danger icon-btn"
                  title={t('mods.removeAddon')}
                  aria-label={t('mods.removeAddon')}
                  onclick={() =>
                    withAddon(
                      addon,
                      () => api.removeAddon(addon.id),
                      t('mods.addonRemoved', { name: addon.name })
                    )}
                  disabled={addonBusy !== '' || !addon.managed}
                >
                  <Icon name="trash" size={14} />
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if conflicts.length > 0}
        <div class="conflicts-block">
          <p class="vk-eyebrow">{t('mods.conflicts', { count: conflicts.length })}</p>
          <p class="vk-subtitle">{t('mods.conflictsHint')}</p>
          <ul class="conflicts">
            {#each conflicts as conflict (conflict.fileName)}
              <li>
                <span class="vk-badge vk-badge--warning">{conflict.count}×</span>
                <strong>{conflict.fileName}</strong>
                <span class="vk-faint vk-mono">{conflict.locations.join(' · ')}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </section>
  {:else}
    <GameBananaBrowser oninstalled={loadAddons} />
  {/if}
</div>

<!--
  Il pannello del download di GameBanana sta qui e non dentro il browser:
  passando alla scheda degli addon a metà download il browser sparisce, il
  download no, e il pannello deve restare (§D-086).
-->
<DownloadOverlay
  open={operations.busyWith('gamebanana')}
  title={operations.meta.title ?? ''}
  subtitle={operations.meta.subtitle ?? ''}
/>

<Modal
  open={confirmMusicRemoval}
  title={t('mods.musicRemoveTitle')}
  confirmLabel={t('common.remove')}
  danger
  onconfirm={uninstallMusicPack}
  oncancel={() => (confirmMusicRemoval = false)}
>
  {t('mods.musicRemoveBody')}
</Modal>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 980px;
    margin: 0 auto;
    padding-bottom: 12px;
  }

  /* ---- Modpack ---- */

  .modpack {
    padding: 24px 26px;
  }

  .modpack-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    flex-wrap: wrap;
  }

  .identity {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .versions {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .version {
    display: flex;
    flex-direction: column;
    gap: 1px;
    text-align: right;
  }

  .version strong {
    font-size: 19px;
    font-weight: 900;
    line-height: 1.1;
  }

  /* La versione a cui si sta andando e' quella che conta: si vede. */
  .version.next strong {
    background: var(--vk-play-gradient);
    background-clip: text;
    -webkit-background-clip: text;
    color: transparent;
  }

  /* Quando le due versioni coincidono la seconda resta, ma smorzata: dice
     "sei in pari" senza gridarlo. */
  .version:not(.next) strong {
    color: var(--vk-text);
  }

  .versions .arrow {
    font-size: 18px;
    color: var(--vk-text-faint);
  }

  .versions .arrow.pending {
    color: var(--vk-warning);
  }

  .versions .version:last-child:not(.next) strong {
    color: var(--vk-text-secondary);
  }

  .alert {
    margin: 14px 0 0;
    padding: 10px 12px;
    border-radius: var(--vk-radius-badge);
    background: color-mix(in srgb, var(--vk-danger) 14%, transparent);
    font-size: var(--vk-fs-small);
    color: var(--vk-danger);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-top: 20px;
  }

  /* L'azione principale e' una sola: le altre non devono pesare uguale. */
  .actions .main {
    min-width: 200px;
    height: 46px;
    font-size: var(--vk-fs-body);
  }

  .secondary {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  /* Un'altra operazione tiene il turno: lo si dice sotto i pulsanti spenti. */
  .waiting {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 0 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .progress-block {
    margin-top: 16px;
  }

  .small {
    padding: 4px 10px;
    font-size: var(--vk-fs-micro);
  }

  .activity {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
    padding: 10px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
    font-size: var(--vk-fs-small);
  }

  .activity[data-tone='ok'] {
    border-color: color-mix(in srgb, var(--vk-success) 45%, var(--vk-stroke));
    color: var(--vk-success);
  }

  .activity[data-tone='warn'] {
    border-color: color-mix(in srgb, var(--vk-warning) 45%, var(--vk-stroke));
    color: var(--vk-warning);
  }

  .activity .dismiss {
    margin-left: auto;
    padding: 2px 8px;
    color: inherit;
  }

  /* Pulsazione: dice che l'operazione e' viva anche senza percentuale. */
  .dot {
    width: 9px;
    height: 9px;
    flex: none;
    border-radius: 50%;
    background: var(--vk-cyan);
    animation: vk-pulse 1.1s var(--vk-ease) infinite;
  }

  @keyframes vk-pulse {
    0%,
    100% {
      opacity: 0.35;
      transform: scale(0.8);
    }
    50% {
      opacity: 1;
      transform: scale(1);
    }
  }

  .report {
    margin-top: 12px;
  }

  .changelog-block {
    margin-top: 14px;
  }

  summary {
    cursor: pointer;
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    color: var(--vk-text-secondary);
  }

  .changelog {
    margin: 10px 0 0;
    padding-left: 18px;
    font-size: var(--vk-fs-small);
    color: var(--vk-text-secondary);
  }

  .file-list {
    max-height: 220px;
    margin: 10px 0 0;
    padding-left: 18px;
    overflow-y: auto;
    font-size: var(--vk-fs-eyebrow);
    color: var(--vk-text-secondary);
  }

  /* ---- Music pack ---- */

  .music {
    padding: 22px 26px;
  }

  .music-glyph {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 12px;
    background: var(--vk-play-gradient);
    font-size: 20px;
    font-weight: 900;
    color: #fff;
    box-shadow: 0 0 14px rgb(255 0 102 / 0.3);
  }

  .music-name {
    margin: 0;
    font-size: var(--vk-fs-card-title);
    font-weight: 900;
  }

  .music-note {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .music-toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--vk-fs-micro);
  }

  .tracks {
    font-size: var(--vk-fs-micro);
  }

  .music .secondary {
    margin-left: auto;
  }

  .blocker {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-top: 16px;
    padding: 12px 14px;
    border: 1px solid color-mix(in srgb, var(--vk-warning) 40%, var(--vk-stroke));
    border-radius: var(--vk-radius-badge);
    background: color-mix(in srgb, var(--vk-warning) 8%, transparent);
    color: var(--vk-warning);
    font-size: var(--vk-fs-small);
  }

  .blocker p {
    flex: 1;
    min-width: 200px;
    margin: 0;
  }

  /* ---- Schede ---- */

  .tabs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-top: 4px;
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    height: 46px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel-soft);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-small);
    font-weight: 900;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    cursor: pointer;
    overflow: hidden;
    transition:
      color var(--vk-dur-fast) var(--vk-ease),
      border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .tab:hover {
    color: var(--vk-text-primary);
    border-color: #3a4c74;
  }

  /* La scheda attiva porta la firma arcobaleno del launcher. */
  .tab.active {
    color: var(--vk-text-primary);
    border-color: transparent;
    background: var(--vk-tab-active);
  }

  .tab.active::after {
    content: '';
    position: absolute;
    inset: auto 0 0;
    height: 3px;
    background: var(--vk-rainbow);
  }

  .count {
    display: grid;
    place-items: center;
    min-width: 22px;
    height: 20px;
    padding: 0 6px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.1);
    font-size: var(--vk-fs-eyebrow);
  }

  .count.live {
    background: color-mix(in srgb, var(--vk-cyan) 25%, transparent);
    color: var(--vk-cyan-soft);
    font-variant-numeric: tabular-nums;
  }

  /* ---- Addon ---- */

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }

  .filters {
    display: flex;
    gap: 6px;
    margin-bottom: 12px;
  }

  .chip {
    padding: 5px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: 999px;
    background: transparent;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    cursor: pointer;
  }

  .chip:hover {
    border-color: #3a4c74;
  }

  .chip.active {
    border-color: transparent;
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    background-size:
      auto,
      220% 100%;
    animation: vk-rainbow-edge 8s ease-in-out infinite;
    box-shadow:
      0 0 14px rgb(255 0 102 / 0.22),
      0 0 14px rgb(0 242 255 / 0.18);
    color: var(--vk-text);
  }

  .addons {
    display: flex;
    flex-direction: column;
    gap: 8px;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .addon {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
  }

  .addon.off {
    opacity: 0.62;
  }

  /*
   * Zona di rilascio. È un `<button>` perché fa anche da scorciatoia al
   * selettore di file: trascinare non è l'unico modo, e chi non può trascinare
   * deve poter arrivare allo stesso posto con un clic o da tastiera.
   */
  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 18px;
    margin-bottom: 14px;
    border: 1.5px dashed var(--vk-stroke);
    border-radius: var(--vk-radius-input);
    background: transparent;
    color: var(--vk-text-secondary);
    transition:
      border-color var(--vk-dur-fast) var(--vk-ease),
      background var(--vk-dur-fast) var(--vk-ease),
      color var(--vk-dur-fast) var(--vk-ease);
  }

  .dropzone:hover:not(:disabled) {
    border-color: #3a4c74;
    color: var(--vk-text);
  }

  .dropzone:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Con un archivio sopra la finestra la zona si accende: il bordo diventa
     arcobaleno, come tutto ciò che nel launcher è "attivo adesso". */
  .dropzone.hot {
    border-color: transparent;
    border-style: solid;
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    color: var(--vk-text);
    box-shadow:
      0 0 16px rgb(255 0 102 / 0.22),
      0 0 16px rgb(0 242 255 / 0.2);
  }

  .drop-title {
    font-size: var(--vk-fs-small);
    font-weight: 800;
  }

  .drop-hint {
    font-size: var(--vk-fs-eyebrow);
  }

  /* 16:9 come le anteprime di GameBanana; la sagoma quando non c'è immagine. */
  .addon-thumb {
    flex: none;
    width: 72px;
    height: 41px;
    border-radius: var(--vk-radius-badge);
    object-fit: cover;
    background: var(--vk-input);
  }

  .addon-thumb.empty {
    display: grid;
    place-items: center;
    border: 1px solid var(--vk-stroke);
    color: var(--vk-text-faint);
  }

  .icon-btn {
    padding: 9px 11px;
  }

  .working {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .addon-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .addon-info span {
    font-size: var(--vk-fs-micro);
  }

  .conflicts-block {
    margin-top: 18px;
    padding-top: 16px;
    border-top: 1px solid var(--vk-stroke);
  }

  .conflicts {
    display: flex;
    flex-direction: column;
    gap: 6px;
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    font-size: var(--vk-fs-micro);
  }

  .conflicts li {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  @media (max-width: 720px) {
    .modpack-head {
      align-items: flex-start;
      flex-direction: column;
    }

    .music .secondary {
      margin-left: 0;
    }
  }
</style>
