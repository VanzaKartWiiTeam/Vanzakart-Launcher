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
  /** Chiude da sé un Dolphin già aperto quando si preme Gioca. */
  closeRunningDolphin: boolean;
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
  /** `true` se prima di avviare è stato chiuso un Dolphin già aperto. */
  closedPrevious: boolean;
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
  /** Grado dalla classifica; 0 se non ne ha o se non è ancora noto. */
  prestigeRank: number;
  rankImage: string | null;
  rankLabel: string;
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
  /** Giorni di gioco consecutivi; 0 quando la streak è persa. */
  streak: number;
  /** In vacanza: la streak è congelata, non persa. */
  streakVacation: boolean;
  /** Chiave dell'immagine del rank in `LeaderboardPage.badges`; vuota se non c'è. */
  badge: string;
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
/** Immagine di un rank, una per pagina di classifica. */
export interface BadgeView {
  /** Miniatura PNG come data URI. */
  image: string;
  /** Nome di un rank speciale (staff); vuoto per i gradi del gioco. */
  label: string;
}

export interface LeaderboardPage {
  entries: LeaderboardEntry[];
  /** Le immagini dei rank della pagina, una volta ciascuna. */
  badges: Record<string, BadgeView>;
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

/**
 * Offerta di aggiornamento, da `install.json`.
 *
 * È il manifest che legge anche l'installer: il pacchetto è lo stesso, e
 * l'aggiornamento finisce nella cartella in cui il launcher è installato.
 */
export interface LauncherUpdateOffer {
  current: string;
  latest: string;
  available: boolean;
  notes: string;
  pubDate: string;
  /** Cartella d'installazione: quella scelta a suo tempo, non una nuova. */
  installDir: string;
  sizeBytes: number;
  sizeLabel: string;
  enoughSpace: boolean;
  /** Il pacchetto dichiara un'impronta SHA-256. */
  verifiable: boolean;
  /** Il pacchetto è firmato: si sa chi lo ha pubblicato. */
  signed: boolean;
  /** L'installazione ha il registro scritto dall'installer. */
  managed: boolean;
  canInstall: boolean;
  /**
   * Lo scambio dei file richiede i permessi di amministratore: il launcher
   * è in Programmi e Windows chiederà una conferma (§D-093).
   */
  needsElevation: boolean;
  downloadPage: string;
  /** Perché da qui non si può aggiornare; vuoto quando si può. */
  blocked: string;
  blockedCode: string;
}

/** Esito dell'aggiornamento del launcher. */
export interface LauncherUpdateOutcome {
  version: string;
  installDir: string;
  bytes: number;
  /** Qualcosa della versione precedente sparirà al prossimo avvio. */
  cleanupPending: boolean;
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
  /** Nome di un rank speciale (staff); vuoto per i gradi del gioco. */
  rankLabel: string;
  lastSeen: string | null;
  streak: number;
  streakVacation: boolean;
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
  /** Le facce le disegna il launcher, senza rete (§D-092). */
  nativeReady: boolean;
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

/**
 * Intervallo di un campo numerico dell'editor, come lo accetta il Canale Mii.
 *
 * Viene dal backend (`vk_save::mii::LIMITS`): è l'unico posto in cui i limiti
 * esistono, e l'editor non propone mai un valore che il gioco rifiuterebbe.
 */
export interface MiiFieldLimit {
  field: MiiNumericField;
  min: number;
  max: number;
}

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

// --- Ghost del time trial ----------------------------------------------------

/**
 * Perché un ghost non si può installare. `''` quando si può.
 *
 * - `no-user-folder`: manca la cartella User di Dolphin;
 * - `mod-not-installed`: manca la modpack del canale scelto;
 * - `no-track-map`: la modpack non ha la tabella delle piste;
 * - `track-not-in-modpack`: la pista non c'è nella versione installata.
 */
export type GhostBlocker =
  '' | 'no-user-folder' | 'mod-not-installed' | 'no-track-map' | 'track-not-in-modpack';

export interface GhostRecordView {
  submissionId: number;
  /** Chi ha fatto il tempo: il nome del Mii, mai il profilo di sistema. */
  playerName: string;
  /** Profilo del caricamento, solo quando dice qualcosa in più del nome. */
  profileName: string;
  /** Due lettere, per esempio `IT`; vuoto se il server non lo sa. */
  country: string;
  finishTimeMs: number;
  finishTime: string;
  dateSet: string;
  character: string;
  vehicle: string;
  /** Il ghost del record è già stato scaricato dal launcher. */
  installed: boolean;
}

export interface GhostTrackView {
  id: number;
  courseId: number;
  name: string;
  category: string;
  laps: number;
  sortOrder: number;
  record: GhostRecordView | null;
  installable: boolean;
  blocker: GhostBlocker;
  /** Ghost già presenti nella cartella della pista. */
  installedCount: number;
}

export interface GhostCatalogView {
  tracks: GhostTrackView[];
  channel: Channel;
  cc: number;
  blocker: GhostBlocker;
  recordsAvailable: boolean;
}

export interface GhostEntryView {
  submissionId: number;
  rank: number;
  /** Chi ha fatto il tempo: il nome del Mii, mai il profilo di sistema. */
  playerName: string;
  /** Profilo del caricamento, solo quando dice qualcosa in più del nome. */
  profileName: string;
  country: string;
  countryName: string;
  finishTimeMs: number;
  finishTime: string;
  fastestLap: string;
  lapSplits: string[];
  character: string;
  vehicle: string;
  /** 0 volante, 1 Wii Remote + Nunchuk, 2 Classic Controller, 3 GameCube. */
  controller: number;
  automaticDrift: boolean;
  shroomless: boolean;
  dateSet: string;
  /** Scaricato dal launcher e ancora al suo posto. */
  installed: boolean;
}

export interface GhostLeaderboardView {
  track: GhostTrackView;
  entries: GhostEntryView[];
  page: number;
  totalPages: number;
  total: number;
  fastestLap: string;
}

export interface GhostInstallOutcome {
  submissionId: number;
  trackName: string;
  fileName: string;
  alreadyPresent: boolean;
  installedCount: number;
}
