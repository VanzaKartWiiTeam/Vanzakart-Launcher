/**
 * Annulla e ripeti dell'editor Mii.
 *
 * Tiene gli stati come stringhe JSON: il Mii è un oggetto piatto di numeri e
 * booleani, e una stringa si confronta e si copia senza sorprese. Uno stato
 * entra nella pila solo quando una modifica è **finita** — il clic su un
 * tratto, il rilascio di un cursore — non a ogni passo del trascinamento:
 * cento passi di un cursore sono un annulla solo.
 */

/** Stati ricordati al massimo: oltre, i più vecchi si dimenticano. */
const LIMIT = 100;

export class EditHistory<T> {
  #past: string[] = [];
  #future: string[] = [];
  #current: string;

  constructor(initial: T) {
    this.#current = JSON.stringify(initial);
  }

  /** Registra lo stato attuale, se è cambiato dall'ultima volta. */
  commit(state: T): boolean {
    const next = JSON.stringify(state);
    if (next === this.#current) return false;
    this.#past.push(this.#current);
    if (this.#past.length > LIMIT) this.#past.shift();
    this.#current = next;
    this.#future = [];
    return true;
  }

  /** Lo stato precedente, o `null` se non c'è niente da annullare. */
  undo(): T | null {
    const previous = this.#past.pop();
    if (previous === undefined) return null;
    this.#future.push(this.#current);
    this.#current = previous;
    return JSON.parse(previous) as T;
  }

  /** Lo stato annullato per ultimo, o `null` se non c'è niente da ripetere. */
  redo(): T | null {
    const next = this.#future.pop();
    if (next === undefined) return null;
    this.#past.push(this.#current);
    this.#current = next;
    return JSON.parse(next) as T;
  }

  get canUndo(): boolean {
    return this.#past.length > 0;
  }

  get canRedo(): boolean {
    return this.#future.length > 0;
  }
}
