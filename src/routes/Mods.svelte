<script lang="ts">
  /**
   * Mods.
   *
   * Ricalca il `ModsView` del WPF e ne stringe il disordine: la card della
   * modpack, quella del music pack, e sotto **due schede** — gli addon
   * installati e GameBanana — che nel legacy sono i due pulsanti larghi
   * `InstalledAddonsTabButton` / `GameBananaTabButton`.
   *
   * Il canale di rilascio non sta qui: si sceglie una volta, in Impostazioni →
   * Percorsi, e non ha ragione di occupare spazio in una pagina che si usa a
   * ogni aggiornamento (vedi `docs/decisions.md` §D-039).
   */
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWebview } from '@tauri-apps/api/webview';

  import * as api from '$lib/api';
  import GameBananaBrowser from '$lib/components/GameBananaBrowser.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import Switch from '$lib/components/Switch.svelte';
  import { app } from '$lib/stores/app.svelte';
  import type { AddonView, ConflictView, IntegrityReport, MusicPackStatus } from '$lib/api/types';

  type Tab = 'addons' | 'gamebanana';

  let tab = $state<Tab>('addons');
  let installing = $state(false);
  let verifying = $state(false);
  let checking = $state(false);
  /** Un archivio sta passando sopra la finestra: la zona di rilascio si accende. */
  let dropping = $state(false);
  let integrity = $state<IntegrityReport | null>(null);
  let conflicts = $state<ConflictView[]>([]);
  let addons = $state<AddonView[]>([]);
  let addonBusy = $state('');
  let musicPack = $state<MusicPackStatus | null>(null);
  let musicBusy = $state('');
  /** Anteprime che il server non ha servito: al loro posto la sagoma. */
  let brokenPreviews = $state<string[]>([]);

  /**
   * Cosa sta succedendo, e com'e' finito.
   *
   * Il legacy teneva una riga di stato sempre visibile sotto i pulsanti: senza,
   * premere "Verifica" cambia solo l'etichetta di un pulsante e l'operazione
   * sembra non essere partita.
   */
  type Activity = { tone: 'busy' | 'ok' | 'warn'; text: string };
  let activity = $state<Activity | null>(null);

  const mod = $derived(app.modState);
  const percent = $derived(app.progress.percent ?? 0);
  const running = $derived(installing && app.progress.phase !== 'Idle');

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

  const shownAddons = $derived(
    source === 'all' || !showSourceFilter
      ? localAddons
      : localAddons.filter((addon) => addon.source === source)
  );

  const health = $derived(
    !mod?.installed
      ? { tone: 'vk-badge--danger', label: 'Non installata' }
      : mod.needsRepair
        ? { tone: 'vk-badge--danger', label: 'Da riparare' }
        : mod.updateAvailable
          ? { tone: 'vk-badge--warning', label: 'Aggiornamento' }
          : { tone: 'vk-badge--success', label: 'Aggiornata' }
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

  /** Ricontrolla il manifest remoto senza installare niente. */
  async function checkUpdates() {
    checking = true;
    try {
      await api.checkUpdates();
      await app.refresh();
      const state = app.modState;
      app.toast(
        'Controllo aggiornamenti',
        state?.updateAvailable
          ? `Disponibile la versione ${state.latestVersion}.`
          : 'La modpack è già aggiornata.',
        state?.updateAvailable ? 'info' : 'success'
      );
    } catch (error) {
      app.toast('Controllo non riuscito', api.errorMessage(error), 'warning');
    } finally {
      checking = false;
    }
  }

  /** Apre una cartella e riporta l'errore invece di ingoiarlo. */
  async function openFolder(key: 'mod' | 'addons') {
    try {
      await api.openFolder(key);
    } catch (error) {
      app.toast('Cartella non apribile', api.errorMessage(error), 'warning');
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

  async function withMusic(action: string, run: () => Promise<unknown>, done?: string) {
    if (installing || musicBusy) return;
    musicBusy = action;
    try {
      await run();
      await loadAddons();
      if (done) app.toast('VanzaKart Music Pack', done, 'success');
    } catch (error) {
      app.toast('Operazione non riuscita', api.errorMessage(error), 'warning');
    } finally {
      musicBusy = '';
      if (action === 'install') app.resetProgress();
    }
  }

  async function installMusicPack() {
    app.resetProgress();
    await withMusic('install', async () => {
      const outcome = await api.installMusicPack();
      app.toast('VanzaKart Music Pack', outcome.summary, 'success');
    });
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
      app.toast('Niente da importare', 'Gli addon sono archivi .zip.', 'warning');
      return;
    }

    addonBusy = 'import';
    let imported = 0;
    try {
      for (const archive of archives) {
        try {
          const addon = await api.importAddon(archive, suggestedName(archive));
          imported += 1;
          app.toast('Addon importato', `${addon.name}: ${addon.fileCount} file.`, 'success');
        } catch (error) {
          app.toast('Import non riuscito', api.errorMessage(error), 'warning');
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
      title: 'Seleziona uno o più archivi addon',
      filters: [{ name: 'Archivi ZIP', extensions: ['zip'] }]
    });
    if (selected === null) return;

    await importArchives(Array.isArray(selected) ? selected : [selected]);
  }

  async function withAddon(addon: AddonView, run: () => Promise<unknown>, done?: string) {
    addonBusy = addon.id;
    try {
      await run();
      await loadAddons();
      if (done) app.toast('Addon', done, 'success');
    } catch (error) {
      app.toast('Operazione non riuscita', api.errorMessage(error), 'warning');
    } finally {
      addonBusy = '';
    }
  }

  async function run(action: 'install' | 'repair') {
    if (installing) return;
    installing = true;
    integrity = null;
    activity = {
      tone: 'busy',
      text: action === 'install' ? 'Preparazione…' : 'Riparazione in corso…'
    };
    app.resetProgress();

    try {
      const outcome = action === 'install' ? await api.installMods() : await api.repairMods();
      activity = { tone: outcome.warnings.length > 0 ? 'warn' : 'ok', text: outcome.summary };
      for (const warning of outcome.warnings) app.toast('Avviso', warning, 'warning');
      await app.refresh();
      await loadAddons();
    } catch (error) {
      const message = api.errorMessage(error);
      activity = { tone: 'warn', text: message };
      app.toast('Operazione non riuscita', message, 'danger');
    } finally {
      installing = false;
    }
  }

  async function verify() {
    verifying = true;
    integrity = null;
    activity = { tone: 'busy', text: 'Confronto dei file con il manifest…' };
    try {
      integrity = await api.verifyMods();
      activity = {
        tone: integrity.mismatched.length > 0 ? 'warn' : 'ok',
        text: integrity.message
      };
    } catch (error) {
      const message = api.errorMessage(error);
      activity = { tone: 'warn', text: message };
      app.toast('Verifica non riuscita', message, 'warning');
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
          <span class="vk-eyebrow">Installata</span>
          <strong>{mod?.installedVersion || '—'}</strong>
        </div>
        <span class="arrow" class:pending={mod?.updateAvailable} aria-hidden="true">
          {mod?.updateAvailable ? '→' : '·'}
        </span>
        <div class="version" class:next={mod?.updateAvailable}>
          <span class="vk-eyebrow">Ultima</span>
          <strong>{mod?.latestVersion || '—'}</strong>
        </div>
      </div>
    </header>

    {#if mod?.needsRepair}
      <p class="alert">
        {mod.repairReason}. Finché non ripari, Dolphin avvia Mario Kart Wii originale.
      </p>
    {/if}

    <div class="actions">
      <button
        class="vk-btn vk-btn--primary main"
        onclick={() => run('install')}
        disabled={installing || verifying}
      >
        <Icon name="download" size={15} />
        {installing
          ? 'In corso…'
          : !mod?.installed
            ? 'Installa'
            : mod.updateAvailable || mod.needsRepair
              ? 'Aggiorna'
              : 'Reinstalla'}
      </button>

      <div class="secondary">
        <button
          class="vk-btn"
          onclick={checkUpdates}
          disabled={installing || verifying || checking}
        >
          <Icon name="refresh" size={14} />
          {checking ? 'Controllo…' : 'Aggiornamenti'}
        </button>
        <button
          class="vk-btn"
          onclick={() => run('repair')}
          disabled={installing || verifying || !mod?.installed}
        >
          <Icon name="repair" size={14} />
          Ripara
        </button>
        <button
          class="vk-btn"
          onclick={verify}
          disabled={verifying || installing || !mod?.installed}
        >
          <Icon name="check" size={14} />
          {verifying ? 'Verifica…' : 'Verifica'}
        </button>
        <button
          class="vk-btn"
          title="Apri la cartella della modpack"
          onclick={() => openFolder('mod')}
          disabled={installing}
        >
          <Icon name="folder" size={14} />
          Cartella mod
        </button>
      </div>
    </div>

    {#if running}
      <div class="progress-block">
        <div class="vk-progress" class:vk-progress--indeterminate={app.progress.percent === null}>
          <div class="vk-progress__fill" style="width: {percent}%"></div>
        </div>
        <div class="progress-meta">
          <span>{app.progress.phase} — {app.progress.detail}</span>
          <span class="vk-spacer"></span>
          {#if app.progress.filesTotal > 0}
            <span class="vk-faint">{app.progress.filesDone}/{app.progress.filesTotal} file</span>
          {/if}
          {#if app.progress.bytesLabel}
            <span class="vk-faint">{app.progress.bytesLabel}</span>
          {/if}
          {#if app.progress.speedLabel}
            <span class="speed">{app.progress.speedLabel}</span>
          {/if}
          <button class="vk-btn vk-btn--ghost small" onclick={() => api.cancelOperation()}>
            Annulla
          </button>
        </div>
      </div>
    {:else if verifying}
      <div class="progress-block">
        <div class="vk-progress vk-progress--indeterminate">
          <div class="vk-progress__fill"></div>
        </div>
      </div>
    {/if}

    {#if activity && !running}
      <div class="activity" data-tone={activity.tone}>
        {#if activity.tone === 'busy'}
          <span class="dot busy" aria-hidden="true"></span>
        {:else}
          <Icon name={activity.tone === 'ok' ? 'check' : 'warning'} size={14} />
        {/if}
        <span>{activity.text}</span>
        {#if activity.tone !== 'busy'}
          <button
            class="vk-btn vk-btn--ghost small dismiss"
            aria-label="Nascondi"
            onclick={() => {
              activity = null;
              integrity = null;
            }}
          >
            ✕
          </button>
        {/if}
      </div>
    {/if}

    {#if integrity && integrity.mismatched.length > 0}
      <details class="report">
        <summary>{integrity.mismatched.length} file da ripristinare</summary>
        <ul class="file-list vk-mono">
          {#each integrity.mismatched.slice(0, 60) as path (path)}
            <li>{path}</li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if mod?.changelog?.length}
      <details class="changelog-block">
        <summary>Novità della {mod.latestVersion || 'versione disponibile'}</summary>
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
    <div class="music-id">
      <p class="music-name">
        VanzaKart Music Pack
        {#if musicPack?.installed}
          <span class="vk-badge {musicPack.enabled ? 'vk-badge--success' : ''}">
            {musicPack.enabled ? 'Attivo' : 'Disattivo'}
          </span>
        {/if}
      </p>
      {#if musicPack?.blocker}
        <p class="vk-faint music-note">{musicPack.blocker}</p>
      {:else}
        <p class="vk-faint music-note">
          {musicPack?.installedVersion || 'Non installato'}
          {#if musicPack?.updateAvailable && musicPack.latestVersion}
            → {musicPack.latestVersion}
          {/if}
          {#if musicPack?.installed}
            · {musicPack.fileCount} tracce
          {/if}
        </p>
      {/if}
    </div>

    {#if !musicPack?.blocker}
      <div class="music-actions">
        {#if musicBusy === 'install' && running}
          <div class="vk-progress music-progress">
            <div class="vk-progress__fill" style="width: {percent}%"></div>
          </div>
        {:else}
          {#if !musicPack?.installed || musicPack.updateAvailable}
            <button
              class="vk-btn vk-btn--primary"
              onclick={installMusicPack}
              disabled={installing || musicBusy !== ''}
            >
              <Icon name="download" size={14} />
              {musicPack?.installed ? 'Aggiorna' : 'Installa'}
            </button>
          {/if}

          {#if musicPack?.installed}
            <Switch
              checked={musicPack.enabled}
              label={musicPack.enabled ? 'Disattiva il music pack' : 'Attiva il music pack'}
              busy={musicBusy === 'toggle'}
              disabled={musicBusy !== ''}
              onchange={() =>
                withMusic('toggle', async () => {
                  musicPack = await api.setMusicPackEnabled(!musicPack!.enabled);
                })}
            />
            <button
              class="vk-btn vk-btn--danger"
              onclick={() =>
                withMusic(
                  'uninstall',
                  async () => {
                    musicPack = await api.uninstallMusicPack();
                  },
                  'Tracce originali ripristinate.'
                )}
              disabled={musicBusy !== ''}
            >
              Rimuovi
            </button>
          {/if}
        {/if}
      </div>
    {/if}
  </section>

  <!-- ── SCHEDE ──────────────────────────────────────────────────────── -->
  <nav class="tabs" aria-label="Addon">
    <button class="tab" class:active={tab === 'addons'} onclick={() => (tab = 'addons')}>
      <Icon name="package" size={15} />
      Addon installati
      {#if localAddons.length > 0}<span class="count">{localAddons.length}</span>{/if}
    </button>
    <button class="tab" class:active={tab === 'gamebanana'} onclick={() => (tab = 'gamebanana')}>
      <Icon name="external" size={15} />
      GameBanana
    </button>
  </nav>

  {#if tab === 'addons'}
    <section class="vk-card">
      <div class="section-head">
        <p class="vk-eyebrow">Addon installati</p>
        <div class="vk-row">
          <button
            class="vk-btn"
            title="Apri la cartella degli addon"
            onclick={() => openFolder('addons')}
          >
            <Icon name="folder" size={14} />
            Cartella addon
          </button>
          <button class="vk-btn vk-btn--primary" onclick={importAddon} disabled={addonBusy !== ''}>
            <Icon name="download" size={14} />
            {addonBusy === 'import' ? 'Importo…' : 'Importa .zip'}
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
        <span class="drop-title">{dropping ? 'Rilascia qui' : 'Trascina qui i tuoi addon'}</span>
        <span class="vk-faint drop-hint">archivi .zip, anche più d'uno alla volta</span>
      </button>

      {#if localAddons.length === 0}
        <div class="vk-empty">
          <Icon name="package" size={28} />
          <p>Nessun addon installato.</p>
          <p class="vk-faint">Trascinane uno qui sopra, o prendine uno da GameBanana.</p>
        </div>
      {:else}
        {#if showSourceFilter}
          <div class="filters">
            {#each [['all', 'Tutti'], ['Local', 'Importati'], ['GameBanana', 'GameBanana']] as const as [value, label] (value)}
              <button class="chip" class:active={source === value} onclick={() => (source = value)}>
                {label}
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
                  {addon.author || addon.source} · {addon.fileCount} file{addon.managed
                    ? ''
                    : ' · non gestito'}
                </span>
              </div>

              <!-- Prima lo stato, poi le azioni: la levetta è ciò che si
                   guarda scorrendo l'elenco, non un pulsante fra i pulsanti. -->
              <Switch
                checked={addon.enabled}
                label={addon.enabled ? `Disattiva ${addon.name}` : `Attiva ${addon.name}`}
                disabled={!addon.managed || (addonBusy !== '' && addonBusy !== addon.id)}
                busy={addonBusy === addon.id}
                onchange={() =>
                  withAddon(addon, () => api.setAddonEnabled(addon.id, !addon.enabled))}
              />

              {#if addonBusy === addon.id}
                <span class="working"
                  ><span class="dot busy" aria-hidden="true"></span>Attendi…</span
                >
              {:else}
                {#if addon.sourceUrl}
                  <button
                    class="vk-btn icon-btn"
                    title="Apri la pagina della mod su GameBanana"
                    aria-label="Apri la pagina della mod su GameBanana"
                    onclick={() => api.openExternal(addon.sourceUrl)}
                  >
                    <Icon name="external" size={14} />
                  </button>
                {/if}
                <button
                  class="vk-btn vk-btn--danger icon-btn"
                  title="Rimuovi l'addon"
                  aria-label="Rimuovi l'addon"
                  onclick={() =>
                    withAddon(addon, () => api.removeAddon(addon.id), `${addon.name} rimosso.`)}
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
          <p class="vk-eyebrow">
            {conflicts.length} conflitti
          </p>
          <p class="vk-subtitle">
            Questi file compaiono più volte: Riivolution ne applica uno solo.
          </p>
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
    gap: 8px;
    flex-wrap: wrap;
  }

  .progress-block {
    margin-top: 16px;
  }

  .progress-meta {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  /* La velocità è l'unica cifra che si guarda mentre si aspetta: si stacca. */
  .speed {
    color: var(--vk-cyan-soft);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
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
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    padding: 16px 26px;
  }

  .music-name {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-size: var(--vk-fs-card-title);
    font-weight: 900;
  }

  .music-note {
    margin: 3px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .music-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .music-progress {
    width: 220px;
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
    .modpack-head,
    .music {
      align-items: flex-start;
      flex-direction: column;
    }
  }
</style>
