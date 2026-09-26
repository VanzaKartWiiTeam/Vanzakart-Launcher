import { describe, expect, it } from 'vitest';

import { formatRemaining, newMeter, remainingSeconds, tick } from './transfer';

describe('velocità e tempo rimasto', () => {
  it('misura i byte al secondo fra due campioni', () => {
    const meter = tick(newMeter(0, 0), 1000, 2_000_000);
    expect(meter.rate).toBe(2_000_000);
    expect(remainingSeconds(meter, 10_000_000)).toBe(4);
  });

  it('ignora i campioni troppo ravvicinati', () => {
    const first = tick(newMeter(0, 0), 1000, 1_000_000);
    expect(tick(first, 1100, 5_000_000)).toBe(first);
  });

  it('liscia la velocità invece di saltare al valore nuovo', () => {
    const slow = tick(newMeter(0, 0), 1000, 1_000_000);
    const faster = tick(slow, 2000, 4_000_000);
    expect(faster.rate).toBeGreaterThan(1_000_000);
    expect(faster.rate).toBeLessThan(3_000_000);
  });

  it('riparte quando i byte tornano indietro, a ogni file nuovo', () => {
    const meter = tick(newMeter(0, 0), 1000, 5_000_000);
    const restarted = tick(meter, 2000, 100);
    expect(restarted.rate).toBeNull();
    expect(restarted.bytes).toBe(100);
  });

  it('non inventa un tempo quando non lo sa', () => {
    expect(remainingSeconds(newMeter(0, 0), 100)).toBeNull();
    const meter = tick(newMeter(0, 0), 1000, 1000);
    expect(remainingSeconds(meter, 0)).toBeNull();
    expect(remainingSeconds(meter, 1000)).toBeNull();
  });

  it('scrive il tempo in forma breve', () => {
    expect(formatRemaining(null)).toBe('');
    expect(formatRemaining(0.4)).toBe('1 s');
    expect(formatRemaining(45)).toBe('45 s');
    expect(formatRemaining(150)).toBe('3 min');
    expect(formatRemaining(3900)).toBe('1 h 05');
  });
});
