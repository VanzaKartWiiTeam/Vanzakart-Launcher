<script lang="ts">
  /**
   * Sfondo animato: porta l'`AmbientParticleCanvas` di MainWindow.xaml.
   *
   * Le due curve di Bézier usano esattamente gli stessi dati del WPF
   * (`M-120,130 C180,20 …` e `M-80,650 C250,500 …`), con lo stesso schema a
   * due livelli — traccia spessa sfocata sotto, traccia sottile luminosa
   * sopra — e le tre strisce colorate che scorrono in parallasse.
   */
</script>

<div class="backdrop" aria-hidden="true">
  <svg viewBox="0 0 1440 800" preserveAspectRatio="xMidYMid slice">
    <defs>
      <linearGradient id="vkRainbow" x1="0" y1="0" x2="1" y2="0">
        <stop offset="0" stop-color="#FF0066" />
        <stop offset="0.18" stop-color="#FF8800" />
        <stop offset="0.34" stop-color="#FFEA00" />
        <stop offset="0.5" stop-color="#00FF66" />
        <stop offset="0.67" stop-color="#00F2FF" />
        <stop offset="0.84" stop-color="#3300FF" />
        <stop offset="1" stop-color="#B000FF" />
      </linearGradient>

      <filter id="vkBlurBig" x="-30%" y="-30%" width="160%" height="160%">
        <feGaussianBlur stdDeviation="14" />
      </filter>
      <filter id="vkBlurMid" x="-30%" y="-30%" width="160%" height="160%">
        <feGaussianBlur stdDeviation="12" />
      </filter>
      <filter id="vkGlowSoft" x="-30%" y="-30%" width="160%" height="160%">
        <feGaussianBlur stdDeviation="6" />
      </filter>
    </defs>

    <g class="drift">
      <!-- Curva 1: strato profondo sfocato, poi traccia sottile. -->
      <path
        d="M-120,130 C180,20 330,260 650,112 S1040,160 1460,48"
        stroke="url(#vkRainbow)"
        stroke-width="20"
        fill="none"
        opacity="0.5"
        filter="url(#vkBlurBig)"
      />
      <path
        d="M-120,130 C180,20 330,260 650,112 S1040,160 1460,48"
        stroke="url(#vkRainbow)"
        stroke-width="3"
        fill="none"
        opacity="0.8"
        filter="url(#vkGlowSoft)"
      />

      <!-- Curva 2, ciano. -->
      <path
        d="M-80,650 C250,500 430,720 720,560 S1060,540 1420,430"
        stroke="#00F2FF"
        stroke-width="18"
        fill="none"
        opacity="0.45"
        filter="url(#vkBlurMid)"
      />
      <path
        d="M-80,650 C250,500 430,720 720,560 S1060,540 1420,430"
        stroke="#00F2FF"
        stroke-width="2.5"
        fill="none"
        opacity="0.75"
        filter="url(#vkGlowSoft)"
      />
    </g>

    <!-- Le tre strisce, con le stesse posizioni e colori del WPF. -->
    <rect class="streak streak--a" x="880" y="154" width="260" height="2" rx="1" fill="#39E7FF" />
    <rect class="streak streak--b" x="620" y="690" width="180" height="2" rx="1" fill="#FF3B7A" />
    <rect class="streak streak--c" x="1120" y="420" width="140" height="2" rx="1" fill="#FFD166" />
  </svg>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
    opacity: 0.85;
  }

  svg {
    width: 100%;
    height: 100%;
  }

  .drift {
    animation: vk-drift 24s ease-in-out infinite alternate;
  }

  .streak {
    opacity: 0.4;
    filter: drop-shadow(0 0 12px currentColor);
  }

  .streak--a {
    animation: vk-streak 7s linear infinite;
  }

  .streak--b {
    opacity: 0.35;
    animation: vk-streak 9s linear infinite 1.5s;
  }

  .streak--c {
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
      transform: translateX(-320px);
      opacity: 0;
    }
    12% {
      opacity: 0.45;
    }
    88% {
      opacity: 0.45;
    }
    100% {
      transform: translateX(360px);
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
