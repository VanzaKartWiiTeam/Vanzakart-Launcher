<script lang="ts">
  /** Secondo passo: dove installare, cosa creare, cosa salvare prima. */
  import Icon from '$lib/components/Icon.svelte';
  import type { Bootstrap, InstallOptionsInput } from '$setup/lib/api';
  import { t } from '$setup/lib/i18n/store.svelte';

  let {
    boot,
    options,
    onBrowseInstall,
    onBrowseBackup
  }: {
    boot: Bootstrap;
    /** Oggetto reattivo del genitore: i campi si modificano qui dentro. */
    options: InstallOptionsInput;
    onBrowseInstall: () => void;
    onBrowseBackup: () => void;
  } = $props();
</script>

<div class="vk-view-enter view">
  <header>
    <p class="vk-eyebrow">{t('folder.eyebrow')}</p>
    <h1 class="vk-title">{t('folder.title')}</h1>
  </header>

  <section class="vk-card">
    <label class="field" for="install-dir">
      <span class="vk-eyebrow">{t('folder.installDir')}</span>
      <div class="vk-row">
        <input
          id="install-dir"
          class="vk-input"
          bind:value={options.installDir}
          spellcheck="false"
          autocomplete="off"
        />
        <button class="vk-btn" onclick={onBrowseInstall} type="button">
          <Icon name="folder" size={14} />
          {t('common.browse')}
        </button>
      </div>
    </label>

    {#if boot.suggestedInstallDirs.length > 1}
      <div class="suggestions">
        <span class="vk-faint">{t('folder.suggestions')}</span>
        {#each boot.suggestedInstallDirs as suggestion (suggestion)}
          <button
            class="chip"
            type="button"
            class:chip--active={options.installDir === suggestion}
            onclick={() => (options.installDir = suggestion)}
          >
            {suggestion}
          </button>
        {/each}
      </div>
    {/if}

    {#if boot.existing}
      <fieldset class="modes">
        <legend class="vk-eyebrow">{t('folder.modes')}</legend>
        <label class="choice">
          <input type="radio" bind:group={options.mode} value="update" />
          <span>
            <strong>{t('folder.mode.update')}</strong>
            <span class="vk-faint">{t('folder.mode.update.note')}</span>
          </span>
        </label>
        <label class="choice">
          <input type="radio" bind:group={options.mode} value="clean-reinstall" />
          <span>
            <strong>{t('folder.mode.clean')}</strong>
            <span class="vk-faint">{t('folder.mode.clean.note')}</span>
          </span>
        </label>
      </fieldset>
    {/if}
  </section>

  <div class="vk-grid-2">
    <section class="vk-card">
      <p class="vk-eyebrow">{t('folder.shortcuts')}</p>
      <label class="check">
        <input type="checkbox" bind:checked={options.desktopShortcut} />
        <span>{t('folder.shortcut.desktop')}</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={options.startMenuShortcut} />
        <span>{t('folder.shortcut.startMenu')}</span>
      </label>
      <label class="check">
        <input
          type="checkbox"
          bind:checked={options.uninstallEntry}
          disabled={!options.startMenuShortcut}
        />
        <span>{t('folder.shortcut.uninstallEntry')}</span>
      </label>
      {#if boot.supportsQuickLaunch}
        <label class="check">
          <input type="checkbox" bind:checked={options.quickLaunchShortcut} />
          <span>{t('folder.shortcut.quickLaunch')}</span>
        </label>
      {/if}
      {#if boot.supportsPathSymlink}
        <label class="check">
          <input type="checkbox" bind:checked={options.pathSymlink} />
          <span>
            {t('folder.shortcut.symlinkBefore')}
            <code>vanzakart-launcher</code>
            {t('folder.shortcut.symlinkMiddle')}
            <code>~/.local/bin</code>
          </span>
        </label>
      {/if}
    </section>

    <section class="vk-card">
      <p class="vk-eyebrow">{t('folder.before')}</p>
      <label class="check">
        <input type="checkbox" bind:checked={options.backupData} />
        <span>{t('folder.backupData')}</span>
      </label>
      <label class="field" for="backup-dir">
        <span class="vk-faint">{t('folder.backupDir')}</span>
        <div class="vk-row">
          <input
            id="backup-dir"
            class="vk-input"
            bind:value={options.backupDir}
            disabled={!options.backupData}
            spellcheck="false"
            autocomplete="off"
          />
          <button
            class="vk-btn"
            type="button"
            onclick={onBrowseBackup}
            disabled={!options.backupData}
          >
            <Icon name="folder" size={14} />
            {t('common.browse')}
          </button>
        </div>
      </label>
      <p class="vk-faint note">{t('folder.backupNote')}</p>
    </section>
  </div>
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    gap: var(--vk-gap-md);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .suggestions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: var(--vk-gap);
  }

  .chip {
    padding: 5px 10px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-pill);
    background: var(--vk-panel-soft);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-micro);
    font-family: var(--vk-font-mono);
  }

  .chip:hover {
    color: var(--vk-text);
    border-color: var(--vk-cyan);
  }

  .chip--active {
    color: var(--vk-text);
    border-color: var(--vk-cyan);
    box-shadow: var(--vk-glow-cyan);
  }

  .modes {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: var(--vk-gap-md) 0 0;
    padding: 0;
    border: none;
  }

  legend {
    padding: 0;
    margin-bottom: 8px;
  }

  .choice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-btn);
    background: var(--vk-panel-soft);
    cursor: pointer;
  }

  .choice span {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--vk-fs-small);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 0;
    font-size: var(--vk-fs-small);
    cursor: pointer;
  }

  .check:has(input:disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }

  input[type='checkbox'],
  input[type='radio'] {
    width: 16px;
    height: 16px;
    accent-color: var(--vk-cyan);
    flex: none;
  }

  code {
    font-family: var(--vk-font-mono);
    font-size: var(--vk-fs-micro);
    color: var(--vk-cyan-soft);
  }

  .note {
    margin: var(--vk-gap-sm) 0 0;
    font-size: var(--vk-fs-micro);
  }

  section.vk-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
</style>
