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

export interface Slider {
  field: MiiNumericField;
  label: string;
  min: number;
  max: number;
}

export interface Toggle {
  field: MiiBooleanField;
  label: string;
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
  | { kind: 'render'; label: string; field: MiiNumericField; min: number; max: number }
  | { kind: 'switch'; label: string; field: MiiBooleanField; off: string; on: string }
  | { kind: 'color'; label: string };

export interface Category {
  key: string;
  label: string;
  hint: string;
  groups: OptionGroup[];
  toggles: Toggle[];
  sliders: Slider[];
}

/** Opzioni per pagina nella griglia, come `OptionsPerPage` del legacy. */
export const OPTIONS_PER_PAGE = 6;

export const CATEGORIES: [Category, ...Category[]] = [
  {
    key: 'base',
    label: 'Base',
    hint: 'Sesso, preferenze e proporzioni di base.',
    groups: [{ kind: 'switch', label: 'Sesso', field: 'isFemale', off: 'Maschio', on: 'Femmina' }],
    toggles: [{ field: 'isFavorite', label: 'Preferito' }],
    sliders: [
      { field: 'height', label: 'Altezza', min: 0, max: 127 },
      { field: 'weight', label: 'Corporatura', min: 0, max: 127 },
      { field: 'birthMonth', label: 'Mese di creazione', min: 1, max: 12 },
      { field: 'birthDay', label: 'Giorno di creazione', min: 1, max: 31 }
    ]
  },
  {
    key: 'colors',
    label: 'Colori',
    hint: 'Il colore preferito del Mii.',
    groups: [{ kind: 'color', label: 'Colore preferito' }],
    toggles: [],
    sliders: []
  },
  {
    key: 'face',
    label: 'Viso',
    hint: 'Sfoglia le forme del viso, poi rifinisci incarnato e tratti.',
    groups: [{ kind: 'render', label: 'Forme del viso', field: 'faceShape', min: 0, max: 7 }],
    toggles: [],
    sliders: [
      { field: 'skinColor', label: 'Incarnato', min: 0, max: 5 },
      { field: 'facialFeature', label: 'Tratti', min: 0, max: 11 }
    ]
  },
  {
    key: 'hair',
    label: 'Capelli',
    hint: 'Sfoglia le acconciature; colore e specchiatura sono qui sotto.',
    groups: [{ kind: 'render', label: 'Acconciature', field: 'hairType', min: 0, max: 71 }],
    toggles: [{ field: 'hairFlipped', label: 'Specchia' }],
    sliders: [{ field: 'hairColor', label: 'Colore', min: 0, max: 7 }]
  },
  {
    key: 'eyes',
    label: 'Occhi',
    hint: 'Scegli la forma; posizione e dimensione sono qui sotto.',
    groups: [{ kind: 'render', label: 'Forme degli occhi', field: 'eyeType', min: 0, max: 47 }],
    toggles: [],
    sliders: [
      { field: 'eyeRotation', label: 'Rotazione', min: 0, max: 7 },
      { field: 'eyeColor', label: 'Colore', min: 0, max: 5 },
      { field: 'eyeSize', label: 'Dimensione', min: 0, max: 7 },
      { field: 'eyeSpacing', label: 'Distanza', min: 0, max: 12 },
      { field: 'eyeVertical', label: 'Altezza', min: 0, max: 18 }
    ]
  },
  {
    key: 'brows',
    label: 'Sopracciglia',
    hint: 'Scegli la forma, poi rotazione e distanza.',
    groups: [
      { kind: 'render', label: 'Forme delle sopracciglia', field: 'eyebrowType', min: 0, max: 23 }
    ],
    toggles: [],
    sliders: [
      { field: 'eyebrowRotation', label: 'Rotazione', min: 0, max: 11 },
      { field: 'eyebrowColor', label: 'Colore', min: 0, max: 7 },
      { field: 'eyebrowSize', label: 'Dimensione', min: 0, max: 8 },
      { field: 'eyebrowSpacing', label: 'Distanza', min: 0, max: 12 },
      { field: 'eyebrowVertical', label: 'Altezza', min: 3, max: 18 }
    ]
  },
  {
    key: 'nose',
    label: 'Naso',
    hint: 'Scegli un naso e rifinisci dimensione e altezza.',
    groups: [{ kind: 'render', label: 'Forme del naso', field: 'noseType', min: 0, max: 11 }],
    toggles: [],
    sliders: [
      { field: 'noseSize', label: 'Dimensione', min: 0, max: 8 },
      { field: 'noseVertical', label: 'Altezza', min: 0, max: 18 }
    ]
  },
  {
    key: 'mouth',
    label: 'Bocca',
    hint: 'Scegli una bocca e rifinisci colore, dimensione e altezza.',
    groups: [{ kind: 'render', label: 'Forme della bocca', field: 'mouthType', min: 0, max: 23 }],
    toggles: [],
    sliders: [
      { field: 'mouthColor', label: 'Colore', min: 0, max: 2 },
      { field: 'mouthSize', label: 'Dimensione', min: 0, max: 8 },
      { field: 'mouthVertical', label: 'Altezza', min: 0, max: 18 }
    ]
  },
  {
    key: 'beard',
    label: 'Barba',
    hint: 'Baffi e barba, ognuno con le sue anteprime.',
    groups: [
      { kind: 'render', label: 'Baffi', field: 'mustacheType', min: 0, max: 3 },
      { kind: 'render', label: 'Barba', field: 'beardType', min: 0, max: 3 }
    ],
    toggles: [],
    sliders: [
      { field: 'facialHairColor', label: 'Colore', min: 0, max: 7 },
      { field: 'mustacheSize', label: 'Dimensione dei baffi', min: 0, max: 8 },
      { field: 'mustacheVertical', label: 'Altezza dei baffi', min: 0, max: 16 }
    ]
  },
  {
    key: 'glasses',
    label: 'Occhiali',
    hint: 'Scegli un modello, poi colore e posizione.',
    groups: [{ kind: 'render', label: 'Occhiali', field: 'glassesType', min: 0, max: 8 }],
    toggles: [],
    sliders: [
      { field: 'glassesColor', label: 'Colore', min: 0, max: 5 },
      { field: 'glassesSize', label: 'Dimensione', min: 0, max: 7 },
      { field: 'glassesVertical', label: 'Altezza', min: 0, max: 20 }
    ]
  },
  {
    key: 'mole',
    label: 'Neo',
    hint: 'Attiva il neo e spostalo dove vuoi.',
    groups: [{ kind: 'switch', label: 'Neo', field: 'moleEnabled', off: 'Senza', on: 'Con neo' }],
    toggles: [],
    sliders: [
      { field: 'moleSize', label: 'Dimensione', min: 0, max: 8 },
      { field: 'moleVertical', label: 'Posizione verticale', min: 0, max: 18 },
      { field: 'moleHorizontal', label: 'Posizione orizzontale', min: 0, max: 16 }
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
