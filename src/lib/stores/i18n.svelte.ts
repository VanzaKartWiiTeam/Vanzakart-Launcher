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
 */

import { en } from '$lib/i18n/en';
import { it, type Dictionary } from '$lib/i18n/it';

export const LOCALES = ['it', 'en'] as const;

export type Locale = (typeof LOCALES)[number];

/** Il dizionario italiano è il riferimento: definisce le chiavi. */
export type TranslationKey = keyof typeof it;

/** Nome di ogni lingua, scritto nella lingua stessa: si riconosce sempre. */
export const LOCALE_LABELS: Record<Locale, string> = {
  it: 'Italiano',
  en: 'English'
};

const STORAGE_KEY = 'vk.locale';

const DICTIONARIES: Record<Locale, Dictionary> = { it, en };

function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (LOCALES as readonly string[]).includes(value);
}

/** Lingua di chi non ha ancora scelto: inglese. */
const DEFAULT_LOCALE: Locale = 'en';

/**
 * Lingua di partenza: quella scelta l'ultima volta, altrimenti l'inglese.
 *
 * Non si guarda la lingua del sistema: la community non è solo italiana e
 * l'inglese è la lingua che tutti leggono. Chi preferisce l'italiano lo
 * sceglie una volta in Impostazioni e la scelta resta.
 */
function initialLocale(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (isLocale(saved)) return saved;
  } catch {
    // Webview senza storage: resta la lingua predefinita.
  }

  return DEFAULT_LOCALE;
}

class I18nStore {
  locale = $state<Locale>(initialLocale());

  /** Tag BCP 47 per `Intl`: le date e i numeri seguono la lingua scelta. */
  get tag(): string {
    return this.locale === 'it' ? 'it-IT' : 'en-GB';
  }

  set(locale: Locale): void {
    if (locale === this.locale) return;
    this.locale = locale;
    this.apply();

    try {
      localStorage.setItem(STORAGE_KEY, locale);
    } catch {
      // Senza storage la scelta vale per questa sessione: meglio di niente.
    }
  }

  /** Allinea l'attributo `lang` del documento, che decide sillabazione e voce. */
  apply(): void {
    if (typeof document !== 'undefined') {
      document.documentElement.lang = this.locale;
    }
  }
}

export const i18n = new I18nStore();

/**
 * Testo tradotto.
 *
 * I segnaposto sono `{nome}`; i valori arrivano in `params`. Se una chiave
 * manca nella lingua scelta si ricade sull'italiano invece di mostrare la
 * chiave: un testo nella lingua sbagliata si legge, `mods.badge.title` no.
 */
export function t(key: TranslationKey, params?: Record<string, string | number>): string {
  const dictionary = DICTIONARIES[i18n.locale];
  let text: string = dictionary[key] ?? it[key] ?? key;

  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.split(`{${name}}`).join(String(value));
    }
  }

  return text;
}

/** Numero formattato secondo la lingua scelta (separatore delle migliaia). */
export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  return new Intl.NumberFormat(i18n.tag, options).format(value);
}
