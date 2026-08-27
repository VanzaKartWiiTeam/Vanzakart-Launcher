<script lang="ts">
  /**
   * Interruttore acceso/spento.
   *
   * Prende il posto della coppia di pulsanti "Attiva"/"Disattiva": due
   * pulsanti per uno stato binario costringono a leggerli per capire in quale
   * dei due si è, mentre la levetta lo mostra e basta.
   *
   * È un `role="switch"`: barra spaziatrice e invio lo commutano da tastiera
   * come un pulsante qualsiasi, e uno screen reader lo annuncia come acceso o
   * spento invece di leggerne l'etichetta.
   */
  interface Props {
    checked: boolean;
    /** Descrizione per chi non vede la levetta. */
    label: string;
    disabled?: boolean;
    /** In attesa della risposta del backend: la levetta non si tocca. */
    busy?: boolean;
    onchange: (next: boolean) => void;
  }

  const { checked, label, disabled = false, busy = false, onchange }: Props = $props();
</script>

<button
  type="button"
  role="switch"
  class="switch"
  class:on={checked}
  class:busy
  aria-checked={checked}
  aria-label={label}
  title={label}
  disabled={disabled || busy}
  onclick={() => onchange(!checked)}
>
  <span class="track"></span>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 46px;
    height: 26px;
    padding: 0;
    border: 0;
    background: none;
    flex: none;
  }

  .switch:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  /*
   * Da spento la barra è scura con un bordo neutro. Da acceso il bordo porta
   * l'arcobaleno **fermo e intero**, largo quanto la levetta: se scorresse,
   * ogni switch mostrerebbe la fetta corrispondente alla propria fase — le
   * animazioni partono quando l'elemento nasce, non insieme — e una fila di
   * levette uscirebbe di sei colori diversi.
   */
  .track {
    position: absolute;
    inset: 0;
    border: 1.6px solid var(--vk-stroke);
    border-radius: var(--vk-radius-pill);
    background: var(--vk-input);
    transition:
      background var(--vk-dur) var(--vk-ease),
      border-color var(--vk-dur) var(--vk-ease),
      box-shadow var(--vk-dur) var(--vk-ease);
  }

  .switch.on .track {
    border-color: transparent;
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    box-shadow:
      0 0 9px rgb(255 0 102 / 0.22),
      0 0 9px rgb(0 242 255 / 0.2);
  }

  .switch:hover:not(:disabled) .track {
    border-color: #3a4c74;
  }

  .switch.on:hover:not(:disabled) .track {
    border-color: transparent;
  }

  /* La pallina è bianca sempre: il colore lo porta il bordo, e una pallina che
     cambia tinta a ogni levetta è solo rumore. */
  .knob {
    position: absolute;
    top: 4px;
    left: 4px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.45);
    opacity: 0.55;
    transition:
      transform var(--vk-dur) var(--vk-ease),
      opacity var(--vk-dur) var(--vk-ease);
  }

  .switch.on .knob {
    transform: translateX(20px);
    opacity: 1;
  }

  /* Mentre si aspetta il backend la pallina pulsa: la levetta ha già cambiato
     posizione, ma l'operazione non è finita. */
  .switch.busy .knob {
    animation: vk-switch-wait 0.9s ease-in-out infinite;
  }

  @keyframes vk-switch-wait {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .switch.busy .knob {
      animation: none;
    }
  }
</style>
