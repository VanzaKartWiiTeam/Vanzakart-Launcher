import { describe, expect, it } from 'vitest';

import { en } from './en';
import { it as italian } from './it';
import { i18n, LOCALES, t } from './store.svelte';

/** Segnaposto `{nome}` presenti in una stringa. */
function placeholders(text: string): string[] {
  return [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]!).sort();
}

const keys = Object.keys(italian) as (keyof typeof italian)[];

describe("dizionari dell'installer", () => {
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
   * Come nel launcher: una traduzione identica all'originale di solito è una
   * dimenticanza. Restano passabili le frasi corte — sigle, nomi propri — e
   * quelle senza lettere.
   */
  it('traduce davvero, tranne dove non c’è niente da tradurre', () => {
    const identical = keys.filter((key) => en[key] === italian[key]);

    for (const key of identical) {
      const value = italian[key];
      const short = value.length <= 24;
      const noLetters = !/\p{Letter}/u.test(value);
      expect(short || noLetters, `${String(key)} = ${value}`).toBe(true);
    }
  });
});

describe("t() dell'installer", () => {
  it('sostituisce i segnaposto', () => {
    i18n.set('it');
    expect(t('welcome.existing.version', { version: '2.0.0' })).toBe('Versione 2.0.0');

    i18n.set('en');
    expect(t('done.title', { version: '2.0.0' })).toBe('VanzaKart Launcher 2.0.0 is installed');
    i18n.set('it');
  });

  it('cambia lingua per tutte le chiavi', () => {
    for (const locale of LOCALES) {
      i18n.set(locale);
      expect(t('uninstall.run')).toBe(locale === 'it' ? 'Disinstalla' : 'Uninstall');
    }
    i18n.set('it');
  });
});
