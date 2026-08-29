import { describe, expect, it } from 'vitest';

import { en } from './en';
import { it as italian } from './it';
import { i18n, LOCALES, t } from '$lib/stores/i18n.svelte';

/** Segnaposto `{nome}` presenti in una stringa. */
function placeholders(text: string): string[] {
  return [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]!).sort();
}

const keys = Object.keys(italian) as (keyof typeof italian)[];

describe('dizionari', () => {
  it('hanno esattamente le stesse chiavi', () => {
    expect(Object.keys(en).slice().sort()).toEqual(Object.keys(italian).slice().sort());
  });

  it('non lasciano stringhe vuote', () => {
    for (const key of keys) {
      expect(italian[key].trim(), key).not.toBe('');
      expect(en[key].trim(), key).not.toBe('');
    }
  });

  it('usano gli stessi segnaposto in tutte le lingue', () => {
    for (const key of keys) {
      expect(placeholders(en[key]), key).toEqual(placeholders(italian[key]));
    }
  });

  /*
   * Una traduzione identica all'originale di solito è una dimenticanza. Le
   * eccezioni vere — sigle, nomi propri, parole che si scrivono uguale — si
   * elencano qui, così aggiungerne una è una scelta e non una svista.
   */
  it('traduce davvero, tranne dove non c’è niente da tradurre', () => {
    /** Frasi lunghe fatte di parole che le due lingue scrivono uguale. */
    const same = new Set<string>(['team.versions']);

    const identical = keys.filter((key) => en[key] === italian[key] && !same.has(key));

    for (const key of identical) {
      const value = italian[key];
      const short = value.length <= 24;
      const noLetters = !/\p{Letter}/u.test(value);
      expect(short || noLetters, `${String(key)} = ${value}`).toBe(true);
    }
  });
});

describe('t()', () => {
  it('sostituisce i segnaposto', () => {
    i18n.set('it');
    expect(t('friends.slot', { number: 3 })).toBe('Slot 3');

    i18n.set('en');
    expect(t('board.playerCount', { count: 12 })).toBe('12 players');
    i18n.set('it');
  });

  it('cambia lingua per tutte le chiavi', () => {
    for (const locale of LOCALES) {
      i18n.set(locale);
      expect(t('common.close')).toBe(locale === 'it' ? 'Chiudi' : 'Close');
    }
    i18n.set('it');
  });
});
