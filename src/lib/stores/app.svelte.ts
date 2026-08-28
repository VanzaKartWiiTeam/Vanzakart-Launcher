/**
 * Stato dell'applicazione, in rune Svelte 5.
 *
 * Sostituisce il `LauncherNavigationService` e i campi di `MainWindow` del
 * launcher WPF: una sola fonte di verità, osservata da tutte le pagine.
 */

import * as api from '$lib/api';
import type {
  Channel,
  LauncherStatus,
  ModStatus,
  ProgressEvent,
  SettingsView
} from '$lib/api/types';

/** Le pagine, nello stesso ordine della sidebar legacy. */
export const ROUTES = [
  'home',
  'news',
  'rooms',
  'leaderboard',
  'friends',
  'mods',
  'licenses',
  'settings',
  'debug'
] as const;

export type Route = (typeof ROUTES)[number];

export interface Toast {
  id: number;
  title: string;
  message: string;
  tone: 'info' | 'success' | 'warning' | 'danger';
}

const IDLE_PROGRESS: ProgressEvent = {
  operation: '',
  phase: 'Idle',
  detail: '',
  percent: null,
  bytesDone: 0,
  bytesTotal: 0,
  filesDone: 0,
  filesTotal: 0,
  bytesLabel: '',
  speedLabel: ''
};

class AppStore {
  route = $state<Route>('home');
  /** La pagina Debug è nascosta finché non viene sbloccata, come nel legacy. */
  debugUnlocked = $state(false);

  status = $state<LauncherStatus | null>(null);
  settings = $state<SettingsView | null>(null);

  busy = $state(false);
  progress = $state<ProgressEvent>({ ...IDLE_PROGRESS });
  statusLine = $state('Controllo dell’installazione locale…');
  statusTone = $state<'info' | 'success' | 'warning' | 'danger'>('info');

  toasts = $state<Toast[]>([]);
  private nextToastId = 1;

  get modState(): ModStatus | null {
    return this.status?.modState ?? null;
  }

  get channel(): Channel {
    return this.status?.channel ?? 'Stable';
  }

  get visibleRoutes(): Route[] {
    return ROUTES.filter((route) => route !== 'debug' || this.debugUnlocked);
  }

  navigate(route: Route): void {
    if (route === 'debug' && !this.debugUnlocked) return;
    this.route = route;
  }

  setStatusLine(text: string, tone: AppStore['statusTone'] = 'info'): void {
    this.statusLine = text;
    this.statusTone = tone;
  }

  resetProgress(): void {
    this.progress = { ...IDLE_PROGRESS };
  }

  toast(title: string, message: string, tone: Toast['tone'] = 'info'): void {
    const toast: Toast = { id: this.nextToastId++, title, message, tone };
    this.toasts = [...this.toasts, toast];

    setTimeout(() => this.dismissToast(toast.id), 6000);
  }

  dismissToast(id: number): void {
    this.toasts = this.toasts.filter((item) => item.id !== id);
  }

  /** Ricarica stato e impostazioni dal backend. */
  async refresh(): Promise<void> {
    const [status, settings] = await Promise.all([api.getLauncherStatus(), api.getSettings()]);
    this.status = status;
    this.settings = settings;
    this.applyStatusLine(status);
  }

  private applyStatusLine(status: LauncherStatus): void {
    const mod = status.modState;

    if (!status.settingsComplete) {
      this.setStatusLine(
        'Configura Dolphin, la cartella User e la ROM in Impostazioni.',
        'warning'
      );
      return;
    }
    if (!mod.installed) {
      this.setStatusLine(
        `Modpack ${mod.channel} non installata. Aprila in Mods per installarla.`,
        'warning'
      );
      return;
    }
    if (mod.needsRepair) {
      this.setStatusLine(
        `Modpack ${mod.channel} da riparare: senza i suoi file Dolphin avvierebbe Mario Kart Wii originale.`,
        'danger'
      );
      return;
    }
    if (mod.updateAvailable) {
      this.setStatusLine(
        `Aggiornamento disponibile: ${mod.installedVersion || 'versione sconosciuta'} → ${mod.latestVersion}.`,
        'warning'
      );
      return;
    }
    this.setStatusLine(
      `${mod.channel} ${mod.installedVersion || ''} pronta. Buona gara.`.trim(),
      'success'
    );
  }
}

export const app = new AppStore();

/** Etichetta e sottotitolo di ogni pagina, come nell'header del WPF. */
export const PAGE_META: Record<Route, { title: string; subtitle: string; icon: string }> = {
  home: { title: 'Home / Play', subtitle: 'Pronto a correre su VanzaKart.', icon: 'play' },
  news: { title: 'News', subtitle: 'Aggiornamenti dalla community.', icon: 'news' },
  rooms: { title: 'Rooms', subtitle: 'Chi sta giocando adesso.', icon: 'rooms' },
  leaderboard: { title: 'Leaderboard', subtitle: 'Classifica VR globale.', icon: 'trophy' },
  friends: { title: 'Friends', subtitle: 'Amici salvati nella licenza.', icon: 'friends' },
  mods: { title: 'Mods', subtitle: 'Modpack, music pack e addon.', icon: 'package' },
  licenses: { title: 'Mii & Licenses', subtitle: 'Profili, Mii e salvataggi.', icon: 'license' },
  settings: { title: 'Settings', subtitle: 'Dolphin, controller e canale.', icon: 'settings' },
  debug: { title: 'Debug', subtitle: 'Diagnostica e log.', icon: 'debug' }
};

/** Formatta una durata in minuti come la mostrava il launcher legacy. */
export function formatPlayTime(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes <= 0) return '0 min';
  if (minutes < 60) return `${Math.round(minutes)} min`;

  const hours = Math.floor(minutes / 60);
  const rest = Math.round(minutes % 60);
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}

const DATE_FORMAT = new Intl.DateTimeFormat('it-IT', {
  day: '2-digit',
  month: 'short',
  year: 'numeric'
});

/**
 * Formatta una data ISO in forma breve, o "Mai".
 *
 * Usa `Date.parse` invece di costruire un `Date`: qui serve solo un timestamp
 * da formattare, non un oggetto data da tenere in stato.
 */
export function formatDate(iso: string | null): string {
  if (!iso) return 'Mai';

  const timestamp = Date.parse(iso);
  return Number.isNaN(timestamp) ? 'Mai' : DATE_FORMAT.format(timestamp);
}

/**
 * Distanza da adesso in forma breve: "3 min fa", "2 g fa".
 *
 * Serve dove conta *quanto è vecchio* un dato e non quando è stato prodotto —
 * lo snapshot delle stanze, l'ultima volta che un giocatore è stato visto.
 * Oltre il mese torna alla data piena, che a quel punto dice di più.
 */
export function formatRelative(iso: string | null): string {
  if (!iso) return '';

  const timestamp = Date.parse(iso);
  if (Number.isNaN(timestamp)) return '';

  const seconds = Math.round((Date.now() - timestamp) / 1000);
  if (seconds < 60) return 'adesso';

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min fa`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h fa`;

  const days = Math.floor(hours / 24);
  if (days === 1) return 'ieri';
  if (days < 30) return `${days} g fa`;

  return formatDate(iso);
}

/** Byte in forma leggibile, con le stesse unità del backend. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${Math.round(value)} B` : `${value.toFixed(1)} ${units[unit]}`;
}
