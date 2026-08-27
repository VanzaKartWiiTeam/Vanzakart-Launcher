import { describe, expect, it } from 'vitest';

import { formatBytes, formatDate, formatPlayTime, PAGE_META, ROUTES } from './app.svelte';

describe('formatPlayTime', () => {
  it('mostra i minuti sotto l’ora', () => {
    expect(formatPlayTime(0)).toBe('0 min');
    expect(formatPlayTime(1)).toBe('1 min');
    expect(formatPlayTime(59.4)).toBe('59 min');
  });

  it('passa a ore e minuti oltre i 60 minuti', () => {
    expect(formatPlayTime(60)).toBe('1 h');
    expect(formatPlayTime(90)).toBe('1 h 30 min');
    expect(formatPlayTime(125)).toBe('2 h 5 min');
  });

  it('gestisce valori non validi', () => {
    expect(formatPlayTime(-10)).toBe('0 min');
    expect(formatPlayTime(Number.NaN)).toBe('0 min');
    expect(formatPlayTime(Number.POSITIVE_INFINITY)).toBe('0 min');
  });
});

describe('formatDate', () => {
  it('formatta una data ISO', () => {
    const formatted = formatDate('2026-08-25T10:30:00Z');
    expect(formatted).toContain('2026');
    expect(formatted).not.toBe('Mai');
  });

  it('restituisce "Mai" per valori assenti o non validi', () => {
    expect(formatDate(null)).toBe('Mai');
    expect(formatDate('')).toBe('Mai');
    expect(formatDate('non una data')).toBe('Mai');
  });
});

describe('formatBytes', () => {
  it('usa le stesse unità del backend', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatBytes(3 * 1024 ** 3)).toBe('3.0 GB');
  });

  it('gestisce valori non validi', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(-1)).toBe('0 B');
    expect(formatBytes(Number.NaN)).toBe('0 B');
  });
});

describe('routing', () => {
  it('mantiene l’ordine della sidebar legacy', () => {
    expect(ROUTES).toEqual([
      'home',
      'news',
      'rooms',
      'leaderboard',
      'friends',
      'mods',
      'licenses',
      'settings',
      'debug'
    ]);
  });

  it('ha metadati per ogni pagina', () => {
    for (const route of ROUTES) {
      const meta = PAGE_META[route];
      expect(meta.title.length).toBeGreaterThan(0);
      expect(meta.subtitle.length).toBeGreaterThan(0);
      expect(meta.icon.length).toBeGreaterThan(0);
    }
  });
});
