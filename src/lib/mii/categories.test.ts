import { describe, expect, it } from 'vitest';

import {
  CATEGORIES,
  FACIAL_FEATURES,
  NAME_SYMBOLS,
  clampState,
  daysInMonth,
  rangeOf,
  sliderPosition,
  sliderValue,
  toLimits
} from './categories';
import icons from './icons.json';
import { t } from '$lib/stores/i18n.svelte';
import type {
  MiiBooleanField,
  MiiEditorState,
  MiiFieldLimit,
  MiiNumericField
} from '$lib/api/types';

/**
 * I limiti come li manda il backend (`vk_save::mii::LIMITS`). Qui servono
 * solo come dato di prova: l'editor li chiede al backend e non ne tiene una
 * copia sua.
 */
const LIMITS: MiiFieldLimit[] = [
  { field: 'favoriteColorIndex', min: 0, max: 11 },
  { field: 'birthMonth', min: 1, max: 12 },
  { field: 'birthDay', min: 1, max: 31 },
  { field: 'height', min: 0, max: 127 },
  { field: 'weight', min: 0, max: 127 },
  { field: 'faceShape', min: 0, max: 7 },
  { field: 'skinColor', min: 0, max: 5 },
  { field: 'facialFeature', min: 0, max: 11 },
  { field: 'hairType', min: 0, max: 71 },
  { field: 'hairColor', min: 0, max: 7 },
  { field: 'eyebrowType', min: 0, max: 23 },
  { field: 'eyebrowRotation', min: 0, max: 11 },
  { field: 'eyebrowColor', min: 0, max: 7 },
  { field: 'eyebrowSize', min: 0, max: 8 },
  { field: 'eyebrowVertical', min: 3, max: 18 },
  { field: 'eyebrowSpacing', min: 0, max: 12 },
  { field: 'eyeType', min: 0, max: 47 },
  { field: 'eyeRotation', min: 0, max: 7 },
  { field: 'eyeVertical', min: 0, max: 18 },
  { field: 'eyeColor', min: 0, max: 5 },
  { field: 'eyeSize', min: 0, max: 7 },
  { field: 'eyeSpacing', min: 0, max: 12 },
  { field: 'noseType', min: 0, max: 11 },
  { field: 'noseSize', min: 0, max: 8 },
  { field: 'noseVertical', min: 0, max: 18 },
  { field: 'mouthType', min: 0, max: 23 },
  { field: 'mouthColor', min: 0, max: 2 },
  { field: 'mouthSize', min: 0, max: 8 },
  { field: 'mouthVertical', min: 0, max: 18 },
  { field: 'glassesType', min: 0, max: 8 },
  { field: 'glassesColor', min: 0, max: 5 },
  { field: 'glassesSize', min: 0, max: 7 },
  { field: 'glassesVertical', min: 0, max: 20 },
  { field: 'mustacheType', min: 0, max: 3 },
  { field: 'beardType', min: 0, max: 3 },
  { field: 'facialHairColor', min: 0, max: 7 },
  { field: 'mustacheSize', min: 0, max: 8 },
  { field: 'mustacheVertical', min: 0, max: 16 },
  { field: 'moleSize', min: 0, max: 8 },
  { field: 'moleVertical', min: 0, max: 30 },
  { field: 'moleHorizontal', min: 0, max: 16 }
];

const limits = toLimits(LIMITS);

/** Lo stato predefinito di `vk_save::mii::MiiEditorState`. */
const DEFAULT_STATE: MiiEditorState = {
  name: 'Vanza Mii',
  creatorName: 'VanzaKart',
  isFemale: false,
  isFavorite: true,
  favoriteColorIndex: 4,
  birthMonth: 1,
  birthDay: 1,
  height: 64,
  weight: 64,
  miiId: 0,
  systemId: [0, 0, 0, 0],
  faceShape: 0,
  skinColor: 1,
  facialFeature: 0,
  hairType: 33,
  hairColor: 0,
  hairFlipped: false,
  eyebrowType: 6,
  eyebrowRotation: 6,
  eyebrowColor: 0,
  eyebrowSize: 4,
  eyebrowVertical: 10,
  eyebrowSpacing: 2,
  eyeType: 2,
  eyeRotation: 4,
  eyeVertical: 12,
  eyeColor: 0,
  eyeSize: 4,
  eyeSpacing: 2,
  noseType: 1,
  noseSize: 4,
  noseVertical: 9,
  mouthType: 23,
  mouthColor: 0,
  mouthSize: 4,
  mouthVertical: 13,
  glassesType: 0,
  glassesColor: 0,
  glassesSize: 4,
  glassesVertical: 10,
  mustacheType: 0,
  beardType: 0,
  facialHairColor: 0,
  mustacheSize: 4,
  mustacheVertical: 10,
  moleEnabled: false,
  moleSize: 4,
  moleVertical: 10,
  moleHorizontal: 10
};

const controls = CATEGORIES.flatMap((category) => category.controls);

describe('categorie dell’editor Mii', () => {
  it('mette ogni campo numerico in un controllo, una volta sola', () => {
    const numeric = controls.flatMap((control) =>
      control.kind === 'switch' || control.kind === 'toggle' ? [] : [control.field]
    );
    expect(new Set(numeric).size).toBe(numeric.length);

    // Fuori: `miiId`, l'identità, che assegna il backend.
    const expected = LIMITS.map((limit) => limit.field);
    expect(numeric.slice().sort()).toEqual(expected.slice().sort());
  });

  it('mette ogni interruttore in un controllo, una volta sola', () => {
    const expected: MiiBooleanField[] = ['isFemale', 'isFavorite', 'hairFlipped', 'moleEnabled'];
    const covered = controls.flatMap((control) =>
      control.kind === 'switch' || control.kind === 'toggle' ? [control.field] : []
    );
    expect(covered.slice().sort()).toEqual(expected.slice().sort());
  });

  it('dà a ogni categoria un’etichetta tradotta e almeno un controllo', () => {
    for (const category of CATEGORIES) {
      // `t` ricade sulla chiave quando manca: un'etichetta uguale alla sua
      // chiave è una traduzione dimenticata.
      expect(t(category.label)).not.toBe(category.label);
      expect(t(category.hint)).not.toBe(category.hint);
      expect(category.controls.length).toBeGreaterThan(0);
      for (const control of category.controls) {
        expect(t(control.label)).not.toBe(control.label);
      }
    }
  });

  it('tiene le categorie e l’ordine del launcher legacy', () => {
    // Da `MiiEditorWindow.BuildCategories`. L'ordine è quello che l'utente
    // conosce: cambiarlo sposta i pulsanti sotto le sue dita.
    expect(CATEGORIES.map((category) => category.key)).toEqual([
      'base',
      'colors',
      'face',
      'hair',
      'eyes',
      'brows',
      'nose',
      'mouth',
      'beard',
      'glasses',
      'mole'
    ]);
  });

  it('ha un’icona per ogni valore che una griglia può proporre', () => {
    for (const control of controls) {
      if (control.kind !== 'parts') continue;
      const range = rangeOf(control.field, DEFAULT_STATE, limits);
      const set = icons[control.icon];
      expect(set.length, control.icon).toBeGreaterThan(range.max);

      for (const icon of set.slice(range.min, range.max + 1)) {
        expect(icon.layers.length, control.icon).toBeGreaterThan(0);
        expect(icon.box[2], control.icon).toBeGreaterThan(0);
        expect(icon.box[3], control.icon).toBeGreaterThan(0);
      }
    }
  });

  it('dà un nome a ogni tratto del viso', () => {
    const range = rangeOf('facialFeature', DEFAULT_STATE, limits);
    expect(FACIAL_FEATURES.length).toBe(range.max - range.min + 1);
    for (const key of FACIAL_FEATURES) expect(t(key)).not.toBe(key);
  });

  it('offre solo simboli distinti da inserire nel nome', () => {
    expect(NAME_SYMBOLS.length).toBeGreaterThan(0);
    expect(new Set(NAME_SYMBOLS).size).toBe(NAME_SYMBOLS.length);
    // Il nome sta in 10 unità UTF-16: un simbolo che ne occupasse due
    // renderebbe il conteggio dei caratteri bugiardo.
    for (const symbol of NAME_SYMBOLS) {
      expect(symbol.length, symbol).toBe(1);
    }
  });
});

describe('limiti dell’editor', () => {
  it('riporta ogni campo dentro il suo intervallo', () => {
    const wild: MiiEditorState = { ...DEFAULT_STATE };
    const fields: MiiNumericField[] = LIMITS.map((limit) => limit.field);

    for (const field of fields) wild[field] = 250;
    const high = clampState(wild, limits);

    for (const field of fields) wild[field] = -5;
    const low = clampState(wild, limits);

    // Il mese arriva a dicembre prima del giorno: il 31 resta possibile.
    for (const { field, min, max } of LIMITS) {
      expect(low[field], field).toBe(min);
      expect(high[field], field).toBe(max);
    }
  });

  it('non lascia un giorno che il mese non ha', () => {
    const state = clampState({ ...DEFAULT_STATE, birthMonth: 2, birthDay: 31 }, limits);
    expect(state.birthDay).toBe(29);
    expect(rangeOf('birthDay', { ...DEFAULT_STATE, birthMonth: 4 }, limits).max).toBe(30);
    expect(daysInMonth(12)).toBe(31);
  });

  it('arrotonda i valori non interi', () => {
    expect(clampState({ ...DEFAULT_STATE, eyeSize: 3.6 }, limits).eyeSize).toBe(4);
  });

  it('inverte i cursori dell’altezza in entrambe le direzioni', () => {
    const range = { min: 3, max: 18 };
    for (let value = range.min; value <= range.max; value += 1) {
      const position = sliderPosition(value, range, true);
      expect(position).toBeGreaterThanOrEqual(range.min);
      expect(position).toBeLessThanOrEqual(range.max);
      expect(sliderValue(position, range, true)).toBe(value);
    }
    // Più in alto sul viso significa un valore più basso nel formato.
    expect(sliderPosition(range.min, range, true)).toBe(range.max);
    expect(sliderValue(99, range)).toBe(range.max);
  });
});
