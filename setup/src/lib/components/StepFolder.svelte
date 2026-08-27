<script lang="ts">
  /** Secondo passo: dove installare, cosa creare, cosa salvare prima. */
  import Icon from '$lib/components/Icon.svelte';
  import type { Bootstrap, InstallOptionsInput } from '$setup/lib/api';

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
    <p class="vk-eyebrow">Passo 2</p>
    <h1 class="vk-title">Cartella e scorciatoie</h1>
  </header>

  <section class="vk-card">
    <label class="field" for="install-dir">
      <span class="vk-eyebrow">Cartella d'installazione</span>
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
          Sfoglia
        </button>
      </div>
    </label>

    {#if boot.suggestedInstallDirs.length > 1}
      <div class="suggestions">
        <span class="vk-faint">Proposte:</span>
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
        <legend class="vk-eyebrow">C'è già un'installazione qui</legend>
        <label class="choice">
          <input type="radio" bind:group={options.mode} value="update" />
          <span>
            <strong>Aggiorna</strong>
            <span class="vk-faint">Sostituisce i file del programma e lascia il resto dov'è.</span>
          </span>
        </label>
        <label class="choice">
          <input type="radio" bind:group={options.mode} value="clean-reinstall" />
          <span>
            <strong>Reinstallazione pulita</strong>
            <span class="vk-faint">
              Svuota la cartella prima di installare. Impostazioni, modpack e salvataggi stanno
              altrove e non vengono toccati.
            </span>
          </span>
        </label>
      </fieldset>
    {/if}
  </section>

  <div class="vk-grid-2">
    <section class="vk-card">
      <p class="vk-eyebrow">Scorciatoie</p>
      <label class="check">
        <input type="checkbox" bind:checked={options.desktopShortcut} />
        <span>Sul desktop</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={options.startMenuShortcut} />
        <span>Nel menu applicazioni</span>
      </label>
      <label class="check">
        <input
          type="checkbox"
          bind:checked={options.uninstallEntry}
          disabled={!options.startMenuShortcut}
        />
        <span>Voce per disinstallare</span>
      </label>
      {#if boot.supportsQuickLaunch}
        <label class="check">
          <input type="checkbox" bind:checked={options.quickLaunchShortcut} />
          <span>Nella barra di avvio veloce</span>
        </label>
      {/if}
      {#if boot.supportsPathSymlink}
        <label class="check">
          <input type="checkbox" bind:checked={options.pathSymlink} />
          <span>Comando <code>vanzakart-launcher</code> in <code>~/.local/bin</code></span>
        </label>
      {/if}
    </section>

    <section class="vk-card">
      <p class="vk-eyebrow">Prima di procedere</p>
      <label class="check">
        <input type="checkbox" bind:checked={options.backupData} />
        <span>Copia le impostazioni del launcher</span>
      </label>
      <label class="field" for="backup-dir">
        <span class="vk-faint">Cartella del backup</span>
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
            Sfoglia
          </button>
        </div>
      </label>
      <p class="vk-faint note">
        Vengono copiate le impostazioni e i percorsi configurati. Il token della beta non viene
        copiato.
      </p>
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
