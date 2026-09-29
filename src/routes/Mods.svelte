<script lang="ts">
  /**
   * Mods.
   *
   * Tre domande, una per blocco, e per ognuna una risposta in una riga:
   *
   * - **la modpack** è pronta? Se no, un pulsante solo fa ciò che serve —
   *   installare, aggiornare, riparare. Controllare, verificare, riscaricare
   *   e aprire la cartella si fanno ogni tanto, e stanno nel menu `⋯`;
   * - **il music pack** è acceso? Una levetta;
   * - **quali addon** ho? L'elenco, con la sua levetta per addon, e accanto
   *   GameBanana per prenderne altri.
   *
   * Ogni download della pagina passa dallo store delle operazioni (§D-086):
   * la barra con fase, byte, velocità e tempo che manca resta al suo posto se
   * si cambia pagina a metà, e mentre gira un download gli altri pulsanti
   * dicono che cosa stanno aspettando invece di fallire con "occupato".
   *
   * Il canale di rilascio non sta qui: si sceglie una volta, in Impostazioni →
   * Percorsi (vedi `docs/decisions.md` §D-039).
   */
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWebview } from '@tauri-apps/api/webview';

  import * as api from '$lib/api';
  import DownloadOverlay from '$lib/components/DownloadOverlay.svelte';
  import GameBananaBrowser from '$lib/components/GameBananaBrowser.svelte';
  import Icon, { type IconName } from '$lib/components/Icon.svelte';
  import MenuButton, { type MenuItem } from '$lib/components/MenuButton.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import Switch from '$lib/components/Switch.svelte';
  import TransferProgress from '$lib/components/TransferProgress.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import { operationLabel, operations } from '$lib/stores/operations.svelte';
  import type { AddonView, ConflictView, IntegrityReport, MusicPackStatus } from '$lib/api/types';

  type Tab = 'addons' | 'gamebanana';

  const MUSIC_PACK_ID = 'official-vanzakart-music-pack';

  let tab = $state<Tab>('addons');
  let verifying = $state(false);
  let checking = $state(false);
  /** Un archivio sta passando sopra la finestra: si accende l'invito a rilasciarlo. */
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
   * Vuoto quando si può partire. Sta nel suggerimento del pulsante, non in
   * una riga in più.
   */
  const waitingFor = $derived(
    operations.active ? t('ops.waiting', { operation: operationLabel(operations.active) }) : ''
  );

  /**
   * Il music pack è un addon gestito: compare nella sua card e non fra gli
   * addon, altrimenti sarebbe elencato due volte.
   */
  const localAddons = $derived(addons.filter((addon) => addon.id !== MUSIC_PACK_ID));

  /** Le mod di GameBanana già installate, per segnarle nel catalogo. */
  const installedGameBanana = $derived(
    localAddons
      .map((addon) => /^gamebanana-(\d+)-/.exec(addon.id)?.[1])
      .filter((id): id is string => id !== undefined)
      .map(Number)
  );

  type Tone = 'idle' | 'success' | 'warning' | 'danger';

  /** Lo stato della modpack in una frase. */
  const modStatus = $derived.by((): { tone: Tone; text: string } => {
    if (!mod) return { tone: 'idle', text: t('mods.status.checking') };
    if (!mod.installed) return { tone: 'danger', text: t('mods.status.notInstalled') };
    if (mod.needsRepair) return { tone: 'danger', text: t('mods.status.needsRepair') };
    if (mod.updateAvailable) {
      return {
        tone: 'warning',
        text: t('mods.status.update', {
          from: mod.installedVersion || '—',
          to: mod.latestVersion
        })
      };
    }
    return {
      tone: 'success',
      text: t('mods.status.ready', { version: mod.installedVersion || '—' })
    };
  });

  /**
   * L'unica azione che serve adesso, se ne serve una. Con la modpack pronta
   * non c'è niente da premere: si gioca dalla Home.
   */
  const modAction = $derived.by((): { label: string; icon: IconName } | null => {
    if (!mod?.installed) return { label: t('mods.install'), icon: 'download' };
    if (mod.needsRepair) return { label: t('mods.repair'), icon: 'repair' };
    if (mod.updateAvailable) {
      return { label: t('mods.updateTo', { version: mod.latestVersion }), icon: 'download' };
    }
    return null;
  });

  const modTools = $derived<MenuItem[]>([
    {
      label: checking ? t('home.checking') : t('mods.checkUpdates'),
      icon: 'refresh',
      disabled: operations.busy || verifying || checking,
      onselect: () => void checkUpdates()
    },
    {
      label: t('mods.verifyFiles'),
      icon: 'check',
      disabled: verifying || operations.busy || !mod?.installed,
      onselect: () => void verify()
    },
    {
      label: t('mods.redownload'),
      icon: 'repair',
      hint: t('mods.redownloadHint'),
      disabled: operations.busy || verifying || !mod?.installed,
      onselect: () => void run('repair')
    },
    {
      label: t('mods.modFolder'),
      icon: 'folder',
      disabled: modRunning,
      onselect: () => void openFolder('mod')
    }
  ]);

  /** Lo stato del music pack in una frase. */
  const musicStatus = $derived.by((): string => {
    if (!musicPack) return '';
    if (musicPack.blocker) return t('mods.musicNeedsModpack');
    if (!musicPack.installed) return t('mods.musicDescription');
    if (musicPack.updateAvailable) {
      return t('mods.status.update', {
        from: musicPack.installedVersion || '—',
        to: musicPack.latestVersion
      });
    }
    return musicPack.enabled
      ? t('mods.music.on', { count: musicPack.fileCount })
      : t('mods.music.off', { count: musicPack.fileCount });
  });

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
   * veri dei file, che è l'unica forma che il backend può importare. Si
   * accettano ovunque nella pagina, e l'elenco degli addon si apre da sé.
   */
  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const stop = await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === 'over') {
          dropping = true;
        } else if (event.payload.type === 'drop') {
          dropping = false;
          tab = 'addons';
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

  function addonTools(addon: AddonView): MenuItem[] {
    const items: MenuItem[] = [];
    if (addon.sourceUrl) {
      items.push({
        label: t('mods.openOnGameBanana'),
        icon: 'external',
        onselect: () => void api.openExternal(addon.sourceUrl)
      });
    }
    items.push({
      label: t('mods.removeAddon'),
      icon: 'trash',
      danger: true,
      disabled: addonBusy !== '' || !addon.managed,
      onselect: () =>
        void withAddon(
          addon,
          () => api.removeAddon(addon.id),
          t('mods.addonRemoved', { name: addon.name })
        )
    });
    return items;
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
    <div class="head">
      <div class="identity">
        <p class="vk-eyebrow">
          Modpack
          {#if mod?.channel === 'Beta'}
            <span class="vk-badge vk-badge--beta">BETA</span>
          {/if}
        </p>
        <h2 class="vk-title">VanzaKart</h2>
        <p class="status" data-tone={modStatus.tone}>
          <span class="status-dot" aria-hidden="true"></span>
          {modStatus.text}
        </p>
      </div>

      <div class="head-actions">
        {#if modAction}
          <button
            class="vk-btn vk-btn--primary main"
            onclick={() => run('install')}
            disabled={operations.busy || verifying}
            title={operations.blockedBy('mods') ? waitingFor : undefined}
          >
            <Icon name={modAction.icon} size={15} />
            {modRunning ? t('common.working') : modAction.label}
          </button>
        {/if}
        <MenuButton items={modTools} label={t('mods.toolsMenu')} />
      </div>
    </div>

    {#if mod?.needsRepair}
      <p class="alert">{t('mods.repairAlert', { reason: mod.repairReason })}</p>
    {/if}

    <TransferProgress kind="mods" onretry={() => run('install')} />

    {#if verifying}
      <div class="vk-progress vk-progress--indeterminate verify-bar">
        <div class="vk-progress__fill"></div>
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
      <details class="fold">
        <summary>{t('mods.mismatched', { count: integrity.mismatched.length })}</summary>
        <ul class="file-list vk-mono">
          {#each integrity.mismatched.slice(0, 60) as path (path)}
            <li>{path}</li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if mod?.updateAvailable && mod.changelog.length > 0}
      <details class="fold">
        <summary>{t('mods.changelogTitle', { version: mod.latestVersion })}</summary>
        <ul class="changelog">
          {#each mod.changelog as line, index (index)}
            <li>{line}</li>
          {/each}
        </ul>
      </details>
    {/if}
  </section>

  <!-- ── MUSIC PACK ──────────────────────────────────────────────────── -->
  {#if musicPack}
    <section class="vk-card music" class:off={musicPack.installed && !musicPack.enabled}>
      <div class="music-row">
        <span class="music-glyph" aria-hidden="true"><Icon name="music" size={20} /></span>

        <div class="music-id">
          <h3 class="music-name">Music Pack</h3>
          <p class="vk-faint music-status">{musicStatus}</p>
        </div>

        <div class="music-actions">
          {#if musicPack.blocker}
            {#if !mod?.installed}
              <button class="vk-btn" onclick={() => run('install')} disabled={operations.busy}>
                <Icon name="download" size={14} />
                {t('mods.installModpackFirst')}
              </button>
            {/if}
          {:else}
            {#if !musicPack.installed || musicPack.updateAvailable || musicRunning}
              <button
                class="vk-btn vk-btn--primary"
                onclick={installMusicPack}
                disabled={operations.busy || musicBusy !== ''}
                title={operations.blockedBy('music-pack') ? waitingFor : undefined}
              >
                <Icon name="download" size={14} />
                {musicRunning
                  ? t('common.working')
                  : musicPack.installed
                    ? t('mods.updateTo', { version: musicPack.latestVersion })
                    : t('mods.musicInstall')}
              </button>
            {/if}

            {#if musicPack.installed}
              <Switch
                checked={musicPack.enabled}
                label={musicPack.enabled
                  ? `${t('mods.musicDisable')} — ${t('mods.musicOnHint')}`
                  : `${t('mods.musicEnable')} — ${t('mods.musicOffHint')}`}
                busy={musicBusy === 'toggle'}
                disabled={musicBusy !== '' || operations.busy}
                onchange={toggleMusicPack}
              />
              <MenuButton
                label={t('mods.music.menu')}
                items={[
                  {
                    label: musicBusy === 'uninstall' ? t('mods.wait') : t('common.remove'),
                    icon: 'trash',
                    danger: true,
                    disabled: musicBusy !== '' || operations.busy,
                    onselect: () => (confirmMusicRemoval = true)
                  }
                ]}
              />
            {/if}
          {/if}
        </div>
      </div>

      <TransferProgress kind="music-pack" onretry={installMusicPack} />

      {#if musicPack.updateAvailable && musicChangelog.length > 0}
        <details class="fold">
          <summary>{t('mods.changelogTitle', { version: musicPack.latestVersion })}</summary>
          <ul class="changelog">
            {#each musicChangelog as line, index (index)}
              <li>{line}</li>
            {/each}
          </ul>
        </details>
      {/if}
    </section>
  {/if}

  <!-- ── ADDON ───────────────────────────────────────────────────────── -->
  <div class="addons-head">
    <div class="tabs" role="tablist" aria-label={t('mods.tabsAria')}>
      <button
        role="tab"
        class:active={tab === 'addons'}
        aria-selected={tab === 'addons'}
        onclick={() => (tab = 'addons')}
      >
        <Icon name="package" size={14} />
        {t('mods.tab.installed')}
        {#if localAddons.length > 0}<span class="count">{localAddons.length}</span>{/if}
      </button>
      <button
        role="tab"
        class:active={tab === 'gamebanana'}
        aria-selected={tab === 'gamebanana'}
        onclick={() => (tab = 'gamebanana')}
      >
        <Icon name="external" size={14} />
        {t('mods.tab.discover')}
        {#if operations.busyWith('gamebanana')}
          <span class="count live">{Math.round(operations.percent ?? 0)}%</span>
        {/if}
      </button>
    </div>

    {#if tab === 'addons'}
      <div class="addons-actions">
        <button class="vk-btn" onclick={importAddon} disabled={addonBusy !== ''}>
          <Icon name="plus" size={14} />
          {addonBusy === 'import' ? t('mods.importing') : t('mods.importZip')}
        </button>
        <MenuButton
          label={t('mods.addonFolder')}
          items={[
            {
              label: t('mods.addonFolder'),
              icon: 'folder',
              onselect: () => void openFolder('addons')
            }
          ]}
        />
      </div>
    {/if}
  </div>

  {#if tab === 'addons'}
    {#if localAddons.length === 0}
      <button class="dropzone" onclick={importAddon} disabled={addonBusy !== ''}>
        <Icon name="package" size={26} />
        <span class="drop-title">{t('mods.noAddons')}</span>
        <span class="vk-faint drop-hint">{t('mods.noAddonsHint')}</span>
      </button>
      <button class="vk-btn discover" onclick={() => (tab = 'gamebanana')}>
        <Icon name="external" size={14} />
        {t('mods.browseGameBanana')}
      </button>
    {:else}
      <section class="vk-card addons-card">
        <ul class="addons">
          {#each localAddons as addon (addon.id)}
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
                <span class="addon-meta">
                  {#if addon.author}<span class="vk-faint">{addon.author}</span>{/if}
                  {#if addon.source === 'GameBanana'}
                    <span class="tag">GameBanana</span>
                  {/if}
                  {#if !addon.managed}
                    <span class="tag warn" title={t('mods.manualHint')}>{t('mods.manual')}</span>
                  {/if}
                </span>
              </div>

              {#if addonBusy === addon.id}
                <span class="working">
                  <span class="dot" aria-hidden="true"></span>{t('mods.wait')}
                </span>
              {/if}

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
              <MenuButton
                label={t('mods.addonMenu', { name: addon.name })}
                items={addonTools(addon)}
              />
            </li>
          {/each}
        </ul>
        <p class="vk-faint drop-more">{t('mods.dropMore')}</p>
      </section>
    {/if}

    {#if conflicts.length > 0}
      <details class="conflicts">
        <summary>
          <Icon name="warning" size={14} />
          {t('mods.conflicts', { count: conflicts.length })}
        </summary>
        <p class="vk-faint conflicts-hint">{t('mods.conflictsHint')}</p>
        <ul>
          {#each conflicts as conflict (conflict.fileName)}
            <li>
              <span class="vk-badge vk-badge--warning">{conflict.count}×</span>
              <strong>{conflict.fileName}</strong>
              <span class="vk-faint vk-mono">{conflict.locations.join(' · ')}</span>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  {:else}
    <GameBananaBrowser oninstalled={loadAddons} installed={installedGameBanana} />
  {/if}
</div>

{#if dropping}
  <div class="drop-overlay" aria-hidden="true">
    <div class="drop-card">
      <Icon name="download" size={30} />
      <p>{t('mods.dropOverlay')}</p>
    </div>
  </div>
{/if}

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
    padding-bottom: 12px;
  }

  /* ---- Modpack ---- */

  .modpack {
    padding: 22px 26px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    flex-wrap: wrap;
  }

  .identity .vk-eyebrow {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }

  .identity .vk-title {
    margin: 2px 0 0;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0 0;
    font-size: var(--vk-fs-small);
    font-weight: 700;
    color: var(--vk-text-secondary);
  }

  .status-dot {
    width: 9px;
    height: 9px;
    flex: none;
    border-radius: 50%;
    background: var(--vk-text-faint);
  }

  .status[data-tone='success'] .status-dot {
    background: var(--vk-success);
    box-shadow: 0 0 8px var(--vk-success);
  }

  .status[data-tone='warning'] {
    color: var(--vk-warning);
  }

  .status[data-tone='warning'] .status-dot {
    background: var(--vk-warning);
    box-shadow: 0 0 8px var(--vk-warning);
  }

  .status[data-tone='danger'] {
    color: var(--vk-danger);
  }

  .status[data-tone='danger'] .status-dot {
    background: var(--vk-danger);
    box-shadow: 0 0 8px var(--vk-danger);
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .main {
    min-width: 170px;
    padding: 11px 20px;
    font-size: var(--vk-fs-body);
  }

  .alert {
    margin: 14px 0 0;
    padding: 10px 14px;
    border: 1px solid color-mix(in srgb, var(--vk-danger) 45%, var(--vk-stroke));
    border-radius: var(--vk-radius-badge);
    background: color-mix(in srgb, var(--vk-danger) 8%, transparent);
    color: var(--vk-danger);
    font-size: var(--vk-fs-small);
  }

  .verify-bar {
    margin-top: 14px;
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

  .fold {
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
    padding: 16px 20px;
  }

  .music-row {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .music-glyph {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 12px;
    background: var(--vk-play-gradient);
    color: #fff;
    box-shadow: 0 0 14px rgb(255 0 102 / 0.3);
    transition: filter var(--vk-dur-fast) var(--vk-ease);
  }

  .music.off .music-glyph {
    filter: grayscale(1) brightness(0.7);
    box-shadow: none;
  }

  .music-id {
    flex: 1;
    min-width: 0;
  }

  .music-name {
    margin: 0;
    font-size: var(--vk-fs-card-title);
    font-weight: 900;
  }

  .music-status {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .music-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  /* ---- Addon ---- */

  .addons-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    margin-top: 6px;
  }

  .tabs {
    display: inline-flex;
    padding: 4px;
    border: 1px solid var(--vk-stroke);
    border-radius: 999px;
    background: var(--vk-input);
  }

  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 7px 16px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--vk-text-secondary);
    font: inherit;
    font-size: var(--vk-fs-small);
    font-weight: 800;
    cursor: pointer;
  }

  .tabs button.active {
    background: var(--vk-active-surface);
    color: var(--vk-text);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--vk-cyan) 40%, transparent);
  }

  .count {
    display: grid;
    place-items: center;
    min-width: 20px;
    height: 18px;
    padding: 0 6px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.1);
    font-size: var(--vk-fs-eyebrow);
    color: var(--vk-text);
  }

  .count.live {
    background: color-mix(in srgb, var(--vk-cyan) 25%, transparent);
  }

  .addons-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .addons-card {
    padding: 8px;
  }

  .addons {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .addon {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: var(--vk-radius-badge);
    transition: background var(--vk-dur-fast) var(--vk-ease);
  }

  .addon:hover {
    background: rgb(255 255 255 / 0.03);
  }

  .addon.off .addon-thumb,
  .addon.off .addon-info {
    opacity: 0.55;
  }

  .addon-thumb {
    width: 44px;
    height: 44px;
    flex: none;
    border-radius: 10px;
    object-fit: cover;
    background: var(--vk-input);
  }

  .addon-thumb.empty {
    display: grid;
    place-items: center;
    color: var(--vk-text-faint);
  }

  .addon-info {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .addon-info strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .addon-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: var(--vk-fs-micro);
  }

  .tag {
    padding: 1px 7px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.07);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
  }

  .tag.warn {
    background: color-mix(in srgb, var(--vk-warning) 14%, transparent);
    color: var(--vk-warning);
    cursor: help;
  }

  .working {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  /* Pulsazione: dice che l'operazione è viva anche senza percentuale. */
  .dot {
    width: 8px;
    height: 8px;
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

  .drop-more {
    margin: 8px 10px 4px;
    font-size: var(--vk-fs-eyebrow);
  }

  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 34px 20px;
    border: 2px dashed var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: transparent;
    color: var(--vk-text-secondary);
    cursor: pointer;
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .dropzone:hover:not(:disabled) {
    border-color: var(--vk-cyan);
    color: var(--vk-text);
  }

  .drop-title {
    font-size: var(--vk-fs-body);
    font-weight: 900;
    color: var(--vk-text);
  }

  .drop-hint {
    font-size: var(--vk-fs-micro);
  }

  .discover {
    align-self: center;
  }

  .conflicts {
    padding: 12px 16px;
    border: 1px solid color-mix(in srgb, var(--vk-warning) 40%, var(--vk-stroke));
    border-radius: var(--vk-radius-badge);
    background: color-mix(in srgb, var(--vk-warning) 6%, transparent);
  }

  .conflicts summary {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--vk-warning);
    font-size: var(--vk-fs-small);
  }

  .conflicts-hint {
    margin: 8px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .conflicts ul {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
    font-size: var(--vk-fs-micro);
  }

  .conflicts li {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  /* Un file sopra la finestra: tutta la pagina diventa la zona di rilascio. */
  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    background: rgb(4 7 14 / 0.7);
    backdrop-filter: blur(3px);
    pointer-events: none;
    animation: fade var(--vk-dur-fast) var(--vk-ease);
  }

  .drop-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 36px 48px;
    border: 2px dashed var(--vk-cyan);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel);
    color: var(--vk-cyan-soft);
    box-shadow: 0 0 30px rgb(0 242 255 / 0.25);
  }

  .drop-card p {
    margin: 0;
    font-size: var(--vk-fs-body);
    font-weight: 900;
    color: var(--vk-text);
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dot,
    .drop-overlay {
      animation: none;
    }
  }
</style>
