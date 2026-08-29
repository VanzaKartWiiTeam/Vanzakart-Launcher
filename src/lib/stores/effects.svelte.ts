/**
 * Effetti visivi: pieni o ridotti.
 *
 * Le decorazioni che si muovono da sole — lo sfondo animato, il gradiente che
 * scorre sul bordo dei pulsanti, lo scintillio degli scheletri — costano un
 * ridisegno per fotogramma, e su una webview di sistema con schermo Retina (e
 * magari a 120 Hz) quel costo si sente. Chi ha una macchina che fatica spegne
 * gli effetti e tiene tutto il resto (§D-082).
 *
 * La scelta vive nello storage del browser come la lingua: è una preferenza
 * della finestra, non un dato del launcher.
 */

const STORAGE_KEY = 'vk.effects.reduced';

function initial(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1';
  } catch {
    // Webview senza storage: effetti pieni, come sempre.
    return false;
  }
}

class EffectsStore {
  reduced = $state<boolean>(initial());

  set(reduced: boolean): void {
    if (reduced === this.reduced) return;
    this.reduced = reduced;
    this.apply();

    try {
      localStorage.setItem(STORAGE_KEY, reduced ? '1' : '0');
    } catch {
      // Senza storage la scelta vale per questa sessione.
    }
  }

  toggle(): void {
    this.set(!this.reduced);
  }

  /** Scrive l'attributo che i CSS leggono. */
  apply(): void {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    if (this.reduced) root.dataset.effects = 'reduced';
    else delete root.dataset.effects;
  }
}

export const effects = new EffectsStore();
