<script lang="ts">
  /**
   * Terzo passo: le verifiche. Sono le stesse del setup legacy — spazio,
   * dimensione del download, permessi — più quella che mancava: il launcher
   * aperto, che su Windows bloccava l'estrazione a metà.
   */
  import Icon from '$lib/components/Icon.svelte';
  import type { Preflight } from '$setup/lib/api';
  import { formatBytes } from '$setup/lib/format';

  let {
    preflight,
    checking,
    error,
    onRecheck
  }: {
    preflight: Preflight | null;
    checking: boolean;
    error: string;
    onRecheck: () => void;
  } = $props();

  type Tone = 'ok' | 'warn' | 'bad';

  const checks = $derived.by((): { label: string; value: string; tone: Tone }[] => {
    if (!preflight) return [];
    return [
      {
        label: 'Spazio necessario',
        value: formatBytes(preflight.requiredBytes),
        tone: 'ok'
      },
      {
        label: 'Spazio disponibile',
        value:
          preflight.availableBytes > 0 ? formatBytes(preflight.availableBytes) : 'non rilevabile',
        tone: preflight.enoughSpace ? 'ok' : 'bad'
      },
      {
        label: 'Da scaricare',
        value:
          preflight.downloadBytes > 0 ? formatBytes(preflight.downloadBytes) : 'non dichiarato',
        tone: preflight.downloadBytes > 0 ? 'ok' : 'warn'
      },
      {
        label: 'Cartella scrivibile',
        value: preflight.writable ? 'sì' : 'no, servono altri permessi',
        tone: preflight.writable ? 'ok' : 'bad'
      },
      {
        label: 'Launcher in esecuzione',
        value: preflight.launcherRunning ? 'sì, va chiuso' : 'no',
        tone: preflight.launcherRunning ? 'bad' : 'ok'
      },
      {
        label: 'Verifica del pacchetto',
        value: preflight.verifiable ? 'impronta SHA-256' : 'non dichiarata dal server',
        tone: preflight.verifiable ? 'ok' : 'warn'
      }
    ];
  });
</script>

<div class="vk-view-enter view">
  <header>
    <p class="vk-eyebrow">Passo 3</p>
    <h1 class="vk-title">Verifiche</h1>
    {#if preflight}
      <p class="vk-subtitle">
        VanzaKart Launcher {preflight.version} in
        <span class="vk-mono">{preflight.installDir}</span>
      </p>
    {/if}
  </header>

  {#if checking}
    <div class="vk-card">
      <div class="vk-progress vk-progress--indeterminate">
        <div class="vk-progress__fill"></div>
      </div>
      <p class="vk-muted checking">Controllo in corso…</p>
    </div>
  {:else if error}
    <div class="vk-error">
      <p class="strong">Le verifiche non sono riuscite.</p>
      <p class="reason">{error}</p>
      <button class="vk-btn" onclick={onRecheck}>
        <Icon name="refresh" size={14} />
        Riprova
      </button>
    </div>
  {:else if preflight}
    <section class="vk-card vk-card--flush">
      <ul class="checks">
        {#each checks as check (check.label)}
          <li>
            <span class="marker marker--{check.tone}">
              <Icon name={check.tone === 'ok' ? 'check' : 'warning'} size={12} />
            </span>
            <span class="label">{check.label}</span>
            <span class="value value--{check.tone}">{check.value}</span>
          </li>
        {/each}
      </ul>
    </section>

    {#if !preflight.enoughSpace}
      <p class="vk-error">
        Sul disco scelto non c'è abbastanza spazio. Libera spazio o scegli un'altra cartella.
      </p>
    {:else if preflight.launcherRunning}
      <p class="vk-error">
        VanzaKart Launcher è aperto. Chiudilo e ripeti le verifiche: i suoi file non si possono
        sostituire mentre è in esecuzione.
      </p>
    {:else if !preflight.writable}
      <p class="vk-error">
        Nella cartella scelta non si può scrivere. Scegline una dentro la tua cartella utente.
      </p>
    {:else}
      <p class="ready">
        <Icon name="check" size={14} />
        Tutto pronto: premi <strong>Installa</strong> per procedere.
      </p>
    {/if}
  {/if}
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    gap: var(--vk-gap-md);
  }

  .checks {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .checks li {
    display: grid;
    grid-template-columns: 24px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 13px var(--vk-gap-md);
    border-bottom: 1px solid var(--vk-stroke);
    font-size: var(--vk-fs-small);
  }

  .checks li:last-child {
    border-bottom: none;
  }

  .marker {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: var(--vk-radius-pill);
  }

  .marker--ok {
    color: var(--vk-success);
    background: rgb(77 255 176 / 0.12);
  }

  .marker--warn {
    color: var(--vk-warning);
    background: rgb(255 209 102 / 0.12);
  }

  .marker--bad {
    color: var(--vk-danger);
    background: rgb(255 107 130 / 0.14);
  }

  .value {
    font-weight: 700;
  }

  .value--warn {
    color: var(--vk-warning);
  }

  .value--bad {
    color: var(--vk-danger);
  }

  .ready {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    color: var(--vk-success);
    font-size: var(--vk-fs-small);
  }

  .checking {
    margin: var(--vk-gap) 0 0;
    font-size: var(--vk-fs-small);
  }

  .strong {
    margin: 0;
    font-weight: 800;
  }

  .reason {
    margin: 6px 0 var(--vk-gap);
    font-size: var(--vk-fs-small);
    color: var(--vk-text-secondary);
    user-select: text;
  }
</style>
