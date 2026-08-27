<script lang="ts">
  /**
   * Colonna dei passi, come le `StepCard` del setup legacy: il passo corrente
   * ha il bordo arcobaleno, quelli già fatti restano segnati con la spunta,
   * quelli futuri sono spenti.
   */
  import Icon from '$lib/components/Icon.svelte';

  let {
    steps,
    current
  }: { steps: { key: string; label: string; hint: string }[]; current: number } = $props();
</script>

<nav class="rail" aria-label="Passi dell'installazione">
  <ol>
    {#each steps as step, index (step.key)}
      <li
        class="step"
        class:step--active={index === current}
        class:step--done={index < current}
        aria-current={index === current ? 'step' : undefined}
      >
        <span class="marker">
          {#if index < current}
            <Icon name="check" size={13} />
          {:else}
            {index + 1}
          {/if}
        </span>
        <span class="text">
          <span class="label">{step.label}</span>
          <span class="hint">{step.hint}</span>
        </span>
      </li>
    {/each}
  </ol>
</nav>

<style>
  .rail {
    width: 268px;
    flex: none;
    padding: var(--vk-gap-md);
    background: var(--vk-sidebar-bg);
    overflow-y: auto;
  }

  ol {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .step {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-btn);
    background: var(--vk-panel-soft);
    transition:
      border-color var(--vk-dur) var(--vk-ease),
      background var(--vk-dur) var(--vk-ease);
  }

  /*
   * Il colore sta sul bordo, non sotto il testo (§D-046): un riempimento
   * arcobaleno renderebbe illeggibile l'etichetta bianca.
   */
  .step--active {
    background: var(--vk-active-surface);
    border-color: transparent;
    background-image:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)), var(--vk-rainbow);
    background-origin: border-box;
    background-clip: padding-box, border-box;
    box-shadow: var(--vk-glow-cyan);
  }

  .step--done {
    border-color: rgb(77 255 176 / 0.35);
  }

  .marker {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    flex: none;
    border-radius: var(--vk-radius-pill);
    background: var(--vk-panel);
    border: 1px solid var(--vk-stroke);
    font-size: var(--vk-fs-micro);
    font-weight: 800;
  }

  .step--done .marker {
    color: var(--vk-success);
    border-color: rgb(77 255 176 / 0.5);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .label {
    font-size: var(--vk-fs-small);
    font-weight: 700;
  }

  .hint {
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  @media (max-width: 900px) {
    .rail {
      width: 200px;
    }

    .hint {
      display: none;
    }
  }
</style>
