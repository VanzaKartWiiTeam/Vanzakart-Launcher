/**
 * Lingua del launcher.
 *
 * Una sola fonte di verità — `i18n.locale` — e una funzione `t()` che legge
 * quel campo: chiamandola dentro il markup, ogni testo si riscrive da solo
 * quando la lingua cambia, senza ricaricare la finestra.
 *
 * Le chiavi vivono in `it.ts` (riferimento) e `en.ts` (traduzione): il tipo
 * di `en` è derivato da quello di `it`, quindi una chiave dimenticata o di
 * troppo è un errore di compilazione, non una stringa mancante a runtime.
 *
 * Il meccanismo — lo store, la ricaduta, la scelta ricordata — sta in
 * `$lib/i18n/core.svelte`, perché lo usa anche l'installer con i suoi
 * dizionari (§D-081).
 */

import { createI18n } from '$lib/i18n/core.svelte';
import { en } from '$lib/i18n/en';
import { it } from '$lib/i18n/it';

export { LOCALES, LOCALE_LABELS, type Locale } from '$lib/i18n/core.svelte';

/** Il dizionario italiano è il riferimento: definisce le chiavi. */
export type TranslationKey = keyof typeof it;

/**
 * Chiave dello storage. La lingua del launcher e quella dell'installer si
 * scelgono in due finestre diverse, ognuna con il suo storage: lo stesso
 * nome dice che è la stessa preferenza, non che è la stessa memoria.
 */
const STORAGE_KEY = 'vk.locale';

const launcher = createI18n({ it, en }, STORAGE_KEY);

export const i18n = launcher.i18n;

/**
 * Testo tradotto.
 *
 * I segnaposto sono `{nome}`; i valori arrivano in `params`. Se una chiave
 * manca nella lingua scelta si ricade sull'italiano invece di mostrare la
 * chiave: un testo nella lingua sbagliata si legge, `mods.badge.title` no.
 */
export const t = launcher.t;

/** Numero formattato secondo la lingua scelta (separatore delle migliaia). */
export const formatNumber = launcher.formatNumber;
