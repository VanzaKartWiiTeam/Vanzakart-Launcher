<script lang="ts">
  /**
   * La faccia di un Mii, renderizzata davvero.
   *
   * Porta il comportamento del launcher legacy: il render arriva dal servizio
   * immagini di Mii Studio a partire dalla "studio data" del Mii, e finché non
   * c'è — o se non arriva — resta la silhouette con l'iniziale sul colore
   * preferito, che è lo stesso fallback del WPF.
   *
   * Lo stesso componente serve le licenze, gli amici, i profili del launcher e
   * il selettore del Mii di una licenza: la faccia è sempre la stessa cosa.
   */
  import { renderStudio } from '$lib/mii/render';
  import { t } from '$lib/stores/i18n.svelte';
  import miiSilhouette from '$lib/assets/mii_silhouette.png';
  import type { MiiRenderKind } from '$lib/api';

  interface Props {
    /** Payload di render. Vuoto quando il Mii non è disponibile. */
    studioData: string;
    /** Iniziale mostrata finché la faccia non c'è. */
    initial?: string;
    /** Colore di fondo del fallback. */
    accent?: string;
    size?: number;
    /**
     * `circle` accanto a una riga, `rounded` per la card grande dell'elenco:
     * il legacy usa un riquadro 84x84 con angoli da 16 px.
     */
    shape?: 'circle' | 'rounded';
    kind?: MiiRenderKind;
    rotation?: number;
    /** Nome del Mii, per chi legge con uno screen reader. */
    name?: string;
  }

  const {
    studioData,
    initial = '?',
    accent = '#39E7FF',
    size = 64,
    shape = 'circle',
    kind = 'face',
    rotation = 0,
    name = ''
  }: Props = $props();

  let image = $state<string | null>(null);

  $effect(() => {
    const data = studioData;
    const shot = kind;
    const turn = rotation;

    let alive = true;
    image = null;

    if (!data.trim()) return;

    void renderStudio(data, shot, turn).then((rendered) => {
      if (alive) image = rendered;
    });

    return () => {
      alive = false;
    };
  });
</script>

<div
  class="avatar"
  class:rounded={shape === 'rounded'}
  style="--size: {size}px; --accent: {accent}"
  role="img"
  aria-label={name ? t('mii.avatarOf', { name }) : t('mii.avatar')}
>
  {#if image}
    <img src={image} alt="" />
  {:else}
    <img class="silhouette" src={miiSilhouette} alt="" />
    <span class="initial" aria-hidden="true">{initial}</span>
  {/if}
</div>

<style>
  .avatar {
    position: relative;
    display: grid;
    place-items: center;
    width: var(--size);
    height: var(--size);
    flex: none;
    border-radius: 50%;
    overflow: hidden;
    background: linear-gradient(140deg, var(--accent), rgb(0 0 0 / 0.35));
    box-shadow: 0 0 18px color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .avatar.rounded {
    border-radius: calc(var(--size) * 0.19);
    border: 1.5px solid color-mix(in srgb, var(--accent) 60%, transparent);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  /* La silhouette resta sotto l'iniziale: da sola non distingue due Mii. */
  .silhouette {
    opacity: 0.35;
  }

  .initial {
    position: absolute;
    color: #08111f;
    font-size: calc(var(--size) * 0.4);
    font-weight: 900;
    text-shadow: 0 1px 2px rgb(255 255 255 / 0.35);
  }
</style>
