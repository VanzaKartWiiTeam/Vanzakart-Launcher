<script lang="ts">
  /**
   * Passo in corso: barra, velocità, tempo rimanente e registro.
   *
   * Un'operazione che non si vede non è partita (§D-041): finché il download
   * non dichiara una percentuale la barra resta indeterminata invece di
   * fingere lo zero.
   */
  import type { ProgressEvent } from '$setup/lib/api';

  let { title, progress, log }: { title: string; progress: ProgressEvent | null; log: string[] } =
    $props();

  const percent = $derived(progress?.percent ?? null);
  const indeterminate = $derived(percent === null);
</script>

<div class="vk-view-enter view">
  <header>
    <p class="vk-eyebrow">{progress?.phase ?? 'Avvio'}</p>
    <h1 class="vk-title">{title}</h1>
    <p class="vk-subtitle">{progress?.detail ?? 'Preparazione…'}</p>
  </header>

  <section class="vk-card">
    <div class="vk-progress" class:vk-progress--indeterminate={indeterminate}>
      <div class="vk-progress__fill" style={indeterminate ? '' : `width: ${percent}%`}></div>
    </div>

    <dl class="stats">
      <div>
        <dt>Avanzamento</dt>
        <dd>{percent === null ? '—' : `${Math.round(percent)}%`}</dd>
      </div>
      <div>
        <dt>Scaricato</dt>
        <dd>{progress?.bytesLabel || '—'}</dd>
      </div>
      <div>
        <dt>Velocità</dt>
        <dd>{progress?.speedLabel || '—'}</dd>
      </div>
      <div>
        <dt>Tempo rimanente</dt>
        <dd>{progress?.etaLabel || '—'}</dd>
      </div>
    </dl>
  </section>

  <section class="vk-card vk-card--flush log-card">
    <p class="vk-eyebrow log-title">Registro</p>
    <ol class="log">
      {#each log as line, index (index)}
        <li>{line}</li>
      {/each}
    </ol>
  </section>
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    gap: var(--vk-gap-md);
    min-height: 0;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: var(--vk-gap);
    margin: var(--vk-gap-md) 0 0;
  }

  .stats div {
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
    font-size: var(--vk-fs-card-title);
    font-weight: 800;
  }

  .log-card {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }

  .log-title {
    padding: 14px var(--vk-gap-md) 0;
  }

  .log {
    flex: 1;
    min-height: 120px;
    max-height: 220px;
    overflow-y: auto;
    margin: 8px 0 0;
    padding: 0 var(--vk-gap-md) 14px;
    list-style: none;
    font-family: var(--vk-font-mono);
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
    user-select: text;
  }

  .log li {
    padding: 2px 0;
  }
</style>
