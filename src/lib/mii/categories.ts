/**
 * Categorie e controlli dell'editor Mii.
 *
 * Le categorie e il loro ordine vengono da `Launcher/MiiEditorWindow.xaml(.cs)`
 * (`BuildCategories`): è l'ordine che l'utente del legacy conosce. Dentro
 * ogni categoria ci sono i controlli dell'editor nativo (§D-092):
 *
 * - `parts`: la griglia di **icone** di un tratto, tutte insieme, senza
 *   pagine e senza render — sceglierne una aggiorna l'anteprima all'istante;
 * - `swatches`: le pastiglie di una tavolozza di colori;
 * - `slider`: un cursore con i pulsanti − e +;
 * - `features`: i tratti del viso (trucco, rughe), che un'icona non sa
 *   mostrare e che quindi restano miniature renderizzate;
 * - `switch` e `toggle`: le scelte sì/no.
 *
 * Nessun controllo porta i propri limiti: arrivano dal backend
 * (`mii_editor_limits`, cioè `vk_save::mii::LIMITS`), l'unico posto in cui
 * esistono. Così la UI non può proporre un valore che il gioco rifiuta.
 */
import type {
  MiiBooleanField,
  MiiEditorState,
  MiiFieldLimit,
  MiiNumericField
} from '$lib/api/types';
import type { TranslationKey } from '$lib/stores/i18n.svelte';
import type { PaletteName, PartIconKind } from './icons';

export type Control =
  | { kind: 'parts'; label: TranslationKey; field: MiiNumericField; icon: PartIconKind }
  | { kind: 'swatches'; label: TranslationKey; field: MiiNumericField; palette: PaletteName }
  | { kind: 'features'; label: TranslationKey; field: 'facialFeature' }
  | { kind: 'slider'; label: TranslationKey; field: MiiNumericField; invert?: boolean }
  | {
      kind: 'switch';
      label: TranslationKey;
      field: MiiBooleanField;
      off: TranslationKey;
      on: TranslationKey;
    }
  | { kind: 'toggle'; label: TranslationKey; field: MiiBooleanField };

/**
 * Etichette e descrizioni sono **chiavi di traduzione**, non testo: la
 * tabella è statica, la lingua no (vedi `stores/i18n.svelte.ts`).
 */
export interface Category {
  key: string;
  label: TranslationKey;
  hint: TranslationKey;
  /** Il tratto che la categoria mostra nella barra, con il valore corrente. */
  icon?: { kind: PartIconKind; field: MiiNumericField };
  controls: Control[];
}

// I cursori "Altezza" sono invertiti: nel formato un valore più alto sposta
// il tratto più in basso, e un cursore che scende andando a destra confonde.
export const CATEGORIES: [Category, ...Category[]] = [
  {
    key: 'base',
    label: 'miicat.base',
    hint: 'miicat.base.hint',
    controls: [
      {
        kind: 'switch',
        label: 'miicat.sex',
        field: 'isFemale',
        off: 'miicat.male',
        on: 'miicat.female'
      },
      { kind: 'toggle', label: 'miicat.favorite', field: 'isFavorite' },
      { kind: 'slider', label: 'miicat.bodyHeight', field: 'height' },
      { kind: 'slider', label: 'miicat.build', field: 'weight' },
      { kind: 'slider', label: 'miicat.birthMonth', field: 'birthMonth' },
      { kind: 'slider', label: 'miicat.birthDay', field: 'birthDay' }
    ]
  },
  {
    key: 'colors',
    label: 'miicat.colors',
    hint: 'miicat.colors.hint',
    controls: [
      {
        kind: 'swatches',
        label: 'miicat.favoriteColor',
        field: 'favoriteColorIndex',
        palette: 'favorite'
      }
    ]
  },
  {
    key: 'face',
    label: 'miicat.face',
    hint: 'miicat.face.hint',
    icon: { kind: 'face', field: 'faceShape' },
    controls: [
      { kind: 'parts', label: 'miicat.faceShapes', field: 'faceShape', icon: 'face' },
      { kind: 'swatches', label: 'miicat.skin', field: 'skinColor', palette: 'skin' },
      { kind: 'features', label: 'miicat.features', field: 'facialFeature' }
    ]
  },
  {
    key: 'hair',
    label: 'miicat.hair',
    hint: 'miicat.hair.hint',
    icon: { kind: 'hair', field: 'hairType' },
    controls: [
      { kind: 'parts', label: 'miicat.hairStyles', field: 'hairType', icon: 'hair' },
      { kind: 'swatches', label: 'miicat.color', field: 'hairColor', palette: 'hair' },
      { kind: 'toggle', label: 'miicat.mirror', field: 'hairFlipped' }
    ]
  },
  {
    key: 'eyes',
    label: 'miicat.eyes',
    hint: 'miicat.eyes.hint',
    icon: { kind: 'eye', field: 'eyeType' },
    controls: [
      { kind: 'parts', label: 'miicat.eyeShapes', field: 'eyeType', icon: 'eye' },
      { kind: 'swatches', label: 'miicat.color', field: 'eyeColor', palette: 'eye' },
      { kind: 'slider', label: 'miicat.size', field: 'eyeSize' },
      { kind: 'slider', label: 'miicat.rotation', field: 'eyeRotation' },
      { kind: 'slider', label: 'miicat.spacing', field: 'eyeSpacing' },
      { kind: 'slider', label: 'miicat.vertical', field: 'eyeVertical', invert: true }
    ]
  },
  {
    key: 'brows',
    label: 'miicat.brows',
    hint: 'miicat.brows.hint',
    icon: { kind: 'eyebrow', field: 'eyebrowType' },
    controls: [
      { kind: 'parts', label: 'miicat.browShapes', field: 'eyebrowType', icon: 'eyebrow' },
      { kind: 'swatches', label: 'miicat.color', field: 'eyebrowColor', palette: 'hair' },
      { kind: 'slider', label: 'miicat.size', field: 'eyebrowSize' },
      { kind: 'slider', label: 'miicat.rotation', field: 'eyebrowRotation' },
      { kind: 'slider', label: 'miicat.spacing', field: 'eyebrowSpacing' },
      { kind: 'slider', label: 'miicat.vertical', field: 'eyebrowVertical', invert: true }
    ]
  },
  {
    key: 'nose',
    label: 'miicat.nose',
    hint: 'miicat.nose.hint',
    icon: { kind: 'nose', field: 'noseType' },
    controls: [
      { kind: 'parts', label: 'miicat.noseShapes', field: 'noseType', icon: 'nose' },
      { kind: 'slider', label: 'miicat.size', field: 'noseSize' },
      { kind: 'slider', label: 'miicat.vertical', field: 'noseVertical', invert: true }
    ]
  },
  {
    key: 'mouth',
    label: 'miicat.mouth',
    hint: 'miicat.mouth.hint',
    icon: { kind: 'mouth', field: 'mouthType' },
    controls: [
      { kind: 'parts', label: 'miicat.mouthShapes', field: 'mouthType', icon: 'mouth' },
      { kind: 'swatches', label: 'miicat.color', field: 'mouthColor', palette: 'mouth' },
      { kind: 'slider', label: 'miicat.size', field: 'mouthSize' },
      { kind: 'slider', label: 'miicat.vertical', field: 'mouthVertical', invert: true }
    ]
  },
  {
    key: 'beard',
    label: 'miicat.beard',
    hint: 'miicat.beard.hint',
    icon: { kind: 'mustache', field: 'mustacheType' },
    controls: [
      { kind: 'parts', label: 'miicat.mustache', field: 'mustacheType', icon: 'mustache' },
      { kind: 'parts', label: 'miicat.beard', field: 'beardType', icon: 'beard' },
      { kind: 'swatches', label: 'miicat.color', field: 'facialHairColor', palette: 'hair' },
      { kind: 'slider', label: 'miicat.mustacheSize', field: 'mustacheSize' },
      {
        kind: 'slider',
        label: 'miicat.mustacheVertical',
        field: 'mustacheVertical',
        invert: true
      }
    ]
  },
  {
    key: 'glasses',
    label: 'miicat.glasses',
    hint: 'miicat.glasses.hint',
    icon: { kind: 'glasses', field: 'glassesType' },
    controls: [
      { kind: 'parts', label: 'miicat.glasses', field: 'glassesType', icon: 'glasses' },
      { kind: 'swatches', label: 'miicat.color', field: 'glassesColor', palette: 'glasses' },
      { kind: 'slider', label: 'miicat.size', field: 'glassesSize' },
      { kind: 'slider', label: 'miicat.vertical', field: 'glassesVertical', invert: true }
    ]
  },
  {
    key: 'mole',
    label: 'miicat.mole',
    hint: 'miicat.mole.hint',
    controls: [
      {
        kind: 'switch',
        label: 'miicat.mole',
        field: 'moleEnabled',
        off: 'miicat.moleOff',
        on: 'miicat.moleOn'
      },
      { kind: 'slider', label: 'miicat.size', field: 'moleSize' },
      { kind: 'slider', label: 'miicat.moleVertical', field: 'moleVertical', invert: true },
      { kind: 'slider', label: 'miicat.moleHorizontal', field: 'moleHorizontal' }
    ]
  }
];

/** Nomi dei tratti del viso, nell'ordine del formato. */
export const FACIAL_FEATURES: TranslationKey[] = [
  'miicat.feature0',
  'miicat.feature1',
  'miicat.feature2',
  'miicat.feature3',
  'miicat.feature4',
  'miicat.feature5',
  'miicat.feature6',
  'miicat.feature7',
  'miicat.feature8',
  'miicat.feature9',
  'miicat.feature10',
  'miicat.feature11'
];

// ---------------------------------------------------------------------------
// Limiti
// ---------------------------------------------------------------------------

export interface Range {
  min: number;
  max: number;
}

export type Limits = Partial<Record<MiiNumericField, Range>>;

/** La tabella del backend come mappa per campo. */
export function toLimits(list: MiiFieldLimit[]): Limits {
  const limits: Limits = {};
  for (const { field, min, max } of list) limits[field] = { min, max };
  return limits;
}

/** Giorni del mese, con il 29 febbraio: come `vk_save::mii::days_in_month`. */
export function daysInMonth(month: number): number {
  if (month === 2) return 29;
  return [4, 6, 9, 11].includes(month) ? 30 : 31;
}

/**
 * L'intervallo di un campo per lo stato corrente. Il giorno di nascita è
 * l'unico che dipende da un altro campo: febbraio non ha il 31.
 */
export function rangeOf(field: MiiNumericField, state: MiiEditorState, limits: Limits): Range {
  const range = limits[field] ?? { min: 0, max: 0 };
  if (field === 'birthDay') {
    return { min: range.min, max: Math.min(range.max, daysInMonth(state.birthMonth)) };
  }
  return range;
}

/**
 * Lo stato con ogni campo dentro il suo intervallo, come `normalized` nel
 * backend. Serve a tenere coerente la UI — cambiare mese sposta il giorno —
 * prima ancora di salvare.
 */
export function clampState(state: MiiEditorState, limits: Limits): MiiEditorState {
  const next = { ...state };
  for (const field of Object.keys(limits) as MiiNumericField[]) {
    const { min, max } = rangeOf(field, next, limits);
    const value = Number(next[field]);
    next[field] = Math.min(max, Math.max(min, Number.isFinite(value) ? Math.round(value) : min));
  }
  return next;
}

/** Il valore mostrato da un cursore: invertito per le altezze. */
export function sliderPosition(value: number, range: Range, invert = false): number {
  return invert ? range.min + range.max - value : value;
}

/** Il valore del campo per una posizione del cursore: l'inverso del precedente. */
export function sliderValue(position: number, range: Range, invert = false): number {
  const clamped = Math.min(range.max, Math.max(range.min, Math.round(position)));
  return invert ? range.min + range.max - clamped : clamped;
}

/**
 * Simboli inseribili nel nome, da `BuildNameSymbolButtons`.
 *
 * Sono quelli che la tastiera della Wii offre e che il gioco sa disegnare:
 * il nome è UTF-16 dentro i 74 byte, quindi ci stanno.
 */
export const NAME_SYMBOLS = [
  '★',
  '☆',
  '♡',
  '♥',
  '♦',
  '♣',
  '♠',
  '♪',
  '♫',
  '☀',
  '☁',
  '☂',
  '→',
  '←',
  '↑',
  '↓',
  '↔',
  '✓',
  '✕',
  '?',
  '!',
  '…',
  '・',
  '。',
  '①',
  '②',
  '③',
  '④',
  '⑤',
  '⑥',
  '⑦',
  '⑧',
  '⑨',
  '⑩',
  'ⓐ',
  'ⓑ',
  'Ⓐ',
  'Ⓑ',
  'Ⓢ',
  'Ⓜ',
  '©',
  '®',
  '™'
];
