/**
 * Wrapper tipizzati sui comandi IPC.
 *
 * Nessun componente chiama `invoke` direttamente: passare da qui garantisce
 * che i nomi dei comandi e le firme restino in un solo punto e che ogni
 * errore arrivi come `ApiError`.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  AddonView,
  ApiError,
  BackupSummary,
  BetaStatus,
  Channel,
  ConflictView,
  ControllerMode,
  ControllerProfile,
  ControllerView,
  DiagnosticEntry,
  DolphinSettings,
  FriendView,
  GameBananaSearchResult,
  InstallOutcome,
  IntegrityReport,
  LaunchBlocker,
  LauncherStatus,
  LaunchResult,
  LeaderboardEntry,
  LicenseView,
  MarioKartAction,
  MiiEditorState,
  MiiView,
  LauncherUpdateStatus,
  MiiRendererStatus,
  ModStatus,
  MusicPackOutcome,
  MusicPackStatus,
  NewsItem,
  ProgressEvent,
  RoomsSummary,
  SaveOverview,
  SettingsView
} from './types';

export * from './types';

/** Evento con cui il backend spinge i progressi. */
export const PROGRESS_EVENT = 'vk://progress';

/** `true` se l'errore ha la forma prodotta dal backend. */
export function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    'message' in value &&
    typeof (value as ApiError).message === 'string'
  );
}

/** Messaggio leggibile per qualunque errore. */
export function errorMessage(error: unknown): string {
  if (isApiError(error)) return error.message;
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  return 'Errore imprevisto.';
}

/** Codice stabile dell'errore, per decidere dove indirizzare l'utente. */
export function errorCode(error: unknown): string {
  return isApiError(error) ? error.code : 'unknown';
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

// --- Stato generale -------------------------------------------------------

export const getLauncherStatus = () => call<LauncherStatus>('launcher_status');
export const bootstrap = () => call<ModStatus>('bootstrap');

// --- Modpack --------------------------------------------------------------

export const getModStatus = () => call<ModStatus>('mods_status');
export const checkUpdates = () => call<ModStatus>('mods_check_updates');
export const installMods = () => call<InstallOutcome>('mods_install');
export const repairMods = () => call<InstallOutcome>('mods_repair');
export const verifyMods = () => call<IntegrityReport>('mods_verify');
export const setChannel = (channel: Channel) => call<ModStatus>('mods_set_channel', { channel });
export const cancelOperation = () => call<void>('operation_cancel');

// --- GameBanana -----------------------------------------------------------

export const searchGameBanana = (query: string, sort: string, page = 1) =>
  call<GameBananaSearchResult>('gamebanana_search', { query, sort, page });

/** Il backend rilegge e valida l'URL: qui viaggiano solo gli identificativi. */
export const installGameBananaFile = (modId: number, fileId: number) =>
  call<AddonView>('gamebanana_install', { modId, fileId });

// --- Music pack -----------------------------------------------------------

export const getMusicPackStatus = () => call<MusicPackStatus>('music_pack_status');
export const installMusicPack = () => call<MusicPackOutcome>('music_pack_install');
export const setMusicPackEnabled = (enabled: boolean) =>
  call<MusicPackStatus>('music_pack_set_enabled', { enabled });
export const uninstallMusicPack = () => call<MusicPackStatus>('music_pack_uninstall');

// --- Avvio ----------------------------------------------------------------

export const launchPreflight = () => call<LaunchBlocker | null>('launch_preflight');
export const launchGame = () => call<LaunchResult>('launch_game');
export const finishSession = () => call<number>('launch_session_finished');

// --- Impostazioni ---------------------------------------------------------

export const getSettings = () => call<SettingsView>('settings_get');

export const updatePaths = (paths: {
  dolphinPath?: string;
  romPath?: string;
  userFolderPath?: string;
}) => call<SettingsView>('settings_update_paths', paths);

export const detectDolphin = () => call<SettingsView>('settings_detect_dolphin');

export const updatePreferences = (preferences: {
  separateSavegame?: boolean;
  myStuffEnabled?: boolean;
  autoCheckUpdates?: boolean;
  downloadConcurrency?: number;
}) => call<SettingsView>('preferences_update', preferences);

// --- Impostazioni di Dolphin ---------------------------------------------

export const getDolphinSettings = () => call<DolphinSettings>('dolphin_settings_get');
export const saveDolphinSettings = (settings: DolphinSettings) =>
  call<void>('dolphin_settings_save', { settings });
export const optimizeDolphin = (screenWidth: number) =>
  call<DolphinSettings>('dolphin_settings_optimize', { screenWidth });
export const resetDolphinCategory = (category: string) =>
  call<DolphinSettings>('dolphin_settings_reset', { category });
export const backupDolphinConfig = () => call<string>('dolphin_config_backup');
export const restoreDolphinConfig = (archive: string) =>
  call<void>('dolphin_config_restore', { archive });
export const deleteGameSettings = () => call<string[]>('dolphin_delete_game_settings');

// --- Community ------------------------------------------------------------

export const fetchNews = () => call<NewsItem[]>('news_fetch');
export const fetchRooms = () => call<RoomsSummary>('rooms_fetch');
export const fetchLeaderboard = (offset = 0) =>
  call<LeaderboardEntry[]>('leaderboard_fetch', { offset });

// --- Beta -----------------------------------------------------------------

export const getBetaStatus = () => call<BetaStatus>('beta_status');
export const verifyBetaToken = (token: string) => call<BetaStatus>('beta_verify', { token });
export const clearBetaToken = () => call<BetaStatus>('beta_clear');

// --- Diagnostica ----------------------------------------------------------

export const collectDiagnostics = () => call<DiagnosticEntry[]>('diagnostics_collect');
export const readLog = () => call<string>('diagnostics_log');
export const listBackups = () => call<BackupSummary[]>('diagnostics_backups');
export const purgeUserData = (confirmation: string) =>
  call<string[]>('diagnostics_purge', { confirmation });

// --- Addon ----------------------------------------------------------------

export const listAddons = () => call<AddonView[]>('addons_list');
export const importAddon = (archive: string, name: string) =>
  call<AddonView>('addons_import', { archive, name });
export const setAddonEnabled = (id: string, enabled: boolean) =>
  call<AddonView>('addons_set_enabled', { id, enabled });
export const removeAddon = (id: string) => call<void>('addons_remove', { id });

export const getConflicts = () => call<ConflictView[]>('addons_conflicts');

// --- Controller -----------------------------------------------------------

export const scanControllers = () => call<ControllerView[]>('controllers_scan');
export const getControllerProfile = () => call<ControllerProfile>('controller_profile_get');
export const saveControllerProfile = (profile: ControllerProfile) =>
  call<void>('controller_profile_save', { profile });
export const getControllerActions = () => call<MarioKartAction[]>('controller_actions');
export const getControllerMode = () => call<ControllerMode>('controller_mode_get');
export const setControllerMode = (mode: ControllerMode) =>
  call<ControllerMode>('controller_mode_set', { mode });

/** Attende un input; `null` allo scadere del timeout di 8 secondi. */
export const captureBinding = (device: string) =>
  call<string | null>('controller_capture', { device });

export const rumbleController = (device: string) => call<boolean>('controller_rumble', { device });

/** Stato dell'aggiornamento del launcher: legge l'ultimo controllo, non la rete. */
export const getLauncherUpdateStatus = () => call<LauncherUpdateStatus>('launcher_update_status');

// --- Licenze e salvataggi -------------------------------------------------

export const listLicenses = () => call<LicenseView[]>('licenses_list');
export const getSaveOverview = () => call<SaveOverview>('saves_overview');
export const backupSave = () => call<string>('saves_backup');
export const listSaveBackups = () => call<string[]>('saves_backups');

/** Sostituisce il salvataggio corrente con un `rksys.dat` scelto dall'utente. */
export const importSave = (source: string) => call<string>('saves_import', { source });

/** Copia il salvataggio corrente dove l'utente ha scelto. */
export const exportSave = (destination: string) => call<string>('saves_export', { destination });

/** Rimette in gioco uno dei backup elencati da `listSaveBackups`. */
export const restoreSaveBackup = (name: string) => call<string>('saves_restore', { name });

/** Assegna un Mii del launcher a una licenza del salvataggio. */
export const setLicenseMii = (saveIndex: number, license: number, miiId: string) =>
  call<LicenseView[]>('licenses_set_mii', { saveIndex, license, miiId });

// --- Amici ----------------------------------------------------------------

export const listFriends = (saveIndex: number, license: number) =>
  call<FriendView[]>('friends_list', { saveIndex, license });

export const addFriend = (saveIndex: number, license: number, friendCode: string) =>
  call<FriendView[]>('friends_add', { saveIndex, license, friendCode });

export const removeFriend = (saveIndex: number, license: number, slot: number) =>
  call<FriendView[]>('friends_remove', { saveIndex, license, slot });

// --- Mii ------------------------------------------------------------------

export const listMiis = () => call<MiiView[]>('mii_list');

export const createMii = (name: string, favoriteColorIndex: number, isFemale: boolean) =>
  call<MiiView>('mii_create', { name, favoriteColorIndex, isFemale });

export const createMiiFromState = (editor: MiiEditorState) =>
  call<MiiView>('mii_create_from_state', { editor });

export const getMiiEditorState = (id: string) => call<MiiEditorState>('mii_editor_state', { id });

export const updateMii = (id: string, editor: MiiEditorState) =>
  call<MiiView>('mii_update', { id, editor });

export const duplicateMii = (id: string) => call<MiiView>('mii_duplicate', { id });
/** Elimina il Mii dal database di Dolphin: è l'unico posto in cui esiste. */
export const deleteMii = (id: string) => call<void>('mii_delete', { id });

export const importMii = (source: string) => call<MiiView>('mii_import', { source });

export const exportMii = (id: string, destination: string) =>
  call<string>('mii_export', { id, destination });

export const randomMiiState = (name: string) => call<MiiEditorState>('mii_random_state', { name });

export const defaultMiiState = (name: string, favoriteColorIndex: number, isFemale: boolean) =>
  call<MiiEditorState>('mii_default_state', { name, favoriteColorIndex, isFemale });

export const getMiiFavoriteColors = () => call<string[]>('mii_favorite_colors');

// --- Render dei Mii -------------------------------------------------------

export const getMiiRendererStatus = () => call<MiiRendererStatus>('mii_renderer_status');
export const installMiiRenderer = () => call<MiiRendererStatus>('mii_renderer_install');
export const removeMiiRenderer = () => call<MiiRendererStatus>('mii_renderer_remove');

/** Inquadrature che il servizio di render sa produrre. */
export type MiiRenderKind = 'face' | 'all_body';

/**
 * Render di una `studioData` già nota — licenza, amico o profilo — come
 * `data:` URI. `null` quando il servizio non risponde: la UI ha la silhouette.
 */
export const renderMiiStudio = (studioData: string, kind: MiiRenderKind = 'face', rotation = 0) =>
  call<string | null>('mii_render_studio', { studioData, kind, rotation });

/** Render di uno stato dell'editor, senza salvarlo da nessuna parte. */
export const renderMiiState = (
  editor: MiiEditorState,
  kind: MiiRenderKind = 'face',
  rotation = 0
) => call<string | null>('mii_render_state', { editor, kind, rotation });

export const clearMiiAvatars = () => call<number>('mii_avatars_clear');

/**
 * Cartelle apribili. Il frontend passa una chiave, mai un percorso: la
 * corrispondenza vive nel backend.
 */
/**
 * Cartelle apribili dall'interfaccia.
 *
 * `mod` e `addons` non stanno sotto la cartella dati del launcher: dipendono
 * dal canale e le risolve il backend a partire dal layout della modpack.
 */
export type KnownFolder =
  'data' | 'logs' | 'backups' | 'cache' | 'mii' | 'downloads' | 'mod' | 'addons';

export const openFolder = (key: KnownFolder) => call<string>('open_known_folder', { key });
export const openExternal = (url: string) => call<void>('open_external', { url });

// --- Eventi ---------------------------------------------------------------

/** Si iscrive agli eventi di progresso. Restituisce la funzione di rimozione. */
export function onProgress(handler: (event: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>(PROGRESS_EVENT, (event) => handler(event.payload));
}
