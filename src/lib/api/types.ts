/**
 * Tipi condivisi con il backend Rust.
 *
 * Ogni interfaccia qui corrisponde a una struct in `src-tauri/src/domain`.
 * Sono scritte a mano di proposito: generarle aggiungerebbe un passo di build
 * per un contratto che cambia raramente e che vogliamo leggere in chiaro.
 */

export type Channel = 'Stable' | 'Beta';

export interface ModStatus {
  channel: Channel;
  installed: boolean;
  installedVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  checked: boolean;
  checkMessage: string;
  modFolder: string;
  otherChannelInstalled: boolean;
  otherChannelVersion: string;
  changelog: string[];
  /** La modpack risulta installata ma il suo descrittore Riivolution è inerte. */
  needsRepair: boolean;
  /** Motivo leggibile di needsRepair, vuoto quando non c'è nulla da riparare. */
  repairReason: string;
}

export interface PlayStats {
  lastPlayedUtc: string | null;
  launchCount: number;
  totalPlayTimeMinutes: number;
}

export interface LauncherStatus {
  launcherVersion: string;
  platform: string;
  channel: Channel;
  settingsComplete: boolean;
  missingSettings: string[];
  modState: ModStatus;
  stats: PlayStats;
  hasBetaToken: boolean;
  betaTokenMasked: string;
  dolphinDetected: boolean;
  dolphinRunning: boolean;
  /** `false` in una build senza la feature `save-writes`: sola lettura. */
  saveWritesEnabled: boolean;
}

export interface SettingsView {
  dolphinPath: string;
  dolphinValid: boolean;
  romPath: string;
  romValid: boolean;
  userFolderPath: string;
  userFolderValid: boolean;
  modFolder: string;
  controllerMode: string;
  detectedUserFolders: string[];
  separateSavegame: boolean;
  myStuffEnabled: boolean;
  autoCheckUpdates: boolean;
  downloadConcurrency: number;
}

export type ProgressPhase =
  | 'Connecting'
  | 'Backup'
  | 'Download'
  | 'Verifying'
  | 'Installing'
  | 'Updating'
  | 'Recovery'
  | 'Rollback'
  | 'Completed'
  | 'Error'
  | 'Idle';

export interface ProgressEvent {
  operation: string;
  phase: ProgressPhase;
  detail: string;
  percent: number | null;
  bytesDone: number;
  bytesTotal: number;
  filesDone: number;
  filesTotal: number;
  /** "12,4 MB / 40,0 MB", vuoto quando la dimensione totale non è nota. */
  bytesLabel: string;
  /** "3,2 MB/s", vuoto finché non c'è abbastanza traffico per misurarla. */
  speedLabel: string;
}

export interface InstallOutcome {
  channel: Channel;
  wasUpdate: boolean;
  version: string;
  mode: 'differential' | 'full-archive' | 'unknown';
  filesWritten: number;
  filesSkipped: number;
  filesPruned: number;
  summary: string;
  warnings: string[];
  backupId: string | null;
}

export interface IntegrityReport {
  checked: boolean;
  totalFiles: number;
  mismatched: string[];
  obsolete: string[];
  message: string;
}

export interface LaunchBlocker {
  code: string;
  message: string;
  navigateTo: string;
}

export interface LaunchResult {
  pid: number;
  descriptorPath: string;
  channel: Channel;
}

export interface NewsItem {
  title: string;
  category: string;
  version: string;
  summary: string;
  dateLabel: string;
  isPinned: boolean;
  mediaPath: string | null;
  mediaKind: 'image' | 'video' | 'link' | null;
}

export interface RoomPlayerView {
  name: string;
  friendCode: string;
  vr: number;
  br: number;
  isHost: boolean;
  /** Payload di render del Mii; vuoto quando il server non lo manda. */
  studioData: string;
  avatarInitial: string;
  accentColor: string;
}

export interface RoomView {
  id: string;
  name: string;
  host: string;
  playerCount: number;
  maxPlayers: number;
  mode: string;
  track: string;
  region: string;
  status: string;
  players: RoomPlayerView[];
}

export interface RoomsSummary {
  totalPlayers: number;
  totalRooms: number;
  publicRooms: number;
  privateRooms: number;
  /** Istante dello snapshot in RFC 3339, vuoto se il server non lo manda. */
  lastUpdated: string;
  /** Stato dichiarato dal server: "Online", "Online (Demo)", ... */
  status: string;
  /** Avviso del server, presente quando risponde con dati dimostrativi. */
  notice: string;
  rooms: RoomView[];
}

export interface LeaderboardEntry {
  position: number;
  name: string;
  points: number;
  friendCode: string;
  prestigeRank: number;
  wins: number;
  games: number;
  winrate: number;
  lastSeen: string | null;
  isSuspicious: boolean;
  vrLast24Hours: number;
  vrLastWeek: number;
  vrLastMonth: number;
  rankImage: string | null;
  /** Payload di render del Mii; vuoto quando il server non ne manda uno valido. */
  studioData: string;
  avatarInitial: string;
  accentColor: string;
}

/**
 * Una pagina di classifica.
 *
 * Il server ne manda al massimo cento righe per volta: `hasMore` dice se vale
 * la pena chiedere la pagina successiva.
 */
export interface LeaderboardPage {
  entries: LeaderboardEntry[];
  offset: number;
  hasMore: boolean;
}

export interface BetaStatus {
  hasToken: boolean;
  maskedToken: string;
  verified: boolean;
  message: string;
  networkError: boolean;
}

export interface DiagnosticEntry {
  label: string;
  value: string;
  ok: boolean | null;
}

export interface BackupSummary {
  id: string;
  path: string;
  fileCount: number;
}

export interface AddonView {
  id: string;
  name: string;
  author: string;
  source: string;
  sourceUrl: string;
  previewUrl: string;
  installedUtc: string;
  fileCount: number;
  enabled: boolean;
  managed: boolean;
}

export interface LicenseView {
  /**
   * Posizione del salvataggio nell'elenco dei file trovati. È così che il
   * frontend indica su quale file operare: i percorsi che riceve sono redatti.
   */
  saveIndex: number;
  slot: number;
  isEmpty: boolean;
  name: string;
  miiName: string;
  /** Identificativo del Mii che la licenza indica in `RFL_DB.dat`. */
  miiId: number;
  /**
   * Payload di render del Mii della licenza, vuoto quando quel Mii non e' nel
   * database di Dolphin. Si passa a `renderMiiStudio` per ottenerne la faccia.
   */
  studioData: string;
  friendCode: string;
  vr: number;
  br: number;
  races: number;
  wins: number;
  winRate: number;
  accentColor: string;
  avatarInitial: string;
  sourceLabel: string;
  savePath: string;
  region: string;
  friendCount: number;
}

/** Stato dell'aggiornamento del launcher, da `versions.json`. */
export interface LauncherUpdateStatus {
  current: string;
  latest: string;
  /** `true` solo se la versione pubblicata è più recente di quella in uso. */
  available: boolean;
  changelog: string[];
  downloadPage: string;
  checked: boolean;
  message: string;
}

/** Un amico salvato dentro una licenza. */
/**
 * Come va un giocatore secondo il server: le stesse righe della classifica.
 *
 * I numeri che `rksys.dat` tiene accanto a un amico li aggiorna il gioco solo
 * quando lo incontra online, quindi sono fermi all'ultimo incontro.
 */
export interface PlayerStatsView {
  position: number;
  name: string;
  points: number;
  wins: number;
  games: number;
  winrate: number;
  prestigeRank: number;
  /** Immagine del rank come data URI, quando esiste. */
  rankImage: string | null;
  lastSeen: string | null;
}

export interface FriendView {
  slot: number;
  friendCode: string;
  miiName: string;
  /** Payload di render del Mii dell'amico, letto dal salvataggio. */
  studioData: string;
  wins: number;
  losses: number;
  raceRating: number;
  battleRating: number;
  /** Richiesta inviata dal launcher, non ancora confermata dal server. */
  isPending: boolean;
  avatarInitial: string;
  accentColor: string;
  /** `null` se non è in classifica o se il server non risponde. */
  stats: PlayerStatsView | null;
}

export interface SaveOverview {
  userFolderConfigured: boolean;
  saveFiles: string[];
  miiCount: number;
  licenseCount: number;
  backupCount: number;
  message: string;
}

/**
 * Un file scaricabile di una mod GameBanana.
 *
 * L'URL di download non compare: resta nel backend, che lo rilegge dall'API al
 * momento dell'installazione e lo valida contro l'allowlist degli host.
 */
export interface GameBananaFile {
  fileId: number;
  fileName: string;
  description: string;
  sizeBytes: number;
  downloadCount: number;
  dateAddedUtc: string;
}

/** Una mod di GameBanana. */
export interface GameBananaMod {
  id: number;
  name: string;
  author: string;
  description: string;
  profileUrl: string;
  views: number;
  likes: number;
  downloads: number;
  /** Miniatura servita da `images.gamebanana.com`; vuota quando non c'è. */
  previewUrl: string;
  files: GameBananaFile[];
}

/** Una pagina di risultati di ricerca. */
export interface GameBananaSearchResult {
  mods: GameBananaMod[];
  totalAvailable: number;
  hasMore: boolean;
  /** Il catalogo dei nomi è stato troncato: la ricerca può essere parziale. */
  catalogTruncated: boolean;
}

/** Stato del music pack ufficiale per il canale selezionato. */
export interface MusicPackStatus {
  installed: boolean;
  enabled: boolean;
  installedVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  fileCount: number;
  changelog: string[];
  /** Vuoto quando il music pack è installabile; altrimenti spiega perché no. */
  blocker: string;
}

/** Esito di un'installazione o di un aggiornamento del music pack. */
export interface MusicPackOutcome {
  mode: string;
  version: string;
  filesWritten: number;
  filesPruned: number;
  summary: string;
}

/** Stato del rendering dei Mii: runtime del gioco e avatar del launcher. */
export interface MiiRendererStatus {
  /** `FFLResHigh.dat` presente: senza, Dolphin disegna sagome vuote. */
  runtimeInstalled: boolean;
  runtimeSizeBytes: number;
  cachedAvatars: number;
  /** Host che verrebbero contattati, per dirlo prima di contattarli. */
  runtimeHost: string;
  renderHost: string;
  message: string;
}

/**
 * Un Mii, letto dal database di Dolphin.
 *
 * Il launcher non ne tiene di propri: `id` è il Mii id in esadecimale, cioè la
 * stessa chiave con cui il gioco lo cerca in `RFL_DB.dat`.
 */
export interface MiiView {
  id: string;
  miiId: number;
  name: string;
  creatorName: string;
  favoriteColor: string;
  favoriteColorIndex: number;
  isFemale: boolean;
  /** Il flag "preferito" che il Mii porta con sé. */
  isFavorite: boolean;
  avatarInitial: string;
  /** Payload di render, da passare a `renderMiiStudio`. */
  studioData: string;
  height: number;
  weight: number;
}

/**
 * Lo stato completo dell'editor Mii: i ~60 campi che i 74 byte descrivono.
 * Corrisponde a `vk_save::mii::MiiEditorState`.
 */
export interface MiiEditorState {
  name: string;
  creatorName: string;
  isFemale: boolean;
  isFavorite: boolean;
  favoriteColorIndex: number;
  birthMonth: number;
  birthDay: number;
  height: number;
  weight: number;
  miiId: number;
  systemId: [number, number, number, number];

  faceShape: number;
  skinColor: number;
  facialFeature: number;

  hairType: number;
  hairColor: number;
  hairFlipped: boolean;

  eyebrowType: number;
  eyebrowRotation: number;
  eyebrowColor: number;
  eyebrowSize: number;
  eyebrowVertical: number;
  eyebrowSpacing: number;

  eyeType: number;
  eyeRotation: number;
  eyeVertical: number;
  eyeColor: number;
  eyeSize: number;
  eyeSpacing: number;

  noseType: number;
  noseSize: number;
  noseVertical: number;

  mouthType: number;
  mouthColor: number;
  mouthSize: number;
  mouthVertical: number;

  glassesType: number;
  glassesColor: number;
  glassesSize: number;
  glassesVertical: number;

  mustacheType: number;
  beardType: number;
  facialHairColor: number;
  mustacheSize: number;
  mustacheVertical: number;

  moleEnabled: boolean;
  moleSize: number;
  moleVertical: number;
  moleHorizontal: number;
}

/** Chiave numerica di `MiiEditorState`, per i cursori dell'editor. */
export type MiiNumericField = {
  [K in keyof MiiEditorState]: MiiEditorState[K] extends number ? K : never;
}[keyof MiiEditorState];

/** Chiave booleana di `MiiEditorState`, per gli interruttori dell'editor. */
export type MiiBooleanField = {
  [K in keyof MiiEditorState]: MiiEditorState[K] extends boolean ? K : never;
}[keyof MiiEditorState];

export interface ConflictView {
  fileName: string;
  count: number;
  locations: string[];
}

/** Le ~80 impostazioni di Dolphin, come le espone `vk-dolphin`. */
export interface DolphinSettings {
  gfxBackend: string;
  internalResolution: number;
  fullscreen: boolean;
  aspectRatio: number;
  vsync: boolean;
  antiAliasing: number;
  anisotropicFiltering: number;
  shaderCompilationMode: number;
  force169: boolean;
  widescreenHack: boolean;
  removeBlur: boolean;
  showFps: boolean;
  ubershaders: boolean;
  textureCacheAccuracy: number;
  frameLimit: number;
  refreshRate: number;

  audioVolume: number;
  audioBackend: string;
  dspLle: boolean;
  audioStretching: boolean;
  audioLatency: number;

  selectedPort: number;
  deviceTypePort1: string;
  deviceTypePort2: string;
  deviceTypePort3: string;
  deviceTypePort4: string;
  analogSensitivity: number;
  analogDeadzone: number;
  vibration: boolean;
  controllerPreset: string;

  wiiLanguage: number;
  wiiRegion: number;
  systemTimeSync: boolean;
  enableSdCard: boolean;
  forceDisableWiimote: boolean;
  launchInWindow: boolean;
  retroRewind: boolean;
  enableCheats: boolean;
  enableRiivolution: boolean;

  cpuOverride: boolean;
  cpuClockRatio: number;
  dualCore: boolean;
  syncGpu: string;
  skipIdle: boolean;
  fastDiscSpeed: boolean;
  performancePreset: string;

  loadCustomTextures: boolean;
  prefetchCustomTextures: boolean;
  postProcessingShader: string;
  enableBloom: boolean;
  enableAmbientOcclusion: boolean;
  enableColorCorrection: boolean;
  gamma: number;
  brightness: number;

  dolphinExecutablePath: string;
  userFolderPath: string;
  modpackPath: string;

  logLevel: string;
  logToFile: boolean;
  waitForShadersBeforeStarting: boolean;
  backendMultithreading: boolean;
  debugMode: boolean;
  portableMode: boolean;
}

export type ControllerMode = 'launcher-configuration' | 'configure-with-dolphin';

export type BindingKind = 'single' | 'trigger' | 'steering';

export interface ControllerView {
  id: string;
  name: string;
  kind: string;
  dolphinDevice: string;
  connected: boolean;
  supportsRumble: boolean;
  isConfigured: boolean;
}

export interface MarioKartAction {
  id: string;
  section: string;
  icon: string;
  title: string;
  description: string;
  kind: BindingKind;
  dolphin_keys: string[];
}

export interface DeviceRef {
  dolphinDevice: string;
  displayName: string;
  kind: string;
  connected: boolean;
  xinputSlot: number;
  supportsRumble: boolean;
}

export interface ControllerProfile {
  device: DeviceRef;
  bindings: Record<string, string>;
  deadzone: number;
  sensitivity: number;
  vibration: boolean;
  loadedFromDolphin: boolean;
  configuredDolphinDevice: string | null;
}

/** Errore restituito da un comando, già sanitizzato dal backend. */
export interface ApiError {
  code: string;
  message: string;
}
