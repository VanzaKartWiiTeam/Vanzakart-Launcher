import { describe, expect, it } from 'vitest';

import { EditHistory } from './history';

describe('annulla e ripeti', () => {
  it('torna indietro e avanti di una modifica alla volta', () => {
    const history = new EditHistory({ hair: 1 });
    history.commit({ hair: 2 });
    history.commit({ hair: 3 });

    expect(history.undo()).toEqual({ hair: 2 });
    expect(history.undo()).toEqual({ hair: 1 });
    expect(history.undo()).toBeNull();
    expect(history.canUndo).toBe(false);

    expect(history.redo()).toEqual({ hair: 2 });
    expect(history.redo()).toEqual({ hair: 3 });
    expect(history.redo()).toBeNull();
  });

  it('non registra uno stato uguale al precedente', () => {
    const history = new EditHistory({ hair: 1 });
    expect(history.commit({ hair: 1 })).toBe(false);
    expect(history.canUndo).toBe(false);
  });

  it('una modifica nuova dopo un annulla cancella ciò che si poteva ripetere', () => {
    const history = new EditHistory({ hair: 1 });
    history.commit({ hair: 2 });
    history.undo();
    history.commit({ hair: 5 });

    expect(history.canRedo).toBe(false);
    expect(history.undo()).toEqual({ hair: 1 });
  });

  it('ricorda al massimo cento modifiche', () => {
    const history = new EditHistory({ step: 0 });
    for (let step = 1; step <= 150; step += 1) history.commit({ step });

    let undone = 0;
    while (history.undo()) undone += 1;
    expect(undone).toBe(100);
  });
});
