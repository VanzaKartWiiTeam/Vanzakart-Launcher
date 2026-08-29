<script lang="ts">
  /**
   * Barra del titolo custom, come il `WindowChrome` di MainWindow.xaml:
   * logo, wordmark VANZAKART, badge versione, badge stato, minimize/close.
   */
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import Icon from './Icon.svelte';
  import logo from '$lib/assets/logo.png';
  import { app } from '$lib/stores/app.svelte';
  import { t } from '$lib/stores/i18n.svelte';

  const window = getCurrentWindow();

  // Su Linux la finestra usa le decorazioni native (docs/ui-parity.md U-02/U-03).
  const isLinux = navigator.userAgent.includes('Linux');

  /**
   * Il pulsante centrale cambia glifo come in qualunque finestra: riquadro
   * singolo quando c'è da ingrandire, riquadri sovrapposti quando c'è da
   * tornare alla dimensione precedente.
   */
  let maximized = $state(false);

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      maximized = await window.isMaximized();
      // `onResized` copre tutto: pulsante, doppio clic sulla barra, Win+↑,
      // trascinamento in alto, snap laterale.
      const stop = await window.onResized(async () => {
        if (!disposed) maximized = await window.isMaximized();
      });
      if (disposed) stop();
      else unlisten = stop;
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  const toneClass = $derived(
    {
      info: '',
      success: 'vk-badge--success',
      warning: 'vk-badge--warning',
      danger: 'vk-badge--danger'
    }[app.statusTone]
  );

  async function minimize() {
    await window.minimize();
  }

  async function toggleMaximize() {
    await window.toggleMaximize();
  }

  async function close() {
    await window.close();
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img src={logo} alt="" width="28" height="28" />
    <span class="wordmark">VANZAKART</span>

    <span class="vk-badge chip">
      {t('titlebar.version', { version: app.status?.launcherVersion ?? '—' })}
    </span>
    <span class="vk-badge chip {toneClass}"
      >{app.statusTone === 'info' ? t('titlebar.ready') : app.statusLine.slice(0, 46)}</span
    >
  </div>

  {#if !isLinux}
    <div class="controls">
      <button class="chrome" onclick={minimize} aria-label={t('titlebar.minimize')}>
        <Icon name="minimize" size={14} />
      </button>
      <button
        class="chrome"
        onclick={toggleMaximize}
        aria-label={maximized ? t('titlebar.restore') : t('titlebar.maximize')}
        title={maximized ? t('titlebar.restore') : t('titlebar.maximize')}
      >
        <Icon name={maximized ? 'restore' : 'maximize'} size={13} />
      </button>
      <button class="chrome chrome--close" onclick={close} aria-label={t('titlebar.close')}>
        <Icon name="close" size={13} />
      </button>
    </div>
  {/if}
</header>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--vk-titlebar-h);
    padding: 0 8px 0 18px;
    background: var(--vk-titlebar-bg);
    flex: none;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .brand img {
    object-fit: contain;
  }

  .wordmark {
    font-size: var(--vk-fs-body);
    font-weight: 900;
    letter-spacing: 0.05em;
  }

  .chip {
    font-weight: 600;
    color: var(--vk-text-secondary);
    max-width: 46ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip:first-of-type {
    margin-left: 2px;
  }

  .controls {
    display: flex;
    gap: 2px;
  }

  .chrome {
    display: grid;
    place-items: center;
    width: 42px;
    height: 32px;
    border: none;
    border-radius: var(--vk-radius-badge);
    background: transparent;
    color: var(--vk-text-secondary);
    transition:
      background var(--vk-dur-fast) var(--vk-ease),
      color var(--vk-dur-fast) var(--vk-ease);
  }

  .chrome:hover {
    background: rgb(255 255 255 / 0.08);
    color: var(--vk-text);
  }

  .chrome--close:hover {
    background: rgb(255 107 130 / 0.22);
    color: #fff;
  }
</style>
