/**
 * Lingua dell'installer.
 *
 * Stesso meccanismo del launcher — `$lib/i18n/core.svelte` — con i dizionari
 * dell'installer: una sola lingua in memoria, `t()` che la legge, e i testi
 * che si riscrivono da soli quando si cambia lingua dalla barra del titolo.
 *
 * L'installer si vede per due minuti, ma sono i primi due minuti: chi non
 * legge l'italiano deve poterli capire (§D-081).
 */

import { createI18n } from '$lib/i18n/core.svelte';
import { en } from './en';
import { it, type SetupDictionary } from './it';

export { LOCALES, LOCALE_LABELS, type Locale } from '$lib/i18n/core.svelte';

/** Il dizionario italiano è il riferimento: definisce le chiavi. */
export type SetupKey = keyof SetupDictionary;

/**
 * Chiave dello storage, la stessa del launcher: le due finestre hanno
 * storage separati, quindi non è memoria condivisa — è lo stesso nome per la
 * stessa preferenza.
 */
const STORAGE_KEY = 'vk.locale';

const setup = createI18n<SetupDictionary>({ it, en }, STORAGE_KEY);

export const i18n = setup.i18n;

/**
 * Testo tradotto. I segnaposto sono `{nome}`; i valori arrivano in `params`.
 * Se una chiave manca nella lingua scelta si ricade sull'italiano: un testo
 * nella lingua sbagliata si legge, `checks.title` no.
 */
export const t = setup.t;
