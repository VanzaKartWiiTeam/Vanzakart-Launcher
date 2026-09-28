<script lang="ts">
  /**
   * L'icona di un tratto Mii: un occhio, un'acconciatura, un paio di occhiali.
   *
   * Disegna i tracciati di `icons.json` nel loro riquadro, con i colori di
   * ruolo già risolti per il Mii corrente (vedi `$lib/mii/icons`). Il riempimento
   * è `evenodd` come nelle `DrawingImage` di Avalonia da cui vengono.
   */
  import { layerColor } from '$lib/mii/icons';
  import type { PartIcon } from '$lib/mii/icons';

  interface Props {
    icon: PartIcon;
    colors: string[];
  }

  const { icon, colors }: Props = $props();

  const viewBox = $derived(icon.box.join(' '));
</script>

<svg {viewBox} preserveAspectRatio="xMidYMid meet" aria-hidden="true">
  {#each icon.layers as layer, index (index)}
    <path
      d={layer.d}
      fill={layerColor(layer.fill, colors)}
      stroke={layerColor(layer.stroke, colors)}
      stroke-width={layer.width ?? 0}
      fill-rule="evenodd"
    />
  {/each}
</svg>

<style>
  svg {
    display: block;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
</style>
