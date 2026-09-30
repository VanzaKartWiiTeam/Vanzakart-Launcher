<script lang="ts">
  /**
   * Cursore del design system.
   *
   * Sotto c'è un `<input type="range">` vero, ridisegnato da capo: la
   * tastiera (frecce, Pag su/giù, Home, Fine), il tocco e i lettori di
   * schermo li porta il browser, e qui non vanno riscritti. Tutti i WebView di
   * Tauri — WebView2, WKWebView, WebKitGTK — leggono i selettori
   * `::-webkit-slider-*`.
   *
   * - il riempimento è l'arcobaleno intero, come le barre di avanzamento;
   * - il valore si legge sempre, a destra dell'etichetta;
   * - − e + ai lati: un clic è un passo, tenerli premuti ripete;
   * - `oninput` arriva a ogni movimento, `onchange` solo a modifica finita
   *   (rilascio, tasto, pulsante): chi tiene una cronologia registra un passo
   *   per modifica, non uno per pixel;
   * - doppio clic: `onreset`, per tornare al valore di partenza.
   */
  import Icon from '$lib/components/Icon.svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import { t } from '$lib/stores/i18n.svelte';

  interface Props {
    value: number;
    min: number;
    max: number;
    step?: number;
    /** Nome del cursore: scritto sopra e letto dai lettori di schermo. */
    label: string;
    /** `false` quando il nome sta già altrove, accanto al cursore. */
    showLabel?: boolean;
    /** Come scrivere il valore; di default il numero. */
    format?: (value: number) => string;
    /** Spiegazione, nel suggerimento. */
    hint?: string | undefined;
    disabled?: boolean;
    /** I pulsanti − e +. */
    steppers?: boolean;
    oninput?: (value: number) => void;
    onchange?: (value: number) => void;
    onreset?: (() => void) | undefined;
  }

  const {
    value,
    min,
    max,
    step = 1,
    label,
    showLabel = true,
    format = String,
    hint,
    disabled = false,
    steppers = true,
    oninput,
    onchange,
    onreset
  }: Props = $props();

  const fill = $derived(max > min ? ((value - min) / (max - min)) * 100 : 0);
  const text = $derived(format(value));

  /** Il valore dopo un passo, dentro i limiti e sulla griglia dei passi. */
  function stepped(direction: 1 | -1): number {
    const next = Math.round((value + direction * step) / step) * step;
    return Math.min(max, Math.max(min, next));
  }

  // --- − e + tenuti premuti -------------------------------------------------

  const REPEAT_AFTER = 380;
  const REPEAT_EVERY = 60;
  let delay: ReturnType<typeof setTimeout> | undefined;
  let repeat: ReturnType<typeof setInterval> | undefined;
  let holding = false;

  function nudge(direction: 1 | -1): boolean {
    const next = stepped(direction);
    if (next === value) return false;
    oninput?.(next);
    return true;
  }

  function press(direction: 1 | -1, event: PointerEvent) {
    if (event.button !== 0 || disabled) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    holding = nudge(direction);
    delay = setTimeout(() => {
      repeat = setInterval(() => {
        if (nudge(direction)) holding = true;
        else release();
      }, REPEAT_EVERY);
    }, REPEAT_AFTER);
  }

  function release() {
    clearTimeout(delay);
    clearInterval(repeat);
    if (holding) {
      holding = false;
      onchange?.(value);
    }
  }

  /** Da tastiera il clic arriva senza pressione del puntatore (`detail` 0). */
  function keyStep(direction: 1 | -1, event: MouseEvent) {
    if (event.detail !== 0) return;
    const next = stepped(direction);
    if (next === value) return;
    oninput?.(next);
    onchange?.(next);
  }

  $effect(() => release);
</script>

<div class="slider" class:disabled>
  {#if showLabel}
    <div class="head">
      <span class="label" {@attach tooltip(hint)}>{label}</span>
      <output class="value">{text}</output>
    </div>
  {/if}

  <div class="row">
    {#if steppers}
      <button
        type="button"
        class="step"
        tabindex="-1"
        aria-label={`${t('slider.decrease')}: ${label}`}
        disabled={disabled || value <= min}
        onpointerdown={(event) => press(-1, event)}
        onpointerup={release}
        onpointercancel={release}
        onclick={(event) => keyStep(-1, event)}
      >
        <Icon name="minus" size={12} />
      </button>
    {/if}

    <input
      class="range"
      type="range"
      {min}
      {max}
      {step}
      {value}
      {disabled}
      aria-label={label}
      aria-valuetext={text}
      style="--fill: {fill}%"
      {@attach tooltip(showLabel ? undefined : hint)}
      oninput={(event) => oninput?.(Number(event.currentTarget.value))}
      onchange={(event) => onchange?.(Number(event.currentTarget.value))}
      ondblclick={() => onreset?.()}
    />

    {#if steppers}
      <button
        type="button"
        class="step"
        tabindex="-1"
        aria-label={`${t('slider.increase')}: ${label}`}
        disabled={disabled || value >= max}
        onpointerdown={(event) => press(1, event)}
        onpointerup={release}
        onpointercancel={release}
        onclick={(event) => keyStep(1, event)}
      >
        <Icon name="plus" size={12} />
      </button>
    {/if}

    {#if !showLabel}
      <output class="value inline">{text}</output>
    {/if}
  </div>
</div>

<style>
  .slider {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    font-size: var(--vk-fs-small);
    font-weight: 700;
  }

  .label {
    min-width: 0;
    overflow: hidden;
    color: var(--vk-text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .value {
    flex: none;
    min-width: 3ch;
    color: var(--vk-text);
    font-family: var(--vk-font-mono);
    font-size: var(--vk-fs-small);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .value.inline {
    min-width: 4ch;
    color: var(--vk-text-secondary);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* ---- − e + ---- */

  .step {
    display: grid;
    flex: none;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--vk-stroke);
    border-radius: 50%;
    background: var(--vk-input);
    color: var(--vk-text-secondary);
    cursor: pointer;
    transition:
      border-color var(--vk-dur-fast) var(--vk-ease),
      color var(--vk-dur-fast) var(--vk-ease),
      background var(--vk-dur-fast) var(--vk-ease);
  }

  .step:hover:not(:disabled) {
    border-color: #3a4c74;
    background: var(--vk-primary-surface-hover);
    color: var(--vk-text);
  }

  .step:active:not(:disabled) {
    transform: scale(0.94);
  }

  .step:disabled {
    opacity: 0.35;
    cursor: default;
  }

  /* ---- Il cursore ---- */

  .range {
    flex: 1;
    min-width: 0;
    height: 24px;
    margin: 0;
    background: transparent;
    cursor: pointer;
    appearance: none;
    -webkit-appearance: none;
  }

  .range:focus-visible {
    outline: none;
  }

  /*
   * L'arcobaleno sta tutto nel tratto pieno, come nelle barre: un cursore
   * vicino all'inizio non deve sembrare solo rosa.
   */
  .range::-webkit-slider-runnable-track {
    height: 6px;
    border-radius: var(--vk-radius-pill);
    background:
      var(--vk-rainbow) 0 0 / var(--fill) 100% no-repeat,
      #1a2236;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.06);
  }

  .range::-webkit-slider-thumb {
    width: 18px;
    height: 18px;
    margin-top: -6px;
    border: 3px solid var(--vk-panel);
    border-radius: 50%;
    background: #fff;
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.35),
      0 2px 6px rgb(0 0 0 / 0.55);
    transition:
      transform var(--vk-dur-fast) var(--vk-ease),
      box-shadow var(--vk-dur-fast) var(--vk-ease);
    appearance: none;
    -webkit-appearance: none;
  }

  .range:hover::-webkit-slider-thumb {
    transform: scale(1.1);
  }

  .range:active::-webkit-slider-thumb {
    transform: scale(1.15);
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.45),
      0 0 0 7px rgb(255 255 255 / 0.08),
      0 2px 6px rgb(0 0 0 / 0.55);
  }

  .range:focus-visible::-webkit-slider-thumb {
    box-shadow:
      0 0 0 2px #fff,
      0 0 0 5px rgb(255 255 255 / 0.18),
      0 2px 6px rgb(0 0 0 / 0.55);
  }

  .disabled .range {
    cursor: not-allowed;
    opacity: 0.45;
  }

  @media (prefers-reduced-motion: reduce) {
    .range::-webkit-slider-thumb,
    .step {
      transition: none;
    }
  }
</style>
