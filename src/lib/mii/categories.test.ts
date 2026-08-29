import { describe, expect, it } from 'vitest';

import { CATEGORIES, NAME_SYMBOLS, OPTIONS_PER_PAGE } from './categories';
import { t } from '$lib/stores/i18n.svelte';
import type { MiiBooleanField, MiiNumericField } from '$lib/api/types';

/**
 * Limiti assoluti del formato, da `vk_save::mii::MiiEditorState::normalized`.
 * Un controllo che li superasse produrrebbe un valore che il backend riporta
 * silenziosamente dentro l'intervallo: l'utente sceglierebbe un'opzione senza
 * vedere alcun effetto.
 */
const LIMITS: Record<MiiNumericField, number> = {
  favoriteColorIndex: 11,
  birthMonth: 12,
  birthDay: 31,
  height: 127,
  weight: 127,
  miiId: Number.MAX_SAFE_INTEGER,
  faceShape: 7,
  skinColor: 5,
  facialFeature: 11,
  hairType: 71,
  hairColor: 7,
  eyebrowType: 23,
  eyebrowRotation: 15,
  eyebrowColor: 7,
  eyebrowSize: 15,
  eyebrowVertical: 31,
  eyebrowSpacing: 15,
  eyeType: 47,
  eyeRotation: 7,
  eyeVertical: 31,
  eyeColor: 5,
  eyeSize: 7,
  eyeSpacing: 15,
  noseType: 11,
  noseSize: 15,
  noseVertical: 31,
  mouthType: 23,
  mouthColor: 2,
  mouthSize: 15,
  mouthVertical: 31,
  glassesType: 8,
  glassesColor: 5,
  glassesSize: 7,
  glassesVertical: 31,
  mustacheType: 3,
  beardType: 3,
  facialHairColor: 7,
  mustacheSize: 15,
  mustacheVertical: 31,
  moleSize: 15,
  moleVertical: 31,
  moleHorizontal: 31
};

const sliders = CATEGORIES.flatMap((category) => category.sliders);
const toggles = CATEGORIES.flatMap((category) => category.toggles);
const groups = CATEGORIES.flatMap((category) => category.groups);

/** Griglie di miniature: un tratto scelto sfogliando, non con un cursore. */
const renderGroups = groups.flatMap((group) => (group.kind === 'render' ? [group] : []));
const switchGroups = groups.flatMap((group) => (group.kind === 'switch' ? [group] : []));

describe('categorie dell’editor Mii', () => {
  it('non ripete lo stesso campo in due posti', () => {
    const numeric = [...sliders.map((slider) => slider.field), ...renderGroups.map((g) => g.field)];
    expect(new Set(numeric).size).toBe(numeric.length);

    const boolean = [...toggles.map((toggle) => toggle.field), ...switchGroups.map((g) => g.field)];
    expect(new Set(boolean).size).toBe(boolean.length);
  });

  it('copre ogni campo numerico dello stato', () => {
    // Fuori dal conteggio: `miiId` (identità, la assegna il backend) e
    // `favoriteColorIndex`, che ha la sua tavolozza di pastiglie colorate.
    const outside = ['miiId', 'favoriteColorIndex'];
    const expected = Object.keys(LIMITS).filter(
      (field) => !outside.includes(field)
    ) as MiiNumericField[];
    const covered = [
      ...sliders.map((slider) => slider.field),
      ...renderGroups.map((group) => group.field)
    ];

    expect(covered.slice().sort()).toEqual(expected.slice().sort());
  });

  it('copre ogni interruttore dello stato', () => {
    const expected: MiiBooleanField[] = ['isFemale', 'isFavorite', 'hairFlipped', 'moleEnabled'];
    const covered = [
      ...toggles.map((toggle) => toggle.field),
      ...switchGroups.map((group) => group.field)
    ];

    expect(covered.slice().sort()).toEqual(expected.slice().sort());
  });

  it('non propone valori che il backend riporterebbe dentro i limiti', () => {
    for (const slider of sliders) {
      expect(slider.min, slider.field).toBeGreaterThanOrEqual(0);
      expect(slider.max, slider.field).toBeGreaterThan(slider.min);
      expect(slider.max, slider.field).toBeLessThanOrEqual(LIMITS[slider.field]);
    }

    for (const group of renderGroups) {
      expect(group.min, group.field).toBeGreaterThanOrEqual(0);
      expect(group.max, group.field).toBeGreaterThan(group.min);
      expect(group.max, group.field).toBeLessThanOrEqual(LIMITS[group.field]);
    }
  });

  it('dà a ogni categoria un’etichetta tradotta e almeno un controllo', () => {
    for (const category of CATEGORIES) {
      // `t` ricade sulla chiave quando manca: un'etichetta uguale alla sua
      // chiave è una traduzione dimenticata.
      expect(t(category.label)).not.toBe(category.label);
      expect(t(category.hint)).not.toBe(category.hint);
      expect(
        category.groups.length + category.sliders.length + category.toggles.length
      ).toBeGreaterThan(0);
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

  it('impagina come il legacy', () => {
    expect(OPTIONS_PER_PAGE).toBe(6);
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
