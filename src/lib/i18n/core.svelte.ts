/**
 * Il meccanismo delle lingue, senza le parole.
 *
 * Launcher e installer hanno dizionari diversi ma la stessa storia: una sola
 * lingua tenuta in memoria, una `t()` che la legge — così ogni testo scritto
 * nel markup si riscrive da sé quando la lingua cambia — e la scelta
 * ricordata per la volta dopo. Qui c'è il meccanismo; le parole le porta chi
 * lo usa (§D-078, §D-081).
 */

export const LOCALES = ['it', 'en'] as const;

export type Locale = (typeof LOCALES)[number];

/** Nome di ogni lingua, scritto nella lingua stessa: si riconosce sempre. */
export const LOCALE_LABELS: Record<Locale, string> = {
  it: 'Italiano',
  en: 'English'
};

/** Lingua di chi non ha ancora scelto: inglese. */
export const DEFAULT_LOCALE: Locale = 'en';

/** Un dizionario è una tabella piatta di chiavi e frasi. */
export type Phrasebook = Record<string, string>;

export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (LOCALES as readonly string[]).includes(value);
}

/** Tag BCP 47 per `Intl`: date e numeri seguono la lingua scelta. */
function tag(locale: Locale): string {
  return locale === 'it' ? 'it-IT' : 'en-GB';
}

class LocaleStore {
  locale = $state<Locale>(DEFAULT_LOCALE);

  readonly #storageKey: string;

  constructor(storageKey: string) {
    this.#storageKey = storageKey;
    this.locale = this.#saved() ?? DEFAULT_LOCALE;
  }

  get tag(): string {
    return tag(this.locale);
  }

  set(locale: Locale): void {
    if (locale === this.locale) return;
    this.locale = locale;
    this.apply();

    try {
      localStorage.setItem(this.#storageKey, locale);
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

  /**
   * Lingua scelta l'ultima volta, se c'è.
   *
   * Non si guarda la lingua del sistema, apposta: un programma che parte in
   * una lingua diversa a seconda del PC è più difficile da supportare di uno
   * che parte sempre uguale.
   */
  #saved(): Locale | null {
    try {
      const value = localStorage.getItem(this.#storageKey);
      return isLocale(value) ? value : null;
    } catch {
      // Webview senza storage: resta la lingua predefinita.
      return null;
    }
  }
}

export type I18nStore = LocaleStore;

/**
 * Store della lingua, `t()` e formattatori, legati ai dizionari passati.
 *
 * Il primo dizionario — l'italiano — è il riferimento: definisce le chiavi e
 * fa da ricaduta. Se una chiave mancasse nella lingua scelta si vede la
 * frase italiana invece della chiave: un testo nella lingua sbagliata si
 * legge, `steps.folder.title` no.
 */
export function createI18n<D extends Phrasebook>(
  dictionaries: Record<Locale, D>,
  storageKey: string
) {
  const i18n = new LocaleStore(storageKey);
  const reference = dictionaries.it;

  function t(key: keyof D & string, params?: Record<string, string | number>): string {
    let text: string = dictionaries[i18n.locale][key] ?? reference[key] ?? key;

    if (params) {
      for (const [name, value] of Object.entries(params)) {
        text = text.split(`{${name}}`).join(String(value));
      }
    }

    return text;
  }

  /** Numero formattato secondo la lingua scelta (separatore delle migliaia). */
  function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
    return new Intl.NumberFormat(i18n.tag, options).format(value);
  }

  return { i18n, t, formatNumber };
}
