<script lang="ts" module>
  /**
   * Icone SVG che sostituiscono i glyph Segoe MDL2 del launcher WPF.
   *
   * Il font esiste solo su Windows: su macOS e Linux i glyph sarebbero
   * quadrati vuoti. La corrispondenza glyph → icona è in docs/ui-parity.md §3.
   */
  export type IconName =
    | 'play' // E768
    | 'news' // E8A5
    | 'rooms' // E716
    | 'trophy' // ED39
    | 'friends' // E902
    | 'package' // E7B8
    | 'license' // E77B
    | 'settings' // E713
    | 'debug' // E943
    | 'refresh' // E72C
    | 'download' // E118
    | 'repair' // E90F
    | 'minimize' // E921
    | 'maximize'
    | 'restore'
    | 'close' // E8BB
    | 'folder'
    | 'check'
    | 'warning'
    | 'external'
    | 'plus'
    | 'edit'
    | 'copy'
    | 'trash'
    | 'swap'
    | 'save'
    | 'heart'
    | 'chevron';

  const PATHS: Record<IconName, string> = {
    play: 'M8 5v14l11-7z',
    news: 'M4 4h13a1 1 0 0 1 1 1v13a2 2 0 0 0 2 2H6a2 2 0 0 1-2-2zm3 4h7v2H7zm0 4h7v2H7zm0 4h5v2H7z',
    rooms:
      'M9 12a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7m7.5 1a2.75 2.75 0 1 0 0-5.5 2.75 2.75 0 0 0 0 5.5M9 13.5c-3 0-6 1.5-6 3.75V19h12v-1.75c0-2.25-3-3.75-6-3.75m7.5.5c-.8 0-1.6.13-2.3.38 1.1.86 1.8 2 1.8 3.37V19H21v-1.5c0-1.9-2.2-3-4.5-3',
    trophy:
      'M6 4h12v2h3v3a4 4 0 0 1-4 4h-.4A6 6 0 0 1 13 15.9V18h3v2H8v-2h3v-2.1a6 6 0 0 1-3.6-2.9H7a4 4 0 0 1-4-4V6h3zm0 4H5v1a2 2 0 0 0 1 1.7zm12 0v2.7A2 2 0 0 0 19 9V8z',
    friends:
      'M10 12a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7m0 1.5c-3.3 0-6.5 1.6-6.5 4V19h13v-1.5c0-2.4-3.2-4-6.5-4M18 8h2v3h3v2h-3v3h-2v-3h-3v-2h3z',
    package:
      'M12 2 3 6.5v11L12 22l9-4.5v-11zm0 2.2 6.4 3.2L12 10.6 5.6 7.4zM5 9.3l6 3v7.1l-6-3zm8 10.1v-7.1l6-3v7.1z',
    license:
      'M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1m4.5 3A2.25 2.25 0 1 0 8.5 12.5 2.25 2.25 0 0 0 8.5 8M5 16.2c0-1.5 2.3-2.3 3.5-2.3s3.5.8 3.5 2.3V17H5zM14 9h5v1.6h-5zm0 3.2h5v1.6h-5z',
    settings:
      'M12 8.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7m8.2 3.5c0 .5 0 1-.1 1.4l2 1.6-2 3.4-2.4-1a7.6 7.6 0 0 1-2.4 1.4L15 21h-4l-.3-2.6a7.6 7.6 0 0 1-2.4-1.4l-2.4 1-2-3.4 2-1.6a8.5 8.5 0 0 1 0-2.8l-2-1.6 2-3.4 2.4 1a7.6 7.6 0 0 1 2.4-1.4L11 3h4l.3 2.6c.9.3 1.7.8 2.4 1.4l2.4-1 2 3.4-2 1.6c.1.5.1.9.1 1.4',
    debug: 'M9.4 16.6 4.8 12l4.6-4.6L8 6l-6 6 6 6zm5.2 0 4.6-4.6-4.6-4.6L16 6l6 6-6 6z',
    refresh:
      'M12 5V2L8 6l4 4V7a5 5 0 1 1-5 5H5a7 7 0 1 0 7-7m6.4 2.6A7 7 0 0 1 19 12h2a9 9 0 0 0-.8-3.7z',
    download: 'M12 3v9.6l3.3-3.3 1.4 1.4L12 15.4l-4.7-4.7 1.4-1.4L12 12.6zM5 18h14v2H5z',
    repair:
      'm17.7 6.3-2.6 2.6-1.4-1.4 2.6-2.6a5 5 0 0 0-6.4 6.4l-6.3 6.3 2.8 2.8 6.3-6.3a5 5 0 0 0 6.4-6.4z',
    minimize: 'M5 11h14v2H5z',
    maximize: 'M5 5h14v14H5zm2 2v10h10V7z',
    // Due riquadri sovrapposti, come il glifo "Restore" di Windows.
    restore: 'M9 3h12v12h-3v-2h1V5H11v1H9zM3 9h12v12H3zm2 2v8h8v-8z',
    close:
      'M18.3 5.7 12 12l6.3 6.3-1.4 1.4L10.6 13.4 4.3 19.7 2.9 18.3 9.2 12 2.9 5.7 4.3 4.3l6.3 6.3 6.3-6.3z',
    folder: 'M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
    check: 'M9.6 16.2 5.4 12l-1.4 1.4 5.6 5.6L20.4 8.2 19 6.8z',
    warning: 'M12 3 1.5 21h21zm0 5 6.6 11H5.4zm-1 4v4h2v-4zm0 5v2h2v-2z',
    external: 'M14 3h7v7h-2V6.4l-8.3 8.3-1.4-1.4L17.6 5H14zM5 5h5v2H7v10h10v-3h2v5H5z',
    plus: 'M11 5h2v6h6v2h-6v6h-2v-6H5v-2h6z',
    edit: 'M3 17.3V21h3.7L17.8 9.9l-3.7-3.7zm17.7-10.3a1 1 0 0 0 0-1.4l-2.3-2.3a1 1 0 0 0-1.4 0l-1.8 1.8 3.7 3.7z',
    copy: 'M15 1H4a2 2 0 0 0-2 2v13h2V3h11zm4 4H8a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2m0 16H8V7h11z',
    trash:
      'M6 19a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V7H6zm3-9h2v8H9zm4 0h2v8h-2zM18 4h-3.2l-1-1h-3.6l-1 1H6v2h12z',
    swap: 'M7.5 3 3 7.5l1.4 1.4L6.5 6.8V17h2V6.8l2.1 2.1L12 7.5zm9 18L21 16.5l-1.4-1.4-2.1 2.1V7h-2v10.2l-2.1-2.1L12 16.5z',
    save: 'M17 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V7zm-5 16a3 3 0 1 1 0-6 3 3 0 0 1 0 6m3-10H5V5h10z',
    heart:
      'M12 21.35 10.55 20C5.4 15.36 2 12.27 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.77-3.4 6.86-8.55 11.53z',
    chevron: 'M7.4 9.6 12 14.2l4.6-4.6L18 11l-6 6-6-6z'
  };
</script>

<script lang="ts">
  interface Props {
    name: IconName;
    size?: number;
    /** Testo alternativo; se assente l'icona è decorativa. */
    label?: string;
  }

  const { name, size = 18, label }: Props = $props();
</script>

<svg
  class="vk-icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="currentColor"
  aria-hidden={label ? undefined : 'true'}
  role={label ? 'img' : undefined}
  aria-label={label}
>
  {#if label}<title>{label}</title>{/if}
  <path d={PATHS[name]} />
</svg>

<style>
  .vk-icon {
    display: block;
    flex: none;
  }
</style>
