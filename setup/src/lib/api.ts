/**
 * Wrapper tipizzati sui comandi dell'installer.
 *
 * Come nel launcher, nessun componente chiama `invoke` direttamente: i nomi
 * dei comandi stanno tutti qui, e nessun indirizzo del server compare mai nel
 * frontend (docs/decisions.md §D-005).
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type SetupMode = 'install' | 'uninstall';
export type InstallMode = 'fresh' | 'update' | 'clean-reinstall';

export interface ApiError {
  code: string;
  message: string;
}

export interface ExistingInstall {
  installDir: string;
  version: string;
  managed: boolean;
  executable: string | null;
  bytes: number;
}

export interface Release {
  version: string;
  notes: string;
  pubDate: string;
  packageKey: string;
  sizeBytes: number;
  verifiable: boolean;
}

export interface Bootstrap {
  mode: SetupMode;
  platform: string;
  target: string;
  setupVersion: string;
  defaultInstallDir: string;
  suggestedInstallDirs: string[];
  defaultBackupDir: string;
  supportsQuickLaunch: boolean;
  supportsPathSymlink: boolean;
  existing: ExistingInstall | null;
  /** Cartella del launcher precedente in C#, se è ancora installato. */
  legacyInstallDir: string | null;
  release: Release | null;
  releaseError: string | null;
  downloadPageUrl: string;
}

export interface Preflight {
  target: string;
  version: string;
  installDir: string;
  downloadBytes: number;
  requiredBytes: number;
  availableBytes: number;
  enoughSpace: boolean;
  writable: boolean;
  launcherRunning: boolean;
  verifiable: boolean;
}

export interface Artifact {
  kind: string;
  path: string;
}

export interface InstallReport {
  installDir: string;
  executable: string;
  uninstaller: string | null;
  version: string;
  target: string;
  bytes: number;
  artifacts: Artifact[];
  backup: string | null;
  downloadSummary: string;
}

export interface InstallOptionsInput {
  installDir: string;
  mode: InstallMode;
  backupData: boolean;
  backupDir: string;
  desktopShortcut: boolean;
  startMenuShortcut: boolean;
  quickLaunchShortcut: boolean;
  uninstallEntry: boolean;
  pathSymlink: boolean;
}

export interface UninstallOptions {
  removeLauncherData: boolean;
  removeCacheAndLogs: boolean;
  removeModpacks: boolean;
  removeModpackUserData: boolean;
}

export interface RemovalItem {
  label: string;
  path: string;
  bytes: number;
  optional: boolean;
  exists: boolean;
}

export interface RemovalPlan {
  items: RemovalItem[];
  totalBytes: number;
  installDir: string;
  version: string;
  managed: boolean;
  hasModpacks: boolean;
}

export interface FailedRemoval {
  path: string;
  reason: string;
}

export interface UninstallReport {
  removed: string[];
  failed: FailedRemoval[];
  deferred: boolean;
  bytesFreed: number;
}

export interface ProgressEvent {
  phase: string;
  detail: string;
  percent: number | null;
  bytesDone: number;
  bytesTotal: number;
  bytesLabel: string;
  speedLabel: string;
  etaLabel: string;
}

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

/** Messaggio leggibile per qualunque cosa arrivi da un `catch`. */
export function errorMessage(error: unknown): string {
  if (isApiError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}

/** Codice dell'errore, o stringa vuota. */
export function errorCode(error: unknown): string {
  return isApiError(error) ? error.code : '';
}

export const bootstrap = () => invoke<Bootstrap>('setup_bootstrap');

export const refreshRelease = () => invoke<Release>('setup_refresh');

export const preflight = (installDir: string) =>
  invoke<Preflight>('setup_preflight', { installDir });

export const install = (options: InstallOptionsInput) =>
  invoke<InstallReport>('setup_install', { options });

export const cancel = () => invoke<void>('setup_cancel');

export const launch = (executable: string) => invoke<void>('setup_launch', { executable });

export const uninstallPlan = (options: UninstallOptions) =>
  invoke<RemovalPlan>('setup_uninstall_plan', { options });

export const uninstallRun = (options: UninstallOptions) =>
  invoke<UninstallReport>('setup_uninstall_run', { options });

export const openDownloadPage = () => invoke<void>('setup_open_download_page');

export const dataRoot = () => invoke<string | null>('setup_data_root');

/** Si iscrive ai progressi. Restituisce la funzione per disiscriversi. */
export function onProgress(handler: (event: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>(PROGRESS_EVENT, (event) => handler(event.payload));
}
