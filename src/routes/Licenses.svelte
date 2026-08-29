<script lang="ts">
  /**
   * Mii & Licenze.
   *
   * Due elenchi e basta: le licenze trovate nei salvataggi e i Mii del
   * database di Dolphin. Le azioni sui Mii non stanno su ogni card — si
   * seleziona un Mii e si agisce dalla barra, come faceva il `MiiCardsListBox`
   * del WPF. Backup, import/export e render dei Mii vivono in due modali:
   * servono di rado e in pagina rubavano solo spazio (§D-044).
   */
  import { onMount } from 'svelte';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';

  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import MiiEditor from './MiiEditor.svelte';
  import MiiAvatar from '$lib/components/MiiAvatar.svelte';
  import miiSilhouette from '$lib/assets/mii_silhouette.png';
  import { forget as forgetRenders } from '$lib/mii/render';
  import { app } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import type { LicenseView, MiiView, MiiRendererStatus, SaveOverview } from '$lib/api/types';

  let overview = $state<SaveOverview | null>(null);
  let licenses = $state<LicenseView[]>([]);
  let backups = $state<string[]>([]);
  let miis = $state<MiiView[]>([]);
  let renderer = $state<MiiRendererStatus | null>(null);

  let loading = $state(true);
  let busy = $state(false);
  let miiBusy = $state('');
  let rendererBusy = $state('');

  /** Il Mii su cui agisce la barra delle azioni. */
  let selectedId = $state<string | null>(null);

  /** `null` quando si crea un Mii nuovo, altrimenti l'id da modificare. */
  let editing = $state<string | null>(null);
  let editorOpen = $state(false);
  let pendingDelete = $state<MiiView | null>(null);

  /** Licenza a cui si sta cambiando il Mii, `null` quando il modale è chiuso. */
  let miiTarget = $state<LicenseView | null>(null);
  let applyingMii = $state('');

  /** Backup in attesa di conferma prima di essere rimesso in gioco. */
  let pendingRestore = $state<string | null>(null);
  let savesOpen = $state(false);
  let advancedOpen = $state(false);

  const canWrite = $derived(app.status?.saveWritesEnabled ?? false);
  const ready = $derived(overview?.userFolderConfigured ?? false);
  const filled = $derived(licenses.filter((license) => !license.isEmpty));
  const selected = $derived(miis.find((mii) => mii.id === selectedId) ?? null);

  /** I Mii che una licenza sta usando: si dice sulla tile, prima di eliminarli. */
  const inUse = $derived(
    new Set(filled.map((license) => license.miiId).filter((miiId) => miiId !== 0))
  );

  onMount(load);

  async function load() {
    loading = true;
    try {
      [overview, licenses, backups, miis, renderer] = await Promise.all([
        api.getSaveOverview(),
        api.listLicenses(),
        api.listSaveBackups(),
        api.listMiis(),
        api.getMiiRendererStatus()
      ]);
      if (!miis.some((mii) => mii.id === selectedId)) selectedId = null;
    } catch (error) {
      app.toast(t('lic.savesUnreadable'), api.errorMessage(error), 'warning');
    } finally {
      loading = false;
    }
  }

  async function reloadMiis() {
    try {
      miis = await api.listMiis();
      if (!miis.some((mii) => mii.id === selectedId)) selectedId = null;
    } catch (error) {
      app.toast(t('lic.miisUnreadable'), api.errorMessage(error), 'warning');
    }
  }

  function openEditor(id: string | null) {
    editing = id;
    editorOpen = true;
  }

  function closeEditor(changed: boolean) {
    editorOpen = false;
    editing = null;
    if (changed) void reloadMiis();
  }

  /** Esegue un'azione su un Mii e riallinea l'elenco. */
  async function withMii(id: string, action: () => Promise<unknown>, done: string) {
    miiBusy = id;
    try {
      await action();
      await reloadMiis();
      app.toast(t('lic.done'), done, 'success');
    } catch (error) {
      app.toast(t('home.operationFailed'), api.errorMessage(error), 'warning');
    } finally {
      miiBusy = '';
    }
  }

  async function importMii() {
    const source = await openDialog({
      multiple: false,
      directory: false,
      title: t('lic.pickMii'),
      filters: [
        { name: t('lic.miiFilter'), extensions: ['mii', 'miigx', 'mae', 'rcd', 'rsd'] },
        { name: t('lic.profileFilter'), extensions: ['json', 'vk-mii'] }
      ]
    });
    if (typeof source !== 'string') return;

    miiBusy = 'import';
    try {
      const imported = await api.importMii(source);
      await reloadMiis();
      selectedId = imported.id;
      app.toast(t('lic.miiImported'), t('lic.miiImportedBody', { name: imported.name }), 'success');
    } catch (error) {
      app.toast(t('lic.importFailed'), api.errorMessage(error), 'warning');
    } finally {
      miiBusy = '';
    }
  }

  async function exportMii(mii: MiiView) {
    const destination = await saveDialog({
      title: t('lic.exportMii'),
      defaultPath: `${mii.name || 'mii'}.mii`,
      filters: [{ name: t('lic.miiFilter'), extensions: ['mii'] }]
    });
    if (typeof destination !== 'string') return;

    miiBusy = mii.id;
    try {
      const written = await api.exportMii(mii.id, destination);
      app.toast(t('lic.miiExported'), written, 'success');
    } catch (error) {
      app.toast(t('lic.exportFailed'), api.errorMessage(error), 'warning');
    } finally {
      miiBusy = '';
    }
  }

  async function confirmDelete() {
    const target = pendingDelete;
    if (!target) return;

    pendingDelete = null;
    await withMii(
      target.id,
      () => api.deleteMii(target.id),
      t('lic.miiDeleted', { name: target.name })
    );
  }

  /**
   * Assegna un Mii del launcher alla licenza scelta.
   *
   * Come nel launcher WPF il backend fa due scritture: prima mette il Mii nel
   * database di Dolphin, poi lo indica dentro `rksys.dat`. Entrambi i file
   * vengono copiati e verificati prima di essere toccati.
   */
  async function applyMii(mii: MiiView) {
    const target = miiTarget;
    if (!target) return;

    applyingMii = mii.id;
    try {
      licenses = await api.setLicenseMii(target.saveIndex, target.slot, mii.id);
      miiTarget = null;
      backups = await api.listSaveBackups();
      overview = await api.getSaveOverview();
      app.toast(
        t('lic.miiAssigned'),
        t('lic.miiAssignedBody', { mii: mii.name, license: target.name }),
        'success'
      );
    } catch (error) {
      app.toast(t('lic.miiNotAssigned'), api.errorMessage(error), 'warning');
    } finally {
      applyingMii = '';
    }
  }

  async function copyFriendCode(code: string) {
    try {
      await navigator.clipboard.writeText(code);
      app.toast(t('debug.copied'), t('lic.fcCopied', { code }), 'success');
    } catch {
      app.toast(t('debug.copyFailed'), t('debug.copyFailedBody'), 'warning');
    }
  }

  // --- Salvataggi -----------------------------------------------------------

  async function backupSave() {
    busy = true;
    try {
      await api.backupSave();
      backups = await api.listSaveBackups();
      overview = await api.getSaveOverview();
      app.toast(t('lic.backupDone'), t('lic.backupDoneBody'), 'success');
    } catch (error) {
      app.toast(t('lic.backupFailed'), api.errorMessage(error), 'warning');
    } finally {
      busy = false;
    }
  }

  /** Sostituisce il salvataggio corrente con un file scelto dall'utente. */
  async function importSave() {
    const source = await openDialog({
      multiple: false,
      directory: false,
      title: t('lic.pickSave'),
      filters: [{ name: t('lic.saveFilter'), extensions: ['dat'] }]
    });
    if (typeof source !== 'string') return;

    busy = true;
    try {
      await api.importSave(source);
      await load();
      app.toast(t('lic.saveImported'), t('lic.saveImportedBody'), 'success');
    } catch (error) {
      app.toast(t('lic.importFailed'), api.errorMessage(error), 'warning');
    } finally {
      busy = false;
    }
  }

  /** Copia il salvataggio corrente fuori dal launcher. */
  async function exportSave() {
    const stamp = new Date().toISOString().slice(0, 19).replace(/[-:T]/g, '');
    const destination = await saveDialog({
      title: t('lic.exportSave'),
      defaultPath: `rksys_export_${stamp}.dat`,
      filters: [{ name: t('lic.saveFilter'), extensions: ['dat'] }]
    });
    if (typeof destination !== 'string') return;

    busy = true;
    try {
      const written = await api.exportSave(destination);
      app.toast(t('lic.saveExported'), written, 'success');
    } catch (error) {
      app.toast(t('lic.exportFailed'), api.errorMessage(error), 'warning');
    } finally {
      busy = false;
    }
  }

  /** Rimette in gioco il backup confermato. */
  async function confirmRestore() {
    const name = pendingRestore;
    if (!name) return;
    pendingRestore = null;

    busy = true;
    try {
      await api.restoreSaveBackup(name);
      await load();
      app.toast(t('lic.restored'), t('lic.restoredBody'), 'success');
    } catch (error) {
      app.toast(t('lic.restoreFailed'), api.errorMessage(error), 'warning');
    } finally {
      busy = false;
    }
  }

  // --- Render dei Mii -------------------------------------------------------

  async function withRenderer(action: string, run: () => Promise<MiiRendererStatus>) {
    rendererBusy = action;
    try {
      renderer = await run();
    } catch (error) {
      app.toast(t('home.operationFailed'), api.errorMessage(error), 'warning');
    } finally {
      rendererBusy = '';
    }
  }

  async function clearAvatars() {
    rendererBusy = 'clear';
    try {
      const removed = await api.clearMiiAvatars();
      forgetRenders();
      renderer = await api.getMiiRendererStatus();
      app.toast(t('lic.cacheCleared'), t('lic.cacheClearedBody', { count: removed }), 'success');
    } catch (error) {
      app.toast(t('home.operationFailed'), api.errorMessage(error), 'warning');
    } finally {
      rendererBusy = '';
    }
  }
</script>

<div class="page">
  <div class="bar">
    <div class="counts">
      <span><strong>{overview?.licenseCount ?? 0}</strong> {t('lic.countLicenses')}</span>
      <span><strong>{miis.length}</strong> {t('lic.countMiis')}</span>
      <span><strong>{backups.length}</strong> {t('lic.countBackups')}</span>
    </div>

    <div class="vk-row">
      <button class="vk-btn" onclick={load} disabled={loading || busy}>
        <Icon name="refresh" size={14} />
        {t('common.refresh')}
      </button>
      <button class="vk-btn" onclick={() => (savesOpen = true)} disabled={!ready}>
        <Icon name="save" size={14} />
        {t('lic.saves')}
      </button>
      <button
        class="vk-btn icon-only"
        title={t('lic.renderer')}
        aria-label={t('lic.renderer')}
        onclick={() => (advancedOpen = true)}
      >
        <Icon name="settings" size={16} />
      </button>
    </div>
  </div>

  {#if loading}
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
  {:else if !ready}
    <div class="vk-card vk-empty">
      <Icon name="license" size={28} />
      <p>{t('lic.needUserFolder')}</p>
      <button class="vk-btn" onclick={() => app.navigate('settings')}>
        {t('lic.goToSettings')}
      </button>
    </div>
  {:else}
    <section class="block">
      <p class="vk-eyebrow">{t('lic.licenses')}</p>

      {#if filled.length === 0}
        <div class="vk-card vk-empty">
          <img src={miiSilhouette} alt="" width="64" height="64" />
          <p>{t('lic.noLicenses')}</p>
          <p class="vk-faint">{t('lic.noLicensesHint')}</p>
        </div>
      {:else}
        <div class="licenses">
          {#each filled as license (`${license.savePath}-${license.slot}`)}
            <article class="license" style="--accent: {license.accentColor}">
              <header class="lic-head">
                <MiiAvatar
                  studioData={license.studioData}
                  initial={license.avatarInitial}
                  accent={license.accentColor}
                  name={license.miiName || license.name}
                  size={48}
                />

                <div class="lic-id">
                  <h3 class="lic-name">{license.name}</h3>
                  <p class="lic-meta">
                    <span class="slot">{t('friends.slot', { number: license.slot + 1 })}</span>
                    <span>{license.region}</span>
                    {#if license.miiName}<span class="mii-of">{license.miiName}</span>{/if}
                  </p>
                </div>

                {#if canWrite}
                  <!--
                    L'icona da sola non diceva a cosa serviva: la scritta sì
                    (§D-061).
                  -->
                  <button
                    class="swap-btn"
                    title={t('lic.swapMiiTitle')}
                    onclick={() => (miiTarget = license)}
                    disabled={miis.length === 0 || applyingMii !== ''}
                  >
                    <Icon name="swap" size={14} />
                    {t('lic.swapMii')}
                  </button>
                {/if}
              </header>

              <dl class="stats">
                <div>
                  <dt>VR</dt>
                  <dd>{license.vr}</dd>
                </div>
                <div>
                  <dt>BR</dt>
                  <dd>{license.br}</dd>
                </div>
                <div>
                  <dt>{t('lic.wins')}</dt>
                  <dd>{license.wins}</dd>
                </div>
                <div>
                  <dt>{t('lic.races')}</dt>
                  <dd>{license.races}</dd>
                </div>
                <div>
                  <dt>{t('lic.winRate')}</dt>
                  <dd>{(license.winRate * 100).toFixed(0)}%</dd>
                </div>
              </dl>

              {#if license.friendCode}
                <button
                  class="fc"
                  title={t('lic.copyFc')}
                  onclick={() => copyFriendCode(license.friendCode)}
                >
                  <span class="fc-tag">FC</span>
                  <span class="vk-mono fc-code">{license.friendCode}</span>
                  <Icon name="copy" size={13} />
                </button>
              {/if}
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <section class="block">
      <div class="block-head">
        <p class="vk-eyebrow">{t('lic.myMiis')}</p>
        <div class="vk-row">
          <button class="vk-btn" onclick={importMii} disabled={miiBusy !== ''}>
            <Icon name="download" size={14} />
            {t('lic.import')}
          </button>
          <button class="vk-btn vk-btn--primary" onclick={() => openEditor(null)}>
            <Icon name="plus" size={14} />
            {t('lic.newMii')}
          </button>
        </div>
      </div>

      {#if miis.length === 0}
        <div class="vk-card vk-empty">
          <img src={miiSilhouette} alt="" width="64" height="64" />
          <p>{t('lic.noMiis')}</p>
          <p class="vk-faint">{t('lic.noMiisHint')}</p>
        </div>
      {:else}
        <div class="actions" class:idle={!selected}>
          {#if selected}
            <div class="picked">
              <MiiAvatar
                studioData={selected.studioData}
                initial={selected.avatarInitial}
                accent={selected.favoriteColor}
                name={selected.name}
                size={28}
              />
              <span class="picked-name">{selected.name}</span>
              {#if selected.creatorName}
                <span class="vk-faint picked-by">
                  {t('lic.by', { name: selected.creatorName })}
                </span>
              {/if}
            </div>

            <div class="vk-row">
              <button
                class="vk-btn compact"
                onclick={() => openEditor(selected.id)}
                disabled={miiBusy !== ''}
              >
                <Icon name="edit" size={13} />
                {t('lic.edit')}
              </button>
              <button
                class="vk-btn compact"
                onclick={() =>
                  withMii(
                    selected.id,
                    () => api.duplicateMii(selected.id),
                    t('lic.duplicated', { name: selected.name })
                  )}
                disabled={miiBusy !== ''}
              >
                <Icon name="copy" size={13} />
                {t('lic.duplicate')}
              </button>
              <button
                class="vk-btn compact"
                onclick={() => exportMii(selected)}
                disabled={miiBusy !== ''}
              >
                <Icon name="external" size={13} />
                {t('lic.export')}
              </button>
              <button
                class="vk-btn vk-btn--danger compact"
                onclick={() => (pendingDelete = selected)}
                disabled={miiBusy !== ''}
              >
                <Icon name="trash" size={13} />
                {t('lic.delete')}
              </button>
            </div>
          {:else}
            <p class="hint">{t('lic.selectHint')}</p>
          {/if}
        </div>

        <div class="miis">
          {#each miis as mii (mii.id)}
            <button
              class="tile"
              class:selected={mii.id === selectedId}
              style="--accent: {mii.favoriteColor}"
              aria-pressed={mii.id === selectedId}
              title={mii.creatorName
                ? t('lic.tileTitle', { name: mii.name, creator: mii.creatorName })
                : mii.name}
              onclick={() => (selectedId = mii.id === selectedId ? null : mii.id)}
              ondblclick={() => openEditor(mii.id)}
            >
              <MiiAvatar
                studioData={mii.studioData}
                initial={mii.avatarInitial}
                accent={mii.favoriteColor}
                name={mii.name}
                size={64}
                shape="rounded"
              />
              <span class="tile-name">{mii.name}</span>
              {#if inUse.has(mii.miiId)}<span class="tile-tag">{t('lic.inUse')}</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </section>
  {/if}
</div>

{#if editorOpen}
  <MiiEditor miiId={editing} onclose={closeEditor} />
{/if}

<Modal
  open={savesOpen}
  title={t('lic.saves')}
  cancelLabel={t('common.close')}
  {busy}
  oncancel={() => (savesOpen = false)}
>
  <div class="vk-row saves-actions">
    <button class="vk-btn vk-btn--primary" onclick={backupSave} disabled={busy}>
      {t('lic.backupNow')}
    </button>
    <button class="vk-btn" onclick={exportSave} disabled={busy}>{t('lic.export')}</button>
    {#if canWrite}
      <button class="vk-btn" title={t('lic.importSaveTitle')} onclick={importSave} disabled={busy}>
        {t('lic.import')}
      </button>
    {/if}
    <button class="vk-btn" onclick={() => api.openFolder('backups')}>
      <Icon name="folder" size={14} />
      {t('lic.folder')}
    </button>
  </div>

  {#if backups.length === 0}
    <p class="vk-faint">{t('lic.noBackups')}</p>
  {:else}
    <ul class="backups">
      {#each backups as name (name)}
        <li>
          <span class="vk-mono">{name}</span>
          {#if canWrite}
            <button
              class="vk-btn compact"
              onclick={() => {
                savesOpen = false;
                pendingRestore = name;
              }}
              disabled={busy}
            >
              {t('lic.restore')}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</Modal>

<Modal
  open={advancedOpen}
  title={t('lic.renderer')}
  cancelLabel={t('common.close')}
  oncancel={() => (advancedOpen = false)}
>
  <div class="renderer">
    <div class="renderer-row">
      <div>
        <p class="renderer-title">
          {t('lic.gameRuntime')}
          {#if renderer?.runtimeInstalled}
            <span class="vk-badge vk-badge--success">{t('lic.installed')}</span>
          {/if}
        </p>
        <p class="vk-faint renderer-note">
          {t('lic.runtimeNote')}
          {#if !renderer?.runtimeInstalled && renderer?.runtimeHost}
            {t('lic.runtimeFrom', { host: renderer.runtimeHost })}
          {/if}
        </p>
      </div>

      {#if renderer?.runtimeInstalled}
        <button
          class="vk-btn vk-btn--danger compact"
          onclick={() => withRenderer('remove', api.removeMiiRenderer)}
          disabled={rendererBusy !== ''}
        >
          {t('common.remove')}
        </button>
      {:else}
        <button
          class="vk-btn vk-btn--primary compact"
          onclick={() => withRenderer('install', api.installMiiRenderer)}
          disabled={rendererBusy !== ''}
        >
          <Icon name="download" size={14} />
          {rendererBusy === 'install' ? t('gb.downloading') : t('mods.install')}
        </button>
      {/if}
    </div>

    <div class="renderer-row">
      <div>
        <p class="renderer-title">{t('lic.facesHere')}</p>
        <p class="vk-faint renderer-note">
          {t('lic.facesNote', {
            host: renderer?.renderHost ?? 'Mii Studio',
            count: renderer?.cachedAvatars ?? 0
          })}
        </p>
      </div>

      {#if renderer?.cachedAvatars}
        <button class="vk-btn compact" onclick={clearAvatars} disabled={rendererBusy !== ''}>
          {t('lic.clearCache')}
        </button>
      {/if}
    </div>
  </div>
</Modal>

<Modal
  open={pendingRestore !== null}
  title={t('lic.restoreTitle')}
  confirmLabel={t('lic.restore')}
  cancelLabel={t('common.cancel')}
  {busy}
  onconfirm={confirmRestore}
  oncancel={() => (pendingRestore = null)}
>
  <p>
    <span class="vk-mono">{pendingRestore}</span>
    {t('lic.restoreBody')}
  </p>
</Modal>

<Modal
  open={miiTarget !== null}
  title={miiTarget ? t('lic.miiOf', { name: miiTarget.name }) : t('lic.licenseMii')}
  cancelLabel={t('common.cancel')}
  busy={applyingMii !== ''}
  oncancel={() => (miiTarget = null)}
>
  <div class="picker">
    {#each miis as mii (mii.id)}
      <button
        class="picker-item"
        style="--accent: {mii.favoriteColor}"
        onclick={() => applyMii(mii)}
        disabled={applyingMii !== ''}
      >
        <MiiAvatar
          studioData={mii.studioData}
          initial={mii.avatarInitial}
          accent={mii.favoriteColor}
          name={mii.name}
          size={36}
        />
        <span class="picker-name">
          <strong>{mii.name}</strong>
          {#if mii.creatorName}
            <span class="vk-faint">{t('lic.by', { name: mii.creatorName })}</span>
          {/if}
        </span>
        {#if miiTarget && mii.miiId === miiTarget.miiId}
          <span class="vk-badge vk-badge--success">{t('lic.current')}</span>
        {:else if applyingMii === mii.id}
          <span class="vk-faint">{t('lic.writing')}</span>
        {/if}
      </button>
    {/each}
  </div>
</Modal>

<Modal
  open={pendingDelete !== null}
  title={t('lic.deleteTitle')}
  confirmLabel={t('lic.delete')}
  cancelLabel={t('common.cancel')}
  danger
  onconfirm={confirmDelete}
  oncancel={() => (pendingDelete = null)}
>
  <p>{t('lic.deleteBody', { name: pendingDelete?.name ?? '' })}</p>
  {#if pendingDelete && inUse.has(pendingDelete.miiId)}
    <p class="vk-faint">{t('lic.deleteInUse')}</p>
  {/if}
</Modal>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 20px;
    max-width: 920px;
    margin: 0 auto;
    padding-bottom: 12px;
  }

  /* Barra di pagina: nessuna card, il titolo lo dà già l'header dell'app. */
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .counts {
    display: flex;
    gap: 18px;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .counts strong {
    font-size: 15px;
    font-weight: 900;
    color: var(--vk-text);
  }

  .icon-only {
    padding: 9px 10px;
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .block-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .skeleton {
    height: 120px;
  }

  /* ---- Licenze ---- */

  .licenses {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(380px, 1fr));
    gap: 12px;
  }

  .license {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border: 1px solid var(--vk-stroke);
    border-left: 3px solid var(--accent);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel-glass);
    box-shadow: var(--vk-shadow-card);
  }

  .lic-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .lic-id {
    min-width: 0;
    flex: 1;
  }

  .lic-name {
    margin: 0;
    font-size: var(--vk-fs-card-title);
    font-weight: 900;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .lic-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 3px 0 0;
    font-size: var(--vk-fs-eyebrow);
    color: var(--vk-text-faint);
    overflow: hidden;
    white-space: nowrap;
  }

  .lic-meta span + span::before {
    content: '·';
    margin-right: 6px;
  }

  .lic-meta .slot {
    color: var(--accent);
    font-weight: 800;
  }

  .lic-meta .mii-of {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .swap-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
    padding: 7px 11px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-pill);
    background: transparent;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    white-space: nowrap;
    transition:
      border-color var(--vk-dur-fast) var(--vk-ease),
      background var(--vk-dur-fast) var(--vk-ease),
      color var(--vk-dur-fast) var(--vk-ease);
  }

  .swap-btn:hover:not(:disabled) {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--vk-text);
  }

  .swap-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* Cinque numeri allineati: prima il valore, l'etichetta sotto in piccolo. */
  .stats {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 2px;
    margin: 0;
    padding: 10px 0;
    border-top: 1px solid var(--vk-stroke);
    border-bottom: 1px solid var(--vk-stroke);
    text-align: center;
  }

  .stats div {
    display: flex;
    flex-direction: column-reverse;
    gap: 2px;
    min-width: 0;
  }

  .stats dt {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--vk-text-faint);
  }

  .stats dd {
    margin: 0;
    font-size: 15px;
    font-weight: 900;
    color: var(--vk-text);
  }

  .fc {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-input);
    color: var(--vk-text-secondary);
  }

  .fc:hover {
    border-color: var(--vk-cyan);
    color: var(--vk-text);
  }

  .fc-tag {
    font-size: 10px;
    font-weight: 900;
    letter-spacing: 0.06em;
    color: var(--vk-cyan-soft);
  }

  .fc-code {
    flex: 1;
    text-align: left;
    color: var(--vk-cyan-soft);
    font-weight: 700;
  }

  /* ---- Mii ---- */

  /* Una barra sola per tutte le azioni: le tile restano pulite. */
  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    min-height: 52px;
    padding: 8px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
  }

  .actions.idle {
    border-style: dashed;
    background: transparent;
  }

  .hint {
    margin: 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-faint);
  }

  .picked {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .picked-name {
    font-weight: 800;
    overflow-wrap: anywhere;
  }

  .picked-by {
    font-size: var(--vk-fs-eyebrow);
  }

  .compact {
    padding: 7px 11px;
    font-size: var(--vk-fs-micro);
    gap: 6px;
  }

  .miis {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: 10px;
  }

  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 10px 8px;
    border: 1.5px solid color-mix(in srgb, var(--accent) 32%, var(--vk-stroke));
    border-radius: 14px;
    background: var(--vk-panel-soft);
    color: inherit;
    cursor: pointer;
    transition:
      border-color var(--vk-dur-fast) var(--vk-ease),
      transform var(--vk-dur-fast) var(--vk-ease),
      box-shadow var(--vk-dur-fast) var(--vk-ease);
  }

  .tile:hover {
    transform: translateY(-2px);
    border-color: var(--accent);
  }

  .tile.selected {
    border-color: var(--accent);
    box-shadow:
      0 0 0 1px var(--accent) inset,
      0 0 20px color-mix(in srgb, var(--accent) 35%, transparent);
  }

  .tile-name {
    max-width: 100%;
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tile-tag {
    position: absolute;
    top: 6px;
    right: 6px;
    padding: 1px 6px;
    border-radius: var(--vk-radius-pill);
    background: color-mix(in srgb, var(--accent) 30%, var(--vk-input));
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--vk-text);
  }

  /* ---- Modali ---- */

  .saves-actions {
    margin-bottom: 14px;
    flex-wrap: wrap;
  }

  .backups {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    max-height: 280px;
    overflow-y: auto;
    list-style: none;
  }

  .backups li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 10px;
    border: 1px solid var(--vk-stroke);
    border-radius: 10px;
  }

  .picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 340px;
    overflow-y: auto;
  }

  .picker-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--vk-stroke));
    border-radius: 12px;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .picker-item:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .picker-item:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .picker-name {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .renderer {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .renderer-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .renderer-title {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-weight: 800;
  }

  .renderer-note {
    margin: 4px 0 0;
    max-width: 46ch;
    font-size: var(--vk-fs-micro);
    line-height: 1.5;
  }
</style>
