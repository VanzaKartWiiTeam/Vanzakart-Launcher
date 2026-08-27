<script lang="ts">
  /**
   * Shell dell'applicazione: title bar, sidebar, header di pagina e area di
   * contenuto scrollabile. Stessa struttura del `Grid` radice di
   * MainWindow.xaml.
   */
  import { onMount } from 'svelte';

  import * as api from '$lib/api';
  import AmbientBackdrop from '$lib/components/AmbientBackdrop.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import Toasts from '$lib/components/Toasts.svelte';
  import { app, PAGE_META } from '$lib/stores/app.svelte';

  import Debug from './Debug.svelte';
  import Friends from './Friends.svelte';
  import Home from './Home.svelte';
  import Leaderboard from './Leaderboard.svelte';
  import Licenses from './Licenses.svelte';
  import Mods from './Mods.svelte';
  import News from './News.svelte';
  import Rooms from './Rooms.svelte';
  import Settings from './Settings.svelte';

  const meta = $derived(PAGE_META[app.route]);

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      unlisten = await api.onProgress((event) => {
        if (disposed) return;
        app.progress = event;
        if (event.detail)
          app.setStatusLine(event.detail, event.phase === 'Error' ? 'danger' : 'info');
      });

      try {
        await app.refresh();
        await api.bootstrap();
        await app.refresh();
      } catch (error) {
        app.toast('Avvio incompleto', api.errorMessage(error), 'warning');
      }
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  /** Ctrl+Shift+D sblocca la pagina Debug, nascosta come nel legacy. */
  function onKeydown(event: KeyboardEvent) {
    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'd') {
      app.debugUnlocked = !app.debugUnlocked;
      app.toast(
        'Debug',
        app.debugUnlocked ? 'Pagina di diagnostica visibile.' : 'Pagina di diagnostica nascosta.',
        'info'
      );
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="shell">
  <AmbientBackdrop />
  <TitleBar />

  <div class="body">
    <Sidebar />

    <main class="main">
      <header class="page-header">
        <h1 class="page-title">{meta.title}</h1>
        <p class="page-subtitle">{meta.subtitle}</p>
      </header>

      <div class="content" id="vk-content">
        {#key app.route}
          <div class="vk-view-enter">
            {#if app.route === 'home'}
              <Home />
            {:else if app.route === 'news'}
              <News />
            {:else if app.route === 'rooms'}
              <Rooms />
            {:else if app.route === 'leaderboard'}
              <Leaderboard />
            {:else if app.route === 'friends'}
              <Friends />
            {:else if app.route === 'mods'}
              <Mods />
            {:else if app.route === 'licenses'}
              <Licenses />
            {:else if app.route === 'settings'}
              <Settings />
            {:else if app.route === 'debug'}
              <Debug />
            {/if}
          </div>
        {/key}
      </div>
    </main>
  </div>

  <Toasts />
</div>

<style>
  /*
   * Rettangolo pieno, senza raggio e senza bordo: gli angoli li arrotonda il
   * gestore finestre. Arrotondarli anche qui creava due curve diverse — la
   * nostra e quella di Windows — con lo spigolo che sbucava fra le due
   * (§U-03).
   */
  .shell {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--vk-window-gradient);
    overflow: hidden;
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
    position: relative;
    z-index: 1;
  }

  .main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    padding: var(--vk-content-pad-top) var(--vk-content-pad-x) 24px;
  }

  .page-header {
    height: var(--vk-header-h);
    display: flex;
    flex-direction: column;
    justify-content: center;
    flex: none;
  }

  .page-title {
    margin: 0;
    font-size: var(--vk-fs-page-title);
    font-weight: 900;
    letter-spacing: -0.01em;
  }

  .page-subtitle {
    margin: 4px 0 0;
    font-size: var(--vk-fs-small);
    color: var(--vk-text-secondary);
  }

  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding-right: 6px;
  }

  /* Sotto i 1320 px il layout si stringe invece di tagliare (ui-parity U-07). */
  @media (max-width: 1180px) {
    .main {
      padding-inline: 16px;
    }
  }
</style>
