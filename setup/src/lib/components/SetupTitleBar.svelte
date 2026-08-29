<script lang="ts">
  /**
   * Barra del titolo dell'installer.
   *
   * La finestra è senza decorazioni come quella del launcher, ma qui i
   * pulsanti restano visibili anche su Linux: l'installer si apre una volta
   * sola, e chi lo usa non ha ancora imparato le scorciatoie del programma.
   * Manca "ingrandisci": una procedura guidata a tutto schermo non serve a
   * nessuno.
   */
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import Icon from '$lib/components/Icon.svelte';
  import logo from '$lib/assets/logo.png';
  import { i18n, t, LOCALES, LOCALE_LABELS } from '$setup/lib/i18n/store.svelte';

  let { subtitle = '', busy = false }: { subtitle?: string; busy?: boolean } = $props();

  const appWindow = getCurrentWindow();

  async function minimize() {
    await appWindow.minimize();
  }

  async function close() {
    await appWindow.close();
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img src={logo} alt="" width="26" height="26" />
    <span class="wordmark">VANZAKART</span>
    {#if subtitle}
      <span class="vk-badge chip">{subtitle}</span>
    {/if}
  </div>

  <div class="controls">
    <!--
      La lingua si sceglie qui: l'installer non ha una pagina impostazioni, e
      la prima schermata deve poter cambiare lingua senza cercarla (§D-081).
    -->
    <div class="languages" role="group" aria-label={t('titlebar.language')}>
      {#each LOCALES as code (code)}
        <button
          class="lang"
          class:lang--active={i18n.locale === code}
          onclick={() => i18n.set(code)}
          lang={code}
          title={LOCALE_LABELS[code]}
        >
          {code.toUpperCase()}
        </button>
      {/each}
    </div>

    <button class="chrome" onclick={minimize} aria-label={t('titlebar.minimize')}>
      <Icon name="minimize" size={14} />
    </button>
    <button
      class="chrome chrome--close"
      onclick={close}
      aria-label={t('titlebar.close')}
      disabled={busy}
      title={busy ? t('titlebar.busy') : t('titlebar.close')}
    >
      <Icon name="close" size={13} />
    </button>
  </div>
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
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .languages {
    display: flex;
    gap: 2px;
    margin-right: 8px;
  }

  .lang {
    padding: 4px 8px;
    border: 1px solid transparent;
    border-radius: var(--vk-radius-badge);
    background: transparent;
    color: var(--vk-text-faint);
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    letter-spacing: 0.04em;
    transition:
      color var(--vk-dur-fast) var(--vk-ease),
      border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .lang:hover {
    color: var(--vk-text);
  }

  .lang--active {
    color: var(--vk-text);
    border-color: var(--vk-cyan);
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

  .chrome:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.08);
    color: var(--vk-text);
  }

  .chrome:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .chrome--close:hover:not(:disabled) {
    background: rgb(255 107 130 / 0.22);
    color: #fff;
  }
</style>
