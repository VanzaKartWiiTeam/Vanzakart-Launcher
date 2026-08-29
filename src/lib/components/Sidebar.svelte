<script lang="ts">
  /**
   * Sidebar da 252 px con i quattro gruppi del launcher WPF —
   * General, Online, Customize, Other — nello stesso ordine, più la card
   * COMMUNITY in fondo.
   */
  import * as api from '$lib/api';
  import Icon from './Icon.svelte';
  import type { IconName } from './Icon.svelte';
  import { app, PAGE_META, TEAM_LINKS, type Route } from '$lib/stores/app.svelte';
  import { t, type TranslationKey } from '$lib/stores/i18n.svelte';

  interface Group {
    label: TranslationKey;
    routes: Route[];
  }

  const GROUPS: Group[] = [
    { label: 'sidebar.group.general', routes: ['home', 'news'] },
    { label: 'sidebar.group.online', routes: ['rooms', 'leaderboard', 'friends'] },
    { label: 'sidebar.group.customize', routes: ['mods', 'licenses'] },
    { label: 'sidebar.group.other', routes: ['settings', 'debug'] }
  ];

  const LABELS: Record<Route, string> = {
    home: 'HOME / PLAY',
    news: 'NEWS',
    rooms: 'ROOMS',
    leaderboard: 'LEADERBOARD',
    friends: 'FRIENDS',
    mods: 'MODS',
    licenses: 'MII & LICENSES',
    settings: 'SETTINGS',
    debug: 'DEBUG'
  };

  const visible = $derived(new Set(app.visibleRoutes));

  async function open(url: string) {
    try {
      await api.openExternal(url);
    } catch (error) {
      app.toast(t('sidebar.openFailed'), api.errorMessage(error), 'warning');
    }
  }
</script>

<nav class="sidebar" aria-label={t('sidebar.nav')}>
  <div class="groups">
    {#each GROUPS as group (group.label)}
      {@const routes = group.routes.filter((route) => visible.has(route))}
      {#if routes.length > 0}
        <p class="group-label">{t(group.label)}</p>
        {#each routes as route (route)}
          <button
            class="nav"
            class:active={app.route === route}
            onclick={() => app.navigate(route)}
            aria-current={app.route === route ? 'page' : undefined}
          >
            <Icon name={PAGE_META[route].icon as IconName} size={17} />
            <span>{LABELS[route]}</span>
          </button>
        {/each}
      {/if}
    {/each}
  </div>

  <div class="vk-card vk-card--mini community">
    <p class="vk-eyebrow">{t('sidebar.community')}</p>
    <p class="vk-faint hint">{t('sidebar.communityHint')}</p>
    <div class="links">
      <button class="vk-btn" onclick={() => open(TEAM_LINKS.website)}>{t('sidebar.website')}</button
      >
      <button class="vk-btn" onclick={() => open(TEAM_LINKS.discord)}>{t('sidebar.server')}</button>
    </div>
  </div>
</nav>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: var(--vk-gap-md);
    width: var(--vk-sidebar-w);
    flex: none;
    padding: 16px;
    background: var(--vk-sidebar-bg);
    border-right: 1px solid #202a42;
    overflow-y: auto;
  }

  .groups {
    display: flex;
    flex-direction: column;
  }

  .group-label {
    margin: 14px 0 8px 12px;
    font-size: var(--vk-fs-eyebrow);
    font-weight: 700;
    color: var(--vk-text-faint);
  }

  .group-label:first-child {
    margin-top: 0;
  }

  .nav {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    margin-bottom: 4px;
    padding: 10px 12px;
    border: 1px solid transparent;
    border-radius: var(--vk-radius-input);
    background: transparent;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-micro);
    font-weight: 700;
    letter-spacing: 0.03em;
    text-align: left;
    transition:
      background var(--vk-dur-fast) var(--vk-ease),
      color var(--vk-dur-fast) var(--vk-ease);
  }

  .nav:hover {
    background: rgb(255 255 255 / 0.05);
    color: var(--vk-text);
  }

  .nav.active {
    background: var(--vk-tab-active);
    border-color: #33456b;
    color: var(--vk-text);
  }

  /* Indicatore arcobaleno della voce attiva. */
  .nav.active::before {
    content: '';
    position: absolute;
    left: -1px;
    top: 8px;
    bottom: 8px;
    width: 3px;
    border-radius: var(--vk-radius-pill);
    background: var(--vk-rainbow);
    background-size: 300% 100%;
  }

  .community {
    flex: none;
  }

  .hint {
    margin: 4px 0 12px;
    font-size: var(--vk-fs-micro);
  }

  .links {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .links :global(.vk-btn) {
    padding: 8px;
    font-size: var(--vk-fs-micro);
  }
</style>
