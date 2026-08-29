/**
 * Categorie, griglie di scelta e cursori dell'editor Mii.
 *
 * Tabella estratta da `Launcher/MiiEditorWindow.xaml(.cs)`. Due cose vengono
 * da lì e non vanno cambiate a occhio:
 *
 * - le **categorie** e il loro ordine, da `BuildCategories`;
 * - i **massimi dei cursori**, che sono quelli degli `Slider` del WPF e non
 *   quelli assoluti del formato. Il gioco accetta indici più alti, ma
 *   producono un Mii deforme.
 *
 * Ogni categoria ha una griglia di scelte (`groups`) con l'anteprima
 * renderizzata di ogni opzione, come `BuildFeatureOptionsForSelectedCategory`,
 * e i cursori di rifinitura che nel WPF stavano nel popup "Adjust".
 *
 * Vive fuori dal componente perché è un dato verificabile: il test controlla
 * che ogni campo dello stato compaia una volta sola e che nessun intervallo
 * esca dai limiti del formato.
 */
import type { MiiBooleanField, MiiNumericField } from '$lib/api/types';
import type { TranslationKey } from '$lib/stores/i18n.svelte';

export interface Slider {
  field: MiiNumericField;
  label: TranslationKey;
  min: number;
  max: number;
}

export interface Toggle {
  field: MiiBooleanField;
  label: TranslationKey;
}

/**
 * Un gruppo di scelte nella griglia.
 *
 * `render` produce una miniatura per ogni indice fra `min` e `max`: è il Mii
 * corrente con quel solo tratto cambiato, renderizzato davvero.
 * `switch` sono le due scelte di un interruttore, anch'esse renderizzate.
 * `color` è la tavolozza del colore preferito, che non ha bisogno di un render.
 */
export type OptionGroup =
  | { kind: 'render'; label: TranslationKey; field: MiiNumericField; min: number; max: number }
  | {
      kind: 'switch';
      label: TranslationKey;
      field: MiiBooleanField;
      off: TranslationKey;
      on: TranslationKey;
    }
  | { kind: 'color'; label: TranslationKey };

/**
 * Etichette e descrizioni sono **chiavi di traduzione**, non testo: la
 * tabella è statica, la lingua no (vedi `stores/i18n.svelte.ts`).
 */
export interface Category {
  key: string;
  label: TranslationKey;
  hint: TranslationKey;
  groups: OptionGroup[];
  toggles: Toggle[];
  sliders: Slider[];
}

/** Opzioni per pagina nella griglia, come `OptionsPerPage` del legacy. */
export const OPTIONS_PER_PAGE = 6;

export const CATEGORIES: [Category, ...Category[]] = [
  {
    key: 'base',
    label: 'miicat.base',
    hint: 'miicat.base.hint',
    groups: [
      {
        kind: 'switch',
        label: 'miicat.sex',
        field: 'isFemale',
        off: 'miicat.male',
        on: 'miicat.female'
      }
    ],
    toggles: [{ field: 'isFavorite', label: 'miicat.favorite' }],
    sliders: [
      { field: 'height', label: 'miicat.bodyHeight', min: 0, max: 127 },
      { field: 'weight', label: 'miicat.build', min: 0, max: 127 },
      { field: 'birthMonth', label: 'miicat.birthMonth', min: 1, max: 12 },
      { field: 'birthDay', label: 'miicat.birthDay', min: 1, max: 31 }
    ]
  },
  {
    key: 'colors',
    label: 'miicat.colors',
    hint: 'miicat.colors.hint',
    groups: [{ kind: 'color', label: 'miicat.favoriteColor' }],
    toggles: [],
    sliders: []
  },
  {
    key: 'face',
    label: 'miicat.face',
    hint: 'miicat.face.hint',
    groups: [{ kind: 'render', label: 'miicat.faceShapes', field: 'faceShape', min: 0, max: 7 }],
    toggles: [],
    sliders: [
      { field: 'skinColor', label: 'miicat.skin', min: 0, max: 5 },
      { field: 'facialFeature', label: 'miicat.features', min: 0, max: 11 }
    ]
  },
  {
    key: 'hair',
    label: 'miicat.hair',
    hint: 'miicat.hair.hint',
    groups: [{ kind: 'render', label: 'miicat.hairStyles', field: 'hairType', min: 0, max: 71 }],
    toggles: [{ field: 'hairFlipped', label: 'miicat.mirror' }],
    sliders: [{ field: 'hairColor', label: 'miicat.color', min: 0, max: 7 }]
  },
  {
    key: 'eyes',
    label: 'miicat.eyes',
    hint: 'miicat.eyes.hint',
    groups: [{ kind: 'render', label: 'miicat.eyeShapes', field: 'eyeType', min: 0, max: 47 }],
    toggles: [],
    sliders: [
      { field: 'eyeRotation', label: 'miicat.rotation', min: 0, max: 7 },
      { field: 'eyeColor', label: 'miicat.color', min: 0, max: 5 },
      { field: 'eyeSize', label: 'miicat.size', min: 0, max: 7 },
      { field: 'eyeSpacing', label: 'miicat.spacing', min: 0, max: 12 },
      { field: 'eyeVertical', label: 'miicat.vertical', min: 0, max: 18 }
    ]
  },
  {
    key: 'brows',
    label: 'miicat.brows',
    hint: 'miicat.brows.hint',
    groups: [{ kind: 'render', label: 'miicat.browShapes', field: 'eyebrowType', min: 0, max: 23 }],
    toggles: [],
    sliders: [
      { field: 'eyebrowRotation', label: 'miicat.rotation', min: 0, max: 11 },
      { field: 'eyebrowColor', label: 'miicat.color', min: 0, max: 7 },
      { field: 'eyebrowSize', label: 'miicat.size', min: 0, max: 8 },
      { field: 'eyebrowSpacing', label: 'miicat.spacing', min: 0, max: 12 },
      { field: 'eyebrowVertical', label: 'miicat.vertical', min: 3, max: 18 }
    ]
  },
  {
    key: 'nose',
    label: 'miicat.nose',
    hint: 'miicat.nose.hint',
    groups: [{ kind: 'render', label: 'miicat.noseShapes', field: 'noseType', min: 0, max: 11 }],
    toggles: [],
    sliders: [
      { field: 'noseSize', label: 'miicat.size', min: 0, max: 8 },
      { field: 'noseVertical', label: 'miicat.vertical', min: 0, max: 18 }
    ]
  },
  {
    key: 'mouth',
    label: 'miicat.mouth',
    hint: 'miicat.mouth.hint',
    groups: [{ kind: 'render', label: 'miicat.mouthShapes', field: 'mouthType', min: 0, max: 23 }],
    toggles: [],
    sliders: [
      { field: 'mouthColor', label: 'miicat.color', min: 0, max: 2 },
      { field: 'mouthSize', label: 'miicat.size', min: 0, max: 8 },
      { field: 'mouthVertical', label: 'miicat.vertical', min: 0, max: 18 }
    ]
  },
  {
    key: 'beard',
    label: 'miicat.beard',
    hint: 'miicat.beard.hint',
    groups: [
      { kind: 'render', label: 'miicat.mustache', field: 'mustacheType', min: 0, max: 3 },
      { kind: 'render', label: 'miicat.beard', field: 'beardType', min: 0, max: 3 }
    ],
    toggles: [],
    sliders: [
      { field: 'facialHairColor', label: 'miicat.color', min: 0, max: 7 },
      { field: 'mustacheSize', label: 'miicat.mustacheSize', min: 0, max: 8 },
      { field: 'mustacheVertical', label: 'miicat.mustacheVertical', min: 0, max: 16 }
    ]
  },
  {
    key: 'glasses',
    label: 'miicat.glasses',
    hint: 'miicat.glasses.hint',
    groups: [{ kind: 'render', label: 'miicat.glasses', field: 'glassesType', min: 0, max: 8 }],
    toggles: [],
    sliders: [
      { field: 'glassesColor', label: 'miicat.color', min: 0, max: 5 },
      { field: 'glassesSize', label: 'miicat.size', min: 0, max: 7 },
      { field: 'glassesVertical', label: 'miicat.vertical', min: 0, max: 20 }
    ]
  },
  {
    key: 'mole',
    label: 'miicat.mole',
    hint: 'miicat.mole.hint',
    groups: [
      {
        kind: 'switch',
        label: 'miicat.mole',
        field: 'moleEnabled',
        off: 'miicat.moleOff',
        on: 'miicat.moleOn'
      }
    ],
    toggles: [],
    sliders: [
      { field: 'moleSize', label: 'miicat.size', min: 0, max: 8 },
      { field: 'moleVertical', label: 'miicat.moleVertical', min: 0, max: 18 },
      { field: 'moleHorizontal', label: 'miicat.moleHorizontal', min: 0, max: 16 }
    ]
  }
];

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
