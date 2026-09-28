/**
 * Icone dei tratti Mii e tavolozze dell'editor.
 *
 * Le icone stanno in `icons.json`, generato da `scripts/convert-mii-icons.py`:
 * un'icona per ogni valore di
 * ogni tratto, nell'ordine dell'indice. Mostrare un'icona invece di un render
 * è ciò che rende la griglia istantanea: nessuna richiesta, nessuna attesa,
 * tutte le 72 acconciature insieme (§D-092).
 *
 * Ogni strato di un'icona ha un colore "di ruolo" (1–5) che qui diventa il
 * colore vero del Mii: l'iride prende il colore degli occhi, i capelli quello
 * dei capelli, il cappello il colore preferito.
 */
import type { MiiEditorState } from '$lib/api/types';

export type PartIconKind =
  'face' | 'hair' | 'eye' | 'eyebrow' | 'nose' | 'mouth' | 'mustache' | 'beard' | 'glasses';

/** Un tracciato: riempimento e contorno sono un ruolo (1–5) o un colore. */
export interface IconLayer {
  d: string;
  fill?: number | string;
  stroke?: number | string;
  width?: number;
}

export interface PartIcon {
  /** Riquadro del contenuto, contorni compresi: diventa il `viewBox`. */
  box: [number, number, number, number];
  layers: IconLayer[];
}

export type IconSet = Record<PartIconKind, PartIcon[]>;

let loading: Promise<IconSet> | null = null;

/**
 * Le icone, caricate una volta sola e solo quando serve: sono duecento
 * kilobyte di tracciati che fuori dall'editor nessuno guarda.
 */
export function loadIcons(): Promise<IconSet> {
  loading ??= import('./icons.json').then((module) => module.default as unknown as IconSet);
  return loading;
}

// ---------------------------------------------------------------------------
// Tavolozze
// ---------------------------------------------------------------------------

/**
 * Colori dei tratti per le pastiglie dell'editor e per le icone. Il colore
 * preferito resta quello del
 * backend (`mii_favorite_colors`), lo stesso che colora gli avatar.
 */
export const PALETTES = {
  skin: ['#ffd39d', '#ffb963', '#de7b3d', '#ffab80', '#c85327', '#752e17'],
  hair: ['#000000', '#562d1b', '#782515', '#9d4a20', '#988b8c', '#684e1b', '#ab6a24', '#ffb757'],
  eye: ['#000000', '#474b5d', '#96482d', '#a59837', '#555dc3', '#488f64'],
  glasses: ['#909090', '#ca9366', '#ff574d', '#7b87bd', '#ffaf47', '#dcc5be'],
  mouth: ['#ff5d0d', '#ff120d', '#ff534d']
} as const;

export type PaletteName = keyof typeof PALETTES | 'favorite';

/** Labbro superiore per colore della bocca: più scuro di quello inferiore. */
const LIP_TOP = ['#a73b1e', '#9a1312', '#af292f'];

const NONE_COLOR = '#ff6581';
const OUTLINE = '#24252d';
const BORDER = 'rgb(0 0 0 / 0.28)';

function pick(list: readonly string[], index: number): string {
  return list[Math.min(Math.max(index, 0), list.length - 1)] ?? list[0] ?? '#000000';
}

/**
 * I cinque colori di ruolo di un tipo di icona, per il Mii che si sta
 * modificando.
 */
export function iconColors(
  kind: PartIconKind,
  state: MiiEditorState,
  favorites: readonly string[]
): string[] {
  const skin = pick(PALETTES.skin, state.skinColor);
  const favorite = pick(favorites, state.favoriteColorIndex);

  switch (kind) {
    case 'face':
      return [skin, BORDER, 'transparent', favorite, favorite];
    case 'hair':
      return [
        skin,
        BORDER,
        pick(PALETTES.hair, state.hairColor),
        favorite,
        `color-mix(in srgb, ${favorite} 55%, black)`
      ];
    case 'eye':
      return ['#f6f7f9', OUTLINE, pick(PALETTES.eye, state.eyeColor), '#ffffff', '#ffffff'];
    case 'eyebrow':
      return [NONE_COLOR, pick(PALETTES.hair, state.eyebrowColor), OUTLINE, OUTLINE, OUTLINE];
    case 'nose':
      return [OUTLINE, OUTLINE, OUTLINE, OUTLINE, OUTLINE];
    case 'mouth':
      return [
        pick(LIP_TOP, state.mouthColor),
        pick(PALETTES.mouth, state.mouthColor),
        OUTLINE,
        '#ffffff',
        '#ffffff'
      ];
    case 'mustache':
    case 'beard':
      return [skin, BORDER, pick(PALETTES.hair, state.facialHairColor), NONE_COLOR, NONE_COLOR];
    case 'glasses':
      return [skin, BORDER, pick(PALETTES.glasses, state.glassesColor), NONE_COLOR, NONE_COLOR];
  }
}

/** Un ruolo diventa un colore; un colore diretto resta com'è. */
export function layerColor(value: number | string | undefined, colors: string[]): string {
  if (value === undefined) return 'none';
  if (typeof value === 'number') return colors[value - 1] ?? 'none';
  return value;
}
