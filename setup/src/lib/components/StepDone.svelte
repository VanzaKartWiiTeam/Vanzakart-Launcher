<script lang="ts">
  /** Ultimo passo: cosa è stato fatto, e il pulsante per avviare. */
  import Icon from '$lib/components/Icon.svelte';
  import type { InstallReport } from '$setup/lib/api';
  import { formatBytes } from '$setup/lib/format';

  let { report, launchAfter = $bindable() }: { report: InstallReport; launchAfter: boolean } =
    $props();

  const shortcuts = $derived(
    report.artifacts.filter(
      (artifact) => artifact.kind !== 'record' && artifact.kind !== 'registry-key'
    )
  );
</script>

<div class="vk-view-enter view">
  <header>
    <p class="vk-eyebrow">Fatto</p>
    <h1 class="vk-title">VanzaKart Launcher {report.version} è installato</h1>
    <p class="vk-subtitle">
      Al primo avvio il launcher chiede dove sono Dolphin e la ROM, poi scarica la modpack.
    </p>
  </header>

  <section class="vk-card">
    <dl class="summary">
      <div>
        <dt>Cartella</dt>
        <dd class="vk-mono">{report.installDir}</dd>
      </div>
      <div>
        <dt>Spazio occupato</dt>
        <dd>{formatBytes(report.bytes)}</dd>
      </div>
      {#if report.uninstaller}
        <div>
          <dt>Disinstallatore</dt>
          <dd class="vk-mono">{report.uninstaller}</dd>
        </div>
      {/if}
      {#if report.backup}
        <div>
          <dt>Backup delle impostazioni</dt>
          <dd class="vk-mono">{report.backup}</dd>
        </div>
      {/if}
    </dl>

    {#if shortcuts.length > 0}
      <ul class="shortcuts">
        {#each shortcuts as artifact (artifact.path)}
          <li>
            <Icon name="check" size={12} />
            <span>{artifact.path}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <label class="launch">
    <input type="checkbox" bind:checked={launchAfter} />
    <span>Avvia VanzaKart Launcher alla chiusura</span>
  </label>
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    gap: var(--vk-gap-md);
  }

  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--vk-gap);
    margin: 0;
  }

  .summary div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  dt {
    font-size: var(--vk-fs-eyebrow);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--vk-text-secondary);
    font-weight: 700;
  }

  dd {
    margin: 0;
    font-size: var(--vk-fs-small);
    word-break: break-all;
    user-select: text;
  }

  .shortcuts {
    margin: var(--vk-gap-md) 0 0;
    padding: var(--vk-gap) 0 0;
    border-top: 1px solid var(--vk-stroke);
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .shortcuts li {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
    word-break: break-all;
  }

  .shortcuts :global(svg) {
    color: var(--vk-success);
    flex: none;
  }

  .launch {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--vk-fs-small);
    cursor: pointer;
  }

  input[type='checkbox'] {
    width: 16px;
    height: 16px;
    accent-color: var(--vk-cyan);
  }
</style>
