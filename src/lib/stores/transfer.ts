/**
 * Conti sui trasferimenti, senza stato Svelte: si provano da soli.
 *
 * Il backend manda byte fatti e byte totali ogni 100 ms; da lì si ricava la
 * velocità e quindi il tempo che manca. La velocità si liscia con una media
 * esponenziale, come fa il backend per la sua etichetta: un valore preso fra
 * due soli campioni salta troppo per essere letto, e un "mancano 2 minuti"
 * che diventa "mancano 9 secondi" e poi di nuovo "2 minuti" è peggio che non
 * dire niente.
 */

/** Peso del campione nuovo nella media. */
const SMOOTHING = 0.25;

/** Sotto questa finestra la misura è rumore. */
const MIN_WINDOW_MS = 250;

export interface RateMeter {
  at: number;
  bytes: number;
  /** Byte al secondo, lisciati. `null` finché non si è misurato niente. */
  rate: number | null;
}

export function newMeter(at: number, bytes = 0): RateMeter {
  return { at, bytes, rate: null };
}

/**
 * Aggiorna la misura con un nuovo campione.
 *
 * Quando i byte tornano indietro è cominciato un file nuovo (il download
 * differenziale conta file per file): la misura riparte invece di produrre
 * una velocità negativa.
 */
export function tick(meter: RateMeter, at: number, bytes: number): RateMeter {
  if (bytes < meter.bytes) return newMeter(at, bytes);

  const elapsed = at - meter.at;
  if (elapsed < MIN_WINDOW_MS) return meter;

  const sample = ((bytes - meter.bytes) / elapsed) * 1000;
  const rate = meter.rate === null ? sample : meter.rate + SMOOTHING * (sample - meter.rate);
  return { at, bytes, rate };
}

/** Secondi che mancano, o `null` quando non si può dire. */
export function remainingSeconds(meter: RateMeter, total: number): number | null {
  if (meter.rate === null || meter.rate < 1 || total <= 0 || meter.bytes >= total) return null;
  return Math.ceil((total - meter.bytes) / meter.rate);
}

/**
 * Tempo che manca in forma breve e senza lingua: `45 s`, `3 min`, `1 h 05`.
 *
 * I numeri e le unità si leggono uguali in italiano e in inglese; la frase
 * attorno ("mancano…") la mette chi la mostra.
 */
export function formatRemaining(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) return '';
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))} s`;

  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min`;

  const hours = Math.floor(minutes / 60);
  return `${hours} h ${String(minutes % 60).padStart(2, '0')}`;
}
