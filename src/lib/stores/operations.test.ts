import { describe, expect, it } from 'vitest';

import type { ProgressEvent } from '$lib/api/types';
import { i18n } from '$lib/stores/i18n.svelte';
import { isCancellable, isOperationKind, operations, phaseLabel } from './operations.svelte';

function event(
  operation: string,
  phase: string,
  extra: Partial<ProgressEvent> = {}
): ProgressEvent {
  return {
    operation,
    phase,
    detail: '',
    percent: null,
    bytesDone: 0,
    bytesTotal: 0,
    filesDone: 0,
    filesTotal: 0,
    bytesLabel: '',
    speedLabel: '',
    ...extra
  };
}

describe('operazioni lunghe', () => {
  it('tengono il nome finché girano e poi ricordano come è andata', async () => {
    let seenWhileRunning: string | null = null;

    const value = await operations.run(
      'music-pack',
      async () => {
        seenWhileRunning = operations.active;
        return { summary: 'Music pack installato.' };
      },
      { describe: (result) => result.summary }
    );

    expect(value.summary).toBe('Music pack installato.');
    expect(seenWhileRunning).toBe('music-pack');
    expect(operations.active).toBeNull();
    expect(operations.outcomeOf('music-pack')).toMatchObject({
      ok: true,
      message: 'Music pack installato.'
    });
  });

  it('una alla volta: la seconda si rifiuta dicendo quale sta girando', async () => {
    i18n.set('it');
    let release: () => void = () => {};
    const first = operations.run('mods', () => new Promise<void>((resolve) => (release = resolve)));

    await expect(operations.run('gamebanana', async () => 1)).rejects.toMatchObject({
      code: 'busy',
      message: expect.stringContaining('Modpack')
    });
    expect(operations.busyWith('mods')).toBe(true);
    expect(operations.blockedBy('gamebanana')).toBe(true);
    expect(operations.blockedBy('mods')).toBe(false);

    release();
    await first;
    expect(operations.busy).toBe(false);
  });

  it('un errore finisce nell’esito, e un annullamento si riconosce', async () => {
    await expect(
      operations.run('mods', async () => {
        throw { code: 'network', message: 'Server irraggiungibile' };
      })
    ).rejects.toMatchObject({ code: 'network' });
    expect(operations.outcomeOf('mods')).toMatchObject({
      ok: false,
      cancelled: false,
      message: 'Server irraggiungibile'
    });

    await expect(
      operations.run('mods', async () => {
        throw { code: 'cancelled', message: 'operation cancelled' };
      })
    ).rejects.toBeTruthy();
    expect(operations.outcomeOf('mods')?.cancelled).toBe(true);

    operations.clearOutcome('mods');
    expect(operations.outcomeOf('mods')).toBeNull();
  });

  it('ogni progresso va all’operazione che lo firma', async () => {
    let release: () => void = () => {};
    const running = operations.run(
      'music-pack',
      () => new Promise<void>((resolve) => (release = resolve))
    );

    operations.receive(
      event('music-pack', 'Download', { percent: 42, bytesDone: 42, bytesTotal: 100 })
    );
    operations.receive(event('qualcos-altro', 'Download', { percent: 99 }));

    expect(operations.progressOf('music-pack')?.percent).toBe(42);
    expect(operations.percent).toBe(42);
    expect(operations.progressOf('mods')).toBeNull();

    release();
    await running;
    expect(operations.percent).toBeNull();
  });

  it('riconosce solo i tipi che conosce', () => {
    expect(isOperationKind('mods')).toBe(true);
    expect(isOperationKind('music-pack')).toBe(true);
    expect(isOperationKind('rm -rf')).toBe(false);
    expect(isOperationKind(null)).toBe(false);
  });

  it('offre "Annulla" solo finché annullare ferma davvero qualcosa', () => {
    expect(isCancellable(null)).toBe(true);
    expect(isCancellable(event('mods', 'Download'))).toBe(true);
    expect(isCancellable(event('mods', 'Verifying'))).toBe(true);
    expect(isCancellable(event('mods', 'Installing'))).toBe(false);
    expect(isCancellable(event('mods', 'Completed'))).toBe(false);
  });

  it('traduce le fasi del backend', () => {
    i18n.set('it');
    expect(phaseLabel('Verifying')).toBe('Verifica');
    i18n.set('en');
    expect(phaseLabel('Verifying')).toBe('Verifying');
    expect(phaseLabel('SconosciutaDelFuturo')).toBe('SconosciutaDelFuturo');
    i18n.set('it');
  });
});
