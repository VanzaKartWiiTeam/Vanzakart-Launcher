/**
 * Le operazioni lunghe: download e installazioni.
 *
 * Il backend ne fa una alla volta — modpack, music pack, addon, launcher —
 * e firma ogni progresso con il nome dell'operazione. Qui si tiene traccia di
 * quale sta girando e di come è finita l'ultima di ogni tipo, **fuori dalle
 * pagine**: cambiare pagina a metà di un download non deve far sparire la
 * barra, né far credere che non stia succedendo niente (§D-086).
 *
 * Chi avvia un'operazione la passa a [`OperationsStore.run`]: la promessa
 * resta viva nello store anche se il componente che l'ha avviata non c'è
 * più, e ogni pagina che la guarda la ritrova com'è.
 */

import * as api from '$lib/api';
import { app } from '$lib/stores/app.svelte';
import { t, type TranslationKey } from '$lib/stores/i18n.svelte';
import type { ApiError, ProgressEvent } from '$lib/api/types';
import { newMeter, remainingSeconds, tick, type RateMeter } from './transfer';

export const OPERATION_KINDS = [
  'mods',
  'music-pack',
  'gamebanana',
  'launcher',
  'mii-renderer'
] as const;

export type OperationKind = (typeof OPERATION_KINDS)[number];

export function isOperationKind(value: unknown): value is OperationKind {
  return typeof value === 'string' && (OPERATION_KINDS as readonly string[]).includes(value);
}

/** Nome leggibile di ogni operazione. */
const KIND_LABELS: Record<OperationKind, TranslationKey> = {
  mods: 'ops.kind.mods',
  'music-pack': 'ops.kind.musicPack',
  gamebanana: 'ops.kind.gamebanana',
  launcher: 'ops.kind.launcher',
  'mii-renderer': 'ops.kind.miiRenderer'
};

export function operationLabel(kind: OperationKind): string {
  return t(KIND_LABELS[kind]);
}

/** Fasi del backend, con il nome che hanno nella UI. */
const PHASE_LABELS: Record<string, TranslationKey> = {
  Connecting: 'phase.connecting',
  Backup: 'phase.backup',
  Download: 'phase.download',
  Verifying: 'phase.verifying',
  Installing: 'phase.installing',
  Updating: 'phase.updating',
  Recovery: 'phase.recovery',
  Rollback: 'phase.rollback',
  Completed: 'phase.completed',
  Error: 'phase.error',
  Idle: 'phase.idle'
};

export function phaseLabel(phase: string): string {
  const key = PHASE_LABELS[phase];
  return key ? t(key) : phase;
}

/**
 * Fasi in cui annullare ha senso. Dopo, il backend sta già sostituendo file
 * verificati e non si ferma a metà: offrire "Annulla" lì sarebbe una promessa
 * che non mantiene.
 */
const CANCELLABLE_PHASES = new Set(['Connecting', 'Backup', 'Download', 'Verifying']);

export function isCancellable(progress: ProgressEvent | null): boolean {
  return progress === null || CANCELLABLE_PHASES.has(progress.phase);
}

/** Come è finita l'ultima operazione di un tipo. */
export interface OperationOutcome {
  ok: boolean;
  cancelled: boolean;
  message: string;
  at: number;
}

/** Cosa mostrare mentre gira: il nome della mod, il file. */
export interface OperationMeta {
  title?: string;
  subtitle?: string;
  /** Che cosa, fra più cose dello stesso tipo: il file di una mod. */
  key?: string;
}

class OperationsStore {
  /** L'operazione in corso, o `null`. */
  active = $state<OperationKind | null>(null);
  meta = $state<OperationMeta>({});
  /** Ultimo progresso ricevuto per ogni tipo di operazione. */
  progress = $state<Partial<Record<OperationKind, ProgressEvent>>>({});
  /** Esito dell'ultima operazione di ogni tipo. */
  outcomes = $state<Partial<Record<OperationKind, OperationOutcome>>>({});
  /** Secondi che mancano alla fine del trasferimento in corso. */
  remaining = $state<number | null>(null);

  #meter: RateMeter | null = null;

  get busy(): boolean {
    return this.active !== null;
  }

  busyWith(kind: OperationKind): boolean {
    return this.active === kind;
  }

  /** `true` quando è in corso un'operazione **diversa** da `kind`. */
  blockedBy(kind: OperationKind): boolean {
    return this.active !== null && this.active !== kind;
  }

  progressOf(kind: OperationKind): ProgressEvent | null {
    return this.progress[kind] ?? null;
  }

  outcomeOf(kind: OperationKind): OperationOutcome | null {
    return this.outcomes[kind] ?? null;
  }

  clearOutcome(kind: OperationKind): void {
    const next = { ...this.outcomes };
    delete next[kind];
    this.outcomes = next;
  }

  /** Percentuale dell'operazione in corso, per gli indicatori compatti. */
  get percent(): number | null {
    if (this.active === null) return null;
    return this.progress[this.active]?.percent ?? null;
  }

  /**
   * Ascolta i progressi del backend. Si chiama una volta, dalla shell.
   *
   * Chiede anche se c'è già un'operazione in corso: succede quando la webview
   * si ricarica a metà di un download, e senza la pagina crederebbe di poter
   * partire con un'altra.
   */
  async listen(): Promise<() => void> {
    const unlisten = await api.onProgress((event) => this.receive(event));

    try {
      const current = await api.getCurrentOperation();
      if (this.active === null && isOperationKind(current)) this.active = current;
    } catch {
      // Un backend che non risponde qui risponderà al primo comando.
    }

    return unlisten;
  }

  /** Un progresso dal backend. Pubblica solo per i test. */
  receive(event: ProgressEvent): void {
    // Le pagine che guardano ancora il progresso "globale" lo trovano lì.
    app.progress = event;
    if (event.detail) {
      app.setStatusLine(event.detail, event.phase === 'Error' ? 'danger' : 'info');
    }

    if (!isOperationKind(event.operation)) return;
    const kind = event.operation;
    this.progress = { ...this.progress, [kind]: event };

    if (kind === this.active && event.bytesTotal > 0) {
      const now = Date.now();
      this.#meter =
        this.#meter === null
          ? newMeter(now, event.bytesDone)
          : tick(this.#meter, now, event.bytesDone);
      this.remaining = remainingSeconds(this.#meter, event.bytesTotal);
    } else if (kind === this.active) {
      this.remaining = null;
    }
  }

  /**
   * Esegue un'operazione lunga.
   *
   * Una sola alla volta, come nel backend: se ce n'è già una si rifiuta
   * subito con un messaggio che dice quale, invece di lasciare che il
   * backend risponda con un "occupato" che non spiega niente.
   */
  async run<T>(
    kind: OperationKind,
    task: () => Promise<T>,
    options: OperationMeta & { describe?: (value: T) => string } = {}
  ): Promise<T> {
    if (this.active !== null) {
      const busy: ApiError = {
        code: 'busy',
        message: t('ops.busy', { operation: operationLabel(this.active) })
      };
      throw busy;
    }

    this.active = kind;
    this.meta = { title: options.title, subtitle: options.subtitle, key: options.key };
    this.remaining = null;
    this.#meter = null;
    const progress = { ...this.progress };
    delete progress[kind];
    this.progress = progress;
    this.clearOutcome(kind);

    try {
      const value = await task();
      this.outcomes = {
        ...this.outcomes,
        [kind]: {
          ok: true,
          cancelled: false,
          message: options.describe?.(value) ?? '',
          at: Date.now()
        }
      };
      return value;
    } catch (error) {
      this.outcomes = {
        ...this.outcomes,
        [kind]: {
          ok: false,
          cancelled: api.errorCode(error) === 'cancelled',
          message: api.errorMessage(error),
          at: Date.now()
        }
      };
      throw error;
    } finally {
      if (this.active === kind) this.active = null;
      this.meta = {};
      this.remaining = null;
      this.#meter = null;
    }
  }

  /** Chiede al backend di fermare l'operazione in corso. */
  async cancel(): Promise<void> {
    await api.cancelOperation();
  }
}

export const operations = new OperationsStore();
