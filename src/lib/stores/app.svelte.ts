/**
 * Stato dell'applicazione, in rune Svelte 5.
 *
 * Sostituisce il `LauncherNavigationService` e i campi di `MainWindow` del
 * launcher WPF: una sola fonte di verità, osservata da tutte le pagine.
 */

import * as api from '$lib/api';
import { i18n, t, type TranslationKey } from '$lib/stores/i18n.svelte';
import type {
  Channel,
  LauncherStatus,
  LauncherUpdateStatus,
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
  statusTone = $state<'info' | 'success' | 'warning' | 'danger'>('info');

  /**
   * La riga di stato, tenuta come chiave finché è una frase nostra.
   *
   * Cambiando lingua deve cambiare anche quello che c'è scritto adesso, non
   * solo il prossimo messaggio: perciò si conserva la chiave e si traduce
   * quando la si legge. I testi che arrivano dal backend — il dettaglio di
   * un progresso — restano come sono: per quelli non abbiamo una chiave.
   */
  private statusText = $state('');
  private statusKey = $state<TranslationKey | null>('status.checking');
  private statusParams = $state<Record<string, string | number>>({});

  get statusLine(): string {
    return this.statusKey === null ? this.statusText : t(this.statusKey, this.statusParams);
  }

  toasts = $state<Toast[]>([]);
  private nextToastId = 1;

  /**
   * Aggiornamento del launcher, distinto da quello della modpack.
   *
   * Sta qui e non nella home perché lo guardano in due: la card della home e
   * l'avviso che compare all'avvio (§D-075).
   */
  launcherUpdate = $state<LauncherUpdateStatus | null>(null);

  /** `true` mentre è aperta la finestra che scarica l'aggiornamento. */
  updaterOpen = $state(false);

  /** L'avviso d'avvio si mostra una volta per sessione. */
  updateNoticeDismissed = $state(false);

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

  /** Riga di stato con un testo già pronto: arriva dal backend. */
  setStatusLine(text: string, tone: AppStore['statusTone'] = 'info'): void {
    this.statusKey = null;
    this.statusText = text;
    this.statusTone = tone;
  }

  /** Riga di stato scritta da noi: si ritraduce se cambia la lingua. */
  setStatusKey(
    key: TranslationKey,
    params: Record<string, string | number> = {},
    tone: AppStore['statusTone'] = 'info'
  ): void {
    this.statusKey = key;
    this.statusParams = params;
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

  /**
   * Rilegge lo stato dell'aggiornamento del launcher.
   *
   * Non è un errore da mostrare: se il controllo non riesce, l'avviso e la
   * card semplicemente non compaiono.
   */
  async refreshLauncherUpdate(): Promise<void> {
    try {
      this.launcherUpdate = await api.getLauncherUpdateStatus();
    } catch {
      this.launcherUpdate = null;
    }
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
      this.setStatusKey('status.settingsIncomplete', {}, 'warning');
      return;
    }
    if (!mod.installed) {
      this.setStatusKey('status.notInstalled', { channel: mod.channel }, 'warning');
      return;
    }
    if (mod.needsRepair) {
      this.setStatusKey('status.needsRepair', { channel: mod.channel }, 'danger');
      return;
    }
    if (mod.updateAvailable) {
      this.setStatusKey(
        'status.updateAvailable',
        { from: mod.installedVersion || t('status.unknownVersion'), to: mod.latestVersion },
        'warning'
      );
      return;
    }
    this.setStatusKey(
      'status.ready',
      { channel: mod.channel, version: mod.installedVersion || '' },
      'success'
    );
  }
}

export const app = new AppStore();

/**
 * I link del team, in un posto solo.
 *
 * Li usano la card Community della sidebar e la scheda Team delle
 * impostazioni: due copie dello stesso indirizzo invecchiano in modo diverso.
 */
export const TEAM_LINKS = {
  website: 'https://vwfc.sitodaking.it/',
  discord: 'https://discord.gg/2UGhrCNV8t',
  paypal: 'https://www.paypal.com/paypalme/SossioStorto'
} as const;

/**
 * Titolo e sottotitolo di ogni pagina, come nell'header del WPF.
 *
 * Sono chiavi, non testo: l'header si riscrive quando cambia la lingua.
 */
export const PAGE_META: Record<
  Route,
  { title: TranslationKey; subtitle: TranslationKey; icon: string }
> = {
  home: { title: 'page.home.title', subtitle: 'page.home.subtitle', icon: 'play' },
  news: { title: 'page.news.title', subtitle: 'page.news.subtitle', icon: 'news' },
  rooms: { title: 'page.rooms.title', subtitle: 'page.rooms.subtitle', icon: 'rooms' },
  leaderboard: {
    title: 'page.leaderboard.title',
    subtitle: 'page.leaderboard.subtitle',
    icon: 'trophy'
  },
  friends: { title: 'page.friends.title', subtitle: 'page.friends.subtitle', icon: 'friends' },
  mods: { title: 'page.mods.title', subtitle: 'page.mods.subtitle', icon: 'package' },
  licenses: { title: 'page.licenses.title', subtitle: 'page.licenses.subtitle', icon: 'license' },
  settings: { title: 'page.settings.title', subtitle: 'page.settings.subtitle', icon: 'settings' },
  debug: { title: 'page.debug.title', subtitle: 'page.debug.subtitle', icon: 'debug' }
};

/** Formatta una durata in minuti come la mostrava il launcher legacy. */
export function formatPlayTime(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes <= 0) return t('time.zeroMinutes');
  if (minutes < 60) return t('time.minutes', { count: Math.round(minutes) });

  const hours = Math.floor(minutes / 60);
  const rest = Math.round(minutes % 60);
  return rest === 0
    ? t('time.hours', { count: hours })
    : t('time.hoursMinutes', { hours, minutes: rest });
}

/**
 * Formatta una data ISO in forma breve, o "Mai".
 *
 * Il formattatore si costruisce a ogni chiamata perché dipende dalla lingua
 * scelta: `Intl` costa poco, e così la data cambia insieme al resto.
 *
 * Usa `Date.parse` invece di costruire un `Date`: qui serve solo un timestamp
 * da formattare, non un oggetto data da tenere in stato.
 */
export function formatDate(iso: string | null): string {
  if (!iso) return t('common.never');

  const timestamp = Date.parse(iso);
  if (Number.isNaN(timestamp)) return t('common.never');

  return new Intl.DateTimeFormat(i18n.tag, {
    day: '2-digit',
    month: 'short',
    year: 'numeric'
  }).format(timestamp);
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
  if (seconds < 60) return t('time.now');

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t('time.minutesAgo', { count: minutes });

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t('time.hoursAgo', { count: hours });

  const days = Math.floor(hours / 24);
  if (days === 1) return t('time.yesterday');
  if (days < 30) return t('time.daysAgo', { count: days });

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
