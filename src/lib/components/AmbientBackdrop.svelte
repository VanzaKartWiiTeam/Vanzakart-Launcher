<script lang="ts">
  /**
   * Sfondo animato: porta l'`AmbientParticleCanvas` di MainWindow.xaml.
   *
   * Le due curve di Bézier usano esattamente gli stessi dati del WPF
   * (`M-120,130 C180,20 …` e `M-80,650 C250,500 …`), con lo stesso schema a
   * due livelli — traccia spessa sfocata sotto, traccia sottile luminosa
   * sopra — e le tre strisce colorate che scorrono in parallasse.
   *
   * La sfocatura è **CSS su livelli promossi**, non un `feGaussianBlur` SVG
   * (§D-082): un filtro SVG dentro un gruppo che si muove viene ricalcolato a
   * ogni fotogramma, e su una webview Retina significa sfocare qualche milione
   * di pixel sessanta (o centoventi) volte al secondo. Così invece il filtro
   * si applica una volta a un livello che poi viene solo traslato.
   */
</script>

<div class="backdrop" aria-hidden="true">
  <div class="drift">
    <!-- Strato profondo: tracce spesse, sfocate dal livello che le contiene. -->
    <svg class="layer glow" viewBox="0 0 1440 800" preserveAspectRatio="xMidYMid slice">
      <defs>
        <linearGradient id="vkRainbowGlow" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#FF0066" />
          <stop offset="0.18" stop-color="#FF8800" />
          <stop offset="0.34" stop-color="#FFEA00" />
          <stop offset="0.5" stop-color="#00FF66" />
          <stop offset="0.67" stop-color="#00F2FF" />
          <stop offset="0.84" stop-color="#3300FF" />
          <stop offset="1" stop-color="#B000FF" />
        </linearGradient>
      </defs>
      <path
        d="M-120,130 C180,20 330,260 650,112 S1040,160 1460,48"
        stroke="url(#vkRainbowGlow)"
        stroke-width="20"
        fill="none"
        opacity="0.5"
      />
      <path
        d="M-80,650 C250,500 430,720 720,560 S1060,540 1420,430"
        stroke="#00F2FF"
        stroke-width="18"
        fill="none"
        opacity="0.45"
      />
    </svg>

    <!-- Tracce sottili: appena sfocate, sopra le altre. -->
    <svg class="layer lines" viewBox="0 0 1440 800" preserveAspectRatio="xMidYMid slice">
      <defs>
        <linearGradient id="vkRainbowLine" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#FF0066" />
          <stop offset="0.18" stop-color="#FF8800" />
          <stop offset="0.34" stop-color="#FFEA00" />
          <stop offset="0.5" stop-color="#00FF66" />
          <stop offset="0.67" stop-color="#00F2FF" />
          <stop offset="0.84" stop-color="#3300FF" />
          <stop offset="1" stop-color="#B000FF" />
        </linearGradient>
      </defs>
      <path
        d="M-120,130 C180,20 330,260 650,112 S1040,160 1460,48"
        stroke="url(#vkRainbowLine)"
        stroke-width="3"
        fill="none"
        opacity="0.8"
      />
      <path
        d="M-80,650 C250,500 430,720 720,560 S1060,540 1420,430"
        stroke="#00F2FF"
        stroke-width="2.5"
        fill="none"
        opacity="0.75"
      />
    </svg>
  </div>

  <!--
    Le tre strisce, con le stesse posizioni e colori del WPF. Sono `div` e non
    `rect`: l'alone è un `box-shadow`, disegnato una volta nel livello, e
    l'animazione muove solo il livello.
  -->
  <div class="streak streak--a"></div>
  <div class="streak streak--b"></div>
  <div class="streak streak--c"></div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
    opacity: 0.85;
    /* Niente di quello che c'è qui dentro può influenzare il resto. */
    contain: strict;
  }

  .drift {
    position: absolute;
    inset: 0;
    will-change: transform;
    animation: vk-drift 24s ease-in-out infinite alternate;
  }

  .layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    /* Il livello viene rasterizzato una volta, filtro compreso. */
    transform: translateZ(0);
  }

  .glow {
    filter: blur(14px);
  }

  .lines {
    filter: blur(4px);
  }

  .streak {
    position: absolute;
    height: 2px;
    border-radius: 1px;
    opacity: 0.4;
    background: currentcolor;
    box-shadow: 0 0 12px currentcolor;
    will-change: transform;
  }

  /* Le posizioni sono quelle del WPF, in frazione della tela 1440×800. */
  .streak--a {
    left: 61.1%;
    top: 19.25%;
    width: 18.05%;
    color: #39e7ff;
    animation: vk-streak 7s linear infinite;
  }

  .streak--b {
    left: 43%;
    top: 86.25%;
    width: 12.5%;
    color: #ff3b7a;
    opacity: 0.35;
    animation: vk-streak 9s linear infinite 1.5s;
  }

  .streak--c {
    left: 77.7%;
    top: 52.5%;
    width: 9.7%;
    color: #ffd166;
    opacity: 0.3;
    animation: vk-streak 11s linear infinite 3s;
  }

  @keyframes vk-drift {
    from {
      transform: translate3d(-18px, -8px, 0);
    }
    to {
      transform: translate3d(18px, 8px, 0);
    }
  }

  @keyframes vk-streak {
    0% {
      transform: translate3d(-320px, 0, 0);
      opacity: 0;
    }
    12% {
      opacity: 0.45;
    }
    88% {
      opacity: 0.45;
    }
    100% {
      transform: translate3d(360px, 0, 0);
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .drift,
    .streak {
      animation: none;
    }
  }
</style>
