<script lang="ts">
  /**
   * Avviso di aggiornamento all'avvio.
   *
   * La home mostra già le due card — quella della modpack e quella del
   * launcher — ma chi apre il launcher per giocare preme PLAY e non le guarda.
   * Questo è lo stesso stato, detto una volta sola all'apertura e in un modo
   * che non si può non vedere (§D-075).
   *
   * Non si ripete: chiuso una volta, per quella sessione non torna.
   */
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';

  const launcher = $derived(app.launcherUpdate?.available ? app.launcherUpdate : null);

  /** La modpack conta solo se il controllo è stato fatto davvero. */
  const modpack = $derived(
    app.modState?.checked && app.modState.installed && app.modState.updateAvailable
      ? app.modState
      : null
  );

  const open = $derived(!app.updateNoticeDismissed && (launcher !== null || modpack !== null));

  const title = $derived(
    launcher && modpack ? t('notice.both') : launcher ? t('notice.launcher') : t('notice.modpack')
  );

  function later() {
    app.updateNoticeDismissed = true;
  }

  /** Il launcher per primo: aggiornarlo può cambiare come installa la modpack. */
  function updateNow() {
    app.updateNoticeDismissed = true;
    if (launcher) {
      app.updaterOpen = true;
    } else {
      app.navigate('mods');
    }
  }

  function goToMods() {
    app.updateNoticeDismissed = true;
    app.navigate('mods');
  }
</script>

<Modal
  {open}
  {title}
  confirmLabel={launcher ? t('home.launcherUpdateAction') : t('notice.goToModpack')}
  cancelLabel={t('notice.later')}
  onconfirm={updateNow}
  oncancel={later}
>
  <ul class="items">
    {#if launcher}
      <li class="item">
        <Icon name="download" size={16} />
        <div>
          <p class="what">{t('notice.launcherItem')}</p>
          <p class="vk-faint versions">
            <span class="vk-mono">{launcher.current}</span>
            <span aria-hidden="true">→</span>
            <span class="vk-mono next">{launcher.latest}</span>
          </p>
        </div>
      </li>
    {/if}

    {#if modpack}
      <li class="item">
        <Icon name="package" size={16} />
        <div>
          <p class="what">{t('notice.modpackItem', { channel: modpack.channel })}</p>
          <p class="vk-faint versions">
            <span class="vk-mono">
              {modpack.installedVersion || t('notice.unknownVersion')}
            </span>
            <span aria-hidden="true">→</span>
            <span class="vk-mono next">{modpack.latestVersion}</span>
          </p>
        </div>
      </li>
    {/if}
  </ul>

  {#if launcher && modpack}
    <p class="vk-subtitle order">
      Conviene partire dal launcher: la modpack si aggiorna dopo, dalla sua pagina.
    </p>
    <button class="vk-btn secondary" onclick={goToMods}>Vai invece alla modpack</button>
  {/if}
</Modal>

<style>
  .items {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
  }

  .what {
    margin: 0;
    font-weight: 800;
  }

  .versions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .next {
    color: var(--vk-cyan-soft);
    font-weight: 700;
  }

  .order {
    margin: 12px 0 10px;
  }

  .secondary {
    width: 100%;
  }
</style>
