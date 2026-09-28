/**
 * Coda dei render Mii.
 *
 * Ogni faccia mostrata dal launcher — l'anteprima dell'editor, le miniature
 * delle opzioni, gli avatar di licenze, amici e profili — passa da qui.
 * Due ragioni, entrambe del launcher legacy:
 *
 * - **un tetto ai render simultanei**, come il `FeaturePreviewRenderGate` del
 *   WPF: con il renderer nativo ogni render occupa un core, senza il servizio
 *   di Mii Studio una raffica di richieste le fa fallire tutte insieme;
 * - **una richiesta sola per immagine**, come `InFlightRenders`: la stessa
 *   faccia compare in più punti della stessa pagina, e chiederla una volta per
 *   punto sarebbe spreco puro.
 *
 * Questa è la cache di sessione, che evita perfino il giro sull'IPC. Il
 * backend non ne tiene una per i render nativi: rifarli costa meno che
 * leggerli dal disco (§D-092).
 */
import * as api from '$lib/api';
import type { MiiEditorState } from '$lib/api/types';
import type { MiiRenderKind } from '$lib/api';

/** Render simultanei ammessi. */
const MAX_CONCURRENT = 4;

/**
 * Lati in cui si renderizza. Un avatar da 44 px non ha bisogno di 512 px di
 * Mii, e un lato preciso per ogni avatar farebbe esplodere la cache.
 */
const SIZES = [128, 256, 384, 512] as const;
const LARGEST = 512;

/** Il lato da chiedere per mostrare un'immagine larga `cssPixels`. */
export function renderSize(cssPixels: number): number {
  const ratio = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const needed = cssPixels * ratio;
  return SIZES.find((size) => size >= needed) ?? LARGEST;
}

/** `null` significa "provato e non riuscito": non si ritenta a ogni render. */
const cache = new Map<string, string | null>();
const inFlight = new Map<string, Promise<string | null>>();

let running = 0;
const waiting: (() => void)[] = [];

async function withSlot<T>(run: () => Promise<T>): Promise<T> {
  if (running >= MAX_CONCURRENT) {
    await new Promise<void>((resolve) => waiting.push(resolve));
  }
  running += 1;
  try {
    return await run();
  } finally {
    running -= 1;
    waiting.shift()?.();
  }
}

function enqueue(key: string, run: () => Promise<string | null>): Promise<string | null> {
  const cached = cache.get(key);
  if (cached !== undefined) return Promise.resolve(cached);

  const pending = inFlight.get(key);
  if (pending) return pending;

  const task = withSlot(run)
    .then((image) => {
      cache.set(key, image);
      return image;
    })
    .catch(() => {
      // Un render mancato non è un errore: resta la silhouette. Ricordarlo
      // evita di ripetere la stessa richiesta a ogni ridisegno.
      cache.set(key, null);
      return null;
    })
    .finally(() => {
      inFlight.delete(key);
    });

  inFlight.set(key, task);
  return task;
}

/** Render di una `studioData` già nota: licenza, amico o profilo. */
export function renderStudio(
  studioData: string,
  kind: MiiRenderKind = 'face',
  rotation = 0,
  size = 512
): Promise<string | null> {
  if (!studioData.trim()) return Promise.resolve(null);

  return enqueue(`s:${kind}:${rotation}:${size}:${studioData}`, () =>
    api.renderMiiStudio(studioData, kind, rotation, size)
  );
}

/**
 * Render di uno stato dell'editor.
 *
 * La chiave è lo stato senza i campi che non cambiano la faccia — nome,
 * creatore, identificativi — così due Mii identici nell'aspetto condividono
 * un render solo.
 */
export function renderState(
  state: MiiEditorState,
  kind: MiiRenderKind = 'face',
  rotation = 0,
  size = 512
): Promise<string | null> {
  return enqueue(`e:${kind}:${rotation}:${size}:${appearanceKey(state)}`, () =>
    api.renderMiiState(state, kind, rotation, size)
  );
}

/**
 * Campi che stanno nei 74 byte ma che il renderer non disegna: due Mii che
 * differiscono solo per questi hanno la stessa faccia, e un render solo.
 */
const NOT_DRAWN = [
  'name',
  'creatorName',
  'miiId',
  'systemId',
  'isFavorite',
  'birthMonth',
  'birthDay'
];

/** Firma dei soli campi che il renderer disegna. */
export function appearanceKey(state: MiiEditorState): string {
  return Object.entries(state)
    .filter(([field]) => !NOT_DRAWN.includes(field))
    .map(([field, value]) => `${field}=${String(value)}`)
    .join('|');
}

/**
 * Svuota la cache di sessione: dopo aver svuotato quella su disco, o dopo
 * aver installato il runtime, quando le facce "non riuscite" ora riescono.
 */
export function forget(): void {
  cache.clear();
}
