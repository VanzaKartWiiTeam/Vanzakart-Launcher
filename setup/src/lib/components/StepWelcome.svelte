<script lang="ts">
  /** Primo passo: chi sei, cosa sto per installare, cosa c'è già. */
  import Icon from '$lib/components/Icon.svelte';
  import type { Bootstrap } from '$setup/lib/api';
  import { formatBytes, formatDate } from '$setup/lib/format';

  let {
    boot,
    busy,
    onRetry,
    onOpenDownloadPage
  }: {
    boot: Bootstrap;
    busy: boolean;
    onRetry: () => void;
    onOpenDownloadPage: () => void;
  } = $props();
</script>

<div class="vk-view-enter view">
  <header>
    <p class="vk-eyebrow">Installazione guidata</p>
    <h1 class="vk-title">VanzaKart Launcher</h1>
    <p class="vk-subtitle">
      Scarica e installa il launcher della modpack su {boot.platform}. Da lì si installano la
      modpack, il music pack e gli addon, e si avvia il gioco.
    </p>
  </header>

  {#if boot.release}
    <section class="vk-card vk-rainbow-top">
      <div class="release">
        <div>
          <p class="vk-eyebrow">Versione da installare</p>
          <p class="version">{boot.release.version}</p>
          {#if boot.release.pubDate}
            <p class="vk-faint">Pubblicata il {formatDate(boot.release.pubDate)}</p>
          {/if}
        </div>
        <dl class="facts">
          <div>
            <dt>Pacchetto</dt>
            <dd>{boot.release.packageKey}</dd>
          </div>
          <div>
            <dt>Dimensione</dt>
            <dd>
              {boot.release.sizeBytes > 0 ? formatBytes(boot.release.sizeBytes) : 'da leggere'}
            </dd>
          </div>
          <div>
            <dt>Verifica</dt>
            <dd class={boot.release.verifiable ? 'ok' : 'warn'}>
              {boot.release.verifiable ? 'impronta SHA-256' : 'non dichiarata'}
            </dd>
          </div>
        </dl>
      </div>

      {#if boot.release.notes}
        <p class="notes">{boot.release.notes}</p>
      {/if}
    </section>
  {:else}
    <section class="vk-error">
      <p class="strong">Non riesco a leggere l'elenco dei pacchetti dal server.</p>
      <p class="reason">{boot.releaseError ?? 'Causa sconosciuta.'}</p>
      <div class="vk-row actions">
        <button class="vk-btn" onclick={onRetry} disabled={busy}>
          <Icon name="refresh" size={14} />
          Riprova
        </button>
        <button class="vk-btn" onclick={onOpenDownloadPage}>
          <Icon name="external" size={14} />
          Apri la pagina dei download
        </button>
      </div>
    </section>
  {/if}

  {#if boot.existing}
    <section class="vk-card existing">
      <div class="vk-row">
        <Icon name="package" size={18} />
        <p class="strong">Installazione già presente</p>
      </div>
      <p class="vk-muted">
        {#if boot.existing.version}
          Versione {boot.existing.version}
        {:else}
          Versione sconosciuta
        {/if}
        · {formatBytes(boot.existing.bytes)}
      </p>
      <p class="vk-mono path">{boot.existing.installDir}</p>
      <p class="vk-faint">
        {#if boot.existing.managed}
          Verrà aggiornata sul posto. Al passo successivo puoi scegliere una reinstallazione pulita.
        {:else}
          Non è stata installata da questa procedura: si può comunque aggiornare o sostituire.
        {/if}
      </p>
    </section>
  {/if}

  {#if boot.legacyInstallDir}
    <section class="vk-card legacy">
      <div class="vk-row">
        <Icon name="package" size={16} />
        <p class="strong">C'è anche il launcher precedente</p>
      </div>
      <p class="vk-mono path">{boot.legacyInstallDir}</p>
      <p class="vk-faint">
        Resta dov'è: il launcher nuovo si installa in una cartella sua e al primo avvio importa le
        impostazioni di quello vecchio, senza toccarne i file. Quando non ti serve più,
        disinstallalo con il suo disinstallatore.
      </p>
    </section>
  {/if}

  <p class="vk-faint footnote">
    L'installazione non richiede privilegi di amministratore e non tocca i dati di gioco già
    presenti.
  </p>
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    gap: var(--vk-gap-md);
  }

  .release {
    display: flex;
    flex-wrap: wrap;
    gap: var(--vk-gap-lg);
    align-items: flex-start;
    justify-content: space-between;
  }

  .version {
    font-size: var(--vk-fs-section);
    font-weight: 900;
    margin: 4px 0 2px;
  }

  .facts {
    display: flex;
    gap: var(--vk-gap-lg);
    margin: 0;
  }

  .facts div {
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
    font-weight: 700;
  }

  dd.ok {
    color: var(--vk-success);
  }

  dd.warn {
    color: var(--vk-warning);
  }

  .notes {
    margin: var(--vk-gap-md) 0 0;
    padding-top: var(--vk-gap);
    border-top: 1px solid var(--vk-stroke);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-small);
    white-space: pre-wrap;
  }

  .existing,
  .legacy {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .strong {
    margin: 0;
    font-weight: 800;
  }

  .path {
    margin: 0;
    color: var(--vk-text-secondary);
    word-break: break-all;
    user-select: text;
  }

  .reason {
    margin: 6px 0 0;
    font-size: var(--vk-fs-small);
    color: var(--vk-text-secondary);
    user-select: text;
  }

  .actions {
    margin-top: var(--vk-gap);
  }

  .footnote {
    margin: 0;
    font-size: var(--vk-fs-micro);
  }
</style>
