<script lang="ts">
  /**
   * Settings.
   *
   * Ricalca il `SettingsView` del WPF: i percorsi, le opzioni di avvio, le
   * impostazioni di Dolphin per categoria, il canale di rilascio.
   *
   * Ogni opzione è una riga — nome a sinistra, controllo a destra — con la
   * spiegazione nel suggerimento della «i» invece che sotto, e un pallino
   * quando il valore è diverso da quello consigliato per VanzaKart. Le
   * modifiche a Dolphin si salvano dalla barra che compare in fondo solo
   * quando ce ne sono; le azioni rare stanno nel `⋯` (§D-102).
   */
  import { open } from '@tauri-apps/plugin-dialog';

  import * as api from '$lib/api';
  import ControllerPanel from '$lib/components/ControllerPanel.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import MenuButton, { type MenuItem } from '$lib/components/MenuButton.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import PathField from '$lib/components/PathField.svelte';
  import Select from '$lib/components/Select.svelte';
  import SettingRow from '$lib/components/SettingRow.svelte';
  import Slider from '$lib/components/Slider.svelte';
  import Switch from '$lib/components/Switch.svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import logo from '$lib/assets/logo.png';
  import { app, TEAM_LINKS } from '$lib/stores/app.svelte';
  import { i18n, t, LOCALES, LOCALE_LABELS, type TranslationKey } from '$lib/stores/i18n.svelte';
  import type { Channel, DolphinSettings } from '$lib/api/types';

  type Tab =
    'paths' | 'video' | 'audio' | 'controller' | 'wii' | 'performance' | 'advanced' | 'about';
  type DolphinTab = 'video' | 'audio' | 'wii' | 'performance' | 'advanced';

  /** Canale di rilascio: si sceglie qui, non nella pagina Mods (§D-039). */
  let betaModalOpen = $state(false);
  let betaToken = $state('');
  let betaBusy = $state(false);
  let betaMessage = $state('');
  let betaInput = $state<HTMLInputElement | null>(null);

  // Il dialogo si apre con il cursore già nel campo: Ctrl+V basta e avanza.
  $effect(() => {
    if (betaModalOpen) betaInput?.focus();
  });

  /**
   * Scrive nel campo un token arrivato dagli appunti.
   *
   * Un token si incolla intero, non a pezzi: il campo prende tutto il testo,
   * ripulito dagli a capo e dagli spazi che si porta dietro un copia-incolla
   * da chat o da mail. Torna `false` se negli appunti non c'era testo.
   */
  function fillBetaToken(text: string): boolean {
    const clean = text.trim();
    if (!clean) return false;
    betaToken = clean;
    return true;
  }

  /**
   * Incolla da tastiera.
   *
   * L'evento `paste` porta con sé il testo degli appunti senza chiedere
   * permessi, e arriva anche quando il fuoco non è nel campo: finché il
   * dialogo è aperto, un Ctrl+V ovunque riempie il token.
   */
  function onBetaPaste(event: ClipboardEvent) {
    if (!betaModalOpen || betaBusy) return;

    // Un altro campo a fuoco si tiene il suo incolla.
    const target = event.target;
    if (
      target !== betaInput &&
      (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)
    )
      return;

    if (!fillBetaToken(event.clipboardData?.getData('text') ?? '')) return;

    event.preventDefault();
    betaMessage = t('settings.betaPasted');
    betaInput?.focus();
  }

  /**
   * Incolla dal pulsante, per chi non ha la tastiera sotto mano o si trova su
   * una webview che il Ctrl+V non lo passa. Se gli appunti non si lasciano
   * leggere resta la scorciatoia, e il messaggio lo dice.
   */
  async function pasteBetaToken() {
    try {
      const text = await navigator.clipboard.readText();
      betaMessage = fillBetaToken(text)
        ? t('settings.betaPasted')
        : t('settings.betaClipboardEmpty');
    } catch {
      betaMessage = t('settings.betaPasteFailed');
    }
    betaInput?.focus();
  }

  const channel = $derived(app.modState?.channel ?? 'Stable');

  async function switchChannel(target: Channel) {
    if (target === channel) return;
    try {
      await api.setChannel(target);
      await app.refresh();
    } catch (error) {
      // Il canale Beta richiede un token: il backend lo dice con questo codice.
      if (api.errorCode(error) === 'configuration') {
        betaMessage = api.errorMessage(error);
        betaModalOpen = true;
        return;
      }
      app.toast(t('settings.channelFailed'), api.errorMessage(error), 'warning');
    }
  }

  async function submitBetaToken() {
    betaBusy = true;
    try {
      const status = await api.verifyBetaToken(betaToken);
      betaMessage = status.message;
      if (status.verified) {
        betaModalOpen = false;
        betaToken = '';
        await api.setChannel('Beta');
        await app.refresh();
        app.toast(t('settings.betaActive'), status.message, 'success');
      }
    } catch (error) {
      betaMessage = api.errorMessage(error);
    } finally {
      betaBusy = false;
    }
  }

  /**
   * La scheda Controller è nascosta finché il pannello non funziona come deve.
   *
   * Il pannello e la sua scheda restano dov'erano: rimettere `true` qui la
   * riporta al suo posto, senza altro da toccare. Nel frattempo i controller
   * si configurano da Dolphin, che è la modalità che il launcher già prevede.
   */
  const CONTROLLER_TAB_VISIBLE: boolean = false;

  /**
   * Le liste di etichette sono derivate, non costanti: si ricostruiscono da
   * sole quando cambia la lingua, senza ricreare la pagina.
   */
  const TABS: { id: Tab; label: string }[] = $derived([
    { id: 'paths', label: t('settings.tab.paths') },
    { id: 'video', label: t('settings.tab.video') },
    { id: 'audio', label: t('settings.tab.audio') },
    ...(CONTROLLER_TAB_VISIBLE
      ? [{ id: 'controller' as const, label: t('settings.tab.controller') }]
      : []),
    { id: 'wii', label: t('settings.tab.wii') },
    { id: 'performance', label: t('settings.tab.performance') },
    { id: 'advanced', label: t('settings.tab.advanced') },
    { id: 'about', label: t('settings.tab.about') }
  ]);

  /** Frecce fra le schede, come in ogni gruppo di linguette. */
  function onTabKey(event: KeyboardEvent) {
    const move = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (!move) return;
    event.preventDefault();
    const index = TABS.findIndex((item) => item.id === tab);
    const next = TABS[(index + move + TABS.length) % TABS.length];
    if (!next) return;
    tab = next.id;
    const buttons = (event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>(
      '[role="tab"]'
    );
    buttons[TABS.indexOf(next)]?.focus();
  }

  /**
   * I link del team.
   *
   * Passano dal backend come tutti gli altri indirizzi esterni: la webview non
   * apre niente da sé e l'URL viene validato prima di arrivare al browser.
   */
  async function openLink(url: string) {
    try {
      await api.openExternal(url);
    } catch (error) {
      app.toast(t('sidebar.openFailed'), api.errorMessage(error), 'warning');
    }
  }

  // -------------------------------------------------------------------------
  // Scelte delle impostazioni di Dolphin
  // -------------------------------------------------------------------------

  const RESOLUTIONS = $derived([
    { value: 0, label: t('settings.res.native') },
    { value: 1, label: '1× (480p)' },
    { value: 2, label: '2× (720p)' },
    { value: 3, label: '3× (1080p)' },
    { value: 4, label: '4× (1440p)' },
    { value: 5, label: '5×' },
    { value: 6, label: '6× (4K)' }
  ]);

  const named = (values: string[]) => values.map((value) => ({ value, label: value }));
  const BACKENDS = named(['Vulkan', 'D3D11', 'D3D12', 'OpenGL', 'Null']);
  const AUDIO_BACKENDS = named(['Cubeb', 'WASAPI', 'OpenAL', 'XAudio2', 'Null']);
  const LOG_LEVELS = named(['Notice', 'Error', 'Warning', 'Info', 'Debug']);
  const ASPECT_RATIOS = $derived([
    { value: 0, label: t('settings.aspect.auto') },
    { value: 1, label: t('settings.aspect.force169') },
    { value: 2, label: t('settings.aspect.force43') },
    { value: 3, label: t('settings.aspect.stretch') }
  ]);
  const REGIONS = [
    { value: 0, label: 'NTSC-J' },
    { value: 1, label: 'NTSC-U' },
    { value: 2, label: 'PAL' },
    { value: 3, label: 'NTSC-K' }
  ];
  const LANGUAGES = $derived([
    { value: 0, label: t('settings.lang.japanese') },
    { value: 1, label: t('settings.lang.english') },
    { value: 2, label: t('settings.lang.german') },
    { value: 3, label: t('settings.lang.french') },
    { value: 4, label: t('settings.lang.spanish') },
    { value: 5, label: t('settings.lang.italian') },
    { value: 6, label: t('settings.lang.dutch') }
  ]);

  type BoolField = {
    [K in keyof DolphinSettings]: DolphinSettings[K] extends boolean ? K : never;
  }[keyof DolphinSettings];

  /** Le levette di ogni scheda: sono tante, e tutte uguali. */
  const SWITCHES: Record<DolphinTab, { field: BoolField; label: TranslationKey }[]> = {
    video: [
      { field: 'fullscreen', label: 'settings.fullscreen' },
      { field: 'vsync', label: 'settings.vsync' },
      { field: 'widescreenHack', label: 'settings.widescreenHack' },
      { field: 'removeBlur', label: 'settings.removeBlur' },
      { field: 'showFps', label: 'settings.showFps' },
      { field: 'loadCustomTextures', label: 'settings.customTextures' }
    ],
    audio: [
      { field: 'audioStretching', label: 'settings.audioStretching' },
      { field: 'dspLle', label: 'settings.dspLle' }
    ],
    wii: [
      { field: 'enableRiivolution', label: 'settings.riivolution' },
      { field: 'enableCheats', label: 'settings.cheats' },
      { field: 'enableSdCard', label: 'settings.sdCard' },
      { field: 'forceDisableWiimote', label: 'settings.disableWiimoteSpeaker' }
    ],
    performance: [
      { field: 'dualCore', label: 'settings.dualCore' },
      { field: 'skipIdle', label: 'settings.skipIdle' },
      { field: 'fastDiscSpeed', label: 'settings.fastDisc' },
      { field: 'cpuOverride', label: 'settings.cpuOverride' }
    ],
    advanced: [
      { field: 'logToFile', label: 'settings.logToFile' },
      { field: 'backendMultithreading', label: 'settings.backendMultithread' },
      { field: 'waitForShadersBeforeStarting', label: 'settings.waitShaders' }
    ]
  };

  /** I campi che ogni scheda mostra: per contare quelli fuori dal consigliato. */
  const FIELDS: Record<DolphinTab, (keyof DolphinSettings)[]> = {
    video: [
      'gfxBackend',
      'internalResolution',
      'aspectRatio',
      ...SWITCHES.video.map((s) => s.field)
    ],
    audio: ['audioBackend', 'audioVolume', 'audioLatency', ...SWITCHES.audio.map((s) => s.field)],
    wii: ['wiiRegion', 'wiiLanguage', ...SWITCHES.wii.map((s) => s.field)],
    performance: [...SWITCHES.performance.map((s) => s.field), 'cpuClockRatio'],
    advanced: ['logLevel', ...SWITCHES.advanced.map((s) => s.field)]
  };

  function isDolphinTab(value: Tab): value is DolphinTab {
    return value in FIELDS;
  }

  let tab = $state<Tab>('paths');
  let dolphin = $state<DolphinSettings | null>(null);
  /** I soli campi che il preset consigliato imposta, col loro valore. */
  let recommended = $state<Partial<DolphinSettings>>({});
  let dirty = $state(false);
  let saving = $state(false);

  const settings = $derived(app.settings);
  const canEditDolphin = $derived(settings?.userFolderValid ?? false);
  const tabLabel = $derived(TABS.find((item) => item.id === tab)?.label ?? '');

  $effect(() => {
    if (canEditDolphin && dolphin === null) void loadDolphin();
  });

  function screenWidth(): number {
    return window.screen.width || 1920;
  }

  async function loadDolphin() {
    try {
      [dolphin, recommended] = await Promise.all([
        api.getDolphinSettings(),
        api.recommendedDolphin(screenWidth()).catch(() => ({}))
      ]);
      dirty = false;
    } catch (error) {
      app.toast(t('settings.dolphinSettings'), api.errorMessage(error), 'warning');
    }
  }

  function set<K extends keyof DolphinSettings>(field: K, value: DolphinSettings[K]) {
    if (!dolphin || dolphin[field] === value) return;
    dolphin[field] = value;
    dirty = true;
  }

  /** `true` se il campo è diverso da come lo vorrebbe il preset consigliato. */
  function differs(field: keyof DolphinSettings): boolean {
    if (!dolphin || !(field in recommended)) return false;
    const want = recommended[field];
    const have = dolphin[field];
    return typeof want === 'number' && typeof have === 'number'
      ? Math.abs(want - have) > 1e-6
      : want !== have;
  }

  /** Il suggerimento del pallino: il valore che il preset sceglierebbe. */
  function note(
    field: keyof DolphinSettings,
    options?: { value: string | number; label: string }[],
    unit = ''
  ): string | undefined {
    if (!differs(field)) return undefined;
    const want = recommended[field];
    const text =
      typeof want === 'boolean'
        ? want
          ? t('common.on')
          : t('common.off')
        : (options?.find((option) => option.value === want)?.label ?? `${String(want)}${unit}`);
    return t('settings.recommended', { value: text });
  }

  const offCount = $derived(
    isDolphinTab(tab) && dolphin ? FIELDS[tab].filter((field) => differs(field)).length : 0
  );

  function done(message: string) {
    app.toast(t('common.done'), message, 'success');
  }

  async function pickDolphin() {
    const selected = await open({
      multiple: false,
      directory: false,
      title: t('settings.pickDolphin'),
      filters: [{ name: 'Dolphin', extensions: ['exe', 'app', 'AppImage', '*'] }]
    });
    if (typeof selected === 'string') await applyPath({ dolphinPath: selected });
  }

  async function pickRom() {
    const selected = await open({
      multiple: false,
      directory: false,
      title: t('settings.pickRom'),
      filters: [
        { name: t('settings.romFilter'), extensions: ['wbfs', 'iso', 'rvz', 'ciso', 'gcm', 'wia'] }
      ]
    });
    if (typeof selected === 'string') await applyPath({ romPath: selected });
  }

  async function pickUserFolder() {
    const selected = await open({
      multiple: false,
      directory: true,
      title: t('settings.pickUserFolder')
    });
    if (typeof selected === 'string') await applyPath({ userFolderPath: selected });
  }

  /** Come `savePath`, ma per il selettore: lì l'errore va nell'avviso. */
  async function applyPath(paths: Parameters<typeof api.updatePaths>[0]) {
    const failure = await savePath(paths);
    if (failure) app.toast(t('settings.pathInvalid'), failure, 'warning');
  }

  /**
   * Salva un percorso. Restituisce il messaggio d'errore, o `null` se è
   * andata: chi scrive a mano lo vede sotto il campo, dove sta guardando,
   * invece che in un avviso che passa (§D-076).
   */
  async function savePath(paths: Parameters<typeof api.updatePaths>[0]): Promise<string | null> {
    try {
      app.settings = await api.updatePaths(paths);
      await app.refresh();
      dolphin = null;
      done(t('settings.pathUpdated'));
      return null;
    } catch (error) {
      return api.errorMessage(error);
    }
  }

  async function autoDetect() {
    try {
      app.settings = await api.detectDolphin();
      await app.refresh();
      if (app.settings.dolphinValid) done(t('settings.detected'));
      else app.toast(t('settings.detectFailed'), t('settings.notDetected'), 'warning');
    } catch (error) {
      app.toast(t('settings.detectFailed'), api.errorMessage(error), 'warning');
    }
  }

  async function updatePreference(patch: Parameters<typeof api.updatePreferences>[0]) {
    try {
      app.settings = await api.updatePreferences(patch);
    } catch (error) {
      app.toast(t('settings.prefFailed'), api.errorMessage(error), 'warning');
    }
  }

  /** Il cursore dei download mentre lo si trascina: si salva al rilascio. */
  let concurrencyDraft = $state<number | null>(null);

  async function saveDolphin() {
    if (!dolphin) return;
    saving = true;
    try {
      await api.saveDolphinSettings(dolphin);
      dirty = false;
      done(t('settings.dolphinSaved'));
    } catch (error) {
      app.toast(t('settings.saveFailed'), api.errorMessage(error), 'danger');
    } finally {
      saving = false;
    }
  }

  /** Scarta le modifiche non salvate: si rilegge quello che c'è negli INI. */
  function discard() {
    dolphin = null;
    dirty = false;
  }

  async function optimize() {
    saving = true;
    try {
      dolphin = await api.optimizeDolphin(screenWidth());
      dirty = false;
      done(t('settings.optimized'));
    } catch (error) {
      app.toast(t('settings.optimizeFailed'), api.errorMessage(error), 'warning');
    } finally {
      saving = false;
    }
  }

  async function resetCategory(category: string) {
    saving = true;
    try {
      dolphin = await api.resetDolphinCategory(category);
      dirty = false;
      done(t('settings.categoryReset', { category: tabLabel }));
    } catch (error) {
      app.toast(t('settings.resetFailed'), api.errorMessage(error), 'warning');
    } finally {
      saving = false;
    }
  }

  async function backupConfig() {
    try {
      await api.backupDolphinConfig();
      done(t('settings.backupDone'));
    } catch (error) {
      app.toast(t('settings.backupFailed'), api.errorMessage(error), 'warning');
    }
  }

  async function removeGameSettings() {
    try {
      const removed = await api.deleteGameSettings();
      done(
        removed.length === 0
          ? t('settings.noGameSettings')
          : t('settings.gameSettingsRemoved', {
              count: removed.length,
              files: removed.join(', ')
            })
      );
    } catch (error) {
      app.toast(t('settings.operationFailed'), api.errorMessage(error), 'warning');
    }
  }

  /** Le azioni rare di una scheda di Dolphin. */
  const dolphinActions = $derived<MenuItem[]>([
    {
      label: t('settings.optimize'),
      icon: 'check',
      hint: t('settings.optimizeHint'),
      disabled: saving,
      onselect: () => void optimize()
    },
    {
      label: t('settings.resetCategory', { category: tabLabel }),
      icon: 'undo',
      hint: t('settings.resetCategoryHint'),
      disabled: saving,
      onselect: () => void resetCategory(tab)
    },
    ...(tab === 'advanced'
      ? [
          {
            label: t('settings.backupConfig'),
            icon: 'save' as const,
            onselect: () => void backupConfig()
          },
          {
            label: t('settings.removeGameSettings'),
            icon: 'trash' as const,
            hint: t('settings.removeGameSettingsHint'),
            onselect: () => void removeGameSettings()
          },
          {
            label: t('settings.openLogs'),
            icon: 'folder' as const,
            onselect: () => void api.openFolder('logs')
          }
        ]
      : [])
  ]);
</script>

<div class="page">
  <div
    class="tabs"
    role="tablist"
    aria-label={t('settings.tabsAria')}
    tabindex="-1"
    onkeydown={onTabKey}
  >
    {#each TABS as item (item.id)}
      <button
        role="tab"
        class:active={tab === item.id}
        aria-selected={tab === item.id}
        tabindex={tab === item.id ? 0 : -1}
        onclick={() => (tab = item.id)}
      >
        {item.label}
      </button>
    {/each}
  </div>

  {#if tab === 'paths'}
    <section class="vk-card">
      <!--
        La lingua sta in cima alla prima scheda: è la scelta che cambia tutto il
        resto di quello che si legge, quindi si trova prima di leggerlo.
      -->
      <SettingRow label={t('settings.language')} hint={t('settings.languageHint')}>
        <div class="segmented" role="radiogroup" aria-label={t('settings.language')}>
          {#each LOCALES as code (code)}
            <button
              role="radio"
              class:active={i18n.locale === code}
              aria-checked={i18n.locale === code}
              lang={code}
              onclick={() => i18n.set(code)}
            >
              {LOCALE_LABELS[code]}
            </button>
          {/each}
        </div>
      </SettingRow>

      <SettingRow label={t('settings.channel')} hint={t('settings.channelHint')}>
        <div class="segmented" role="radiogroup" aria-label={t('settings.channel')}>
          {#each ['Stable', 'Beta'] as const as item (item)}
            <button
              role="radio"
              class:active={channel === item}
              aria-checked={channel === item}
              {@attach tooltip(item === 'Beta' ? t('settings.channelToken') : undefined)}
              onclick={() => switchChannel(item)}
            >
              {item}
            </button>
          {/each}
        </div>
      </SettingRow>
    </section>

    <section class="vk-card">
      <div class="section-head">
        <p class="vk-eyebrow">{t('settings.pathsTitle')}</p>
        <button class="vk-btn" onclick={autoDetect} {@attach tooltip(t('settings.autoDetectHint'))}>
          <Icon name="refresh" size={14} />
          {t('settings.autoDetect')}
        </button>
      </div>

      <div class="paths">
        <PathField
          label={t('settings.dolphinExe')}
          value={settings?.dolphinPath ?? ''}
          valid={settings?.dolphinValid ?? false}
          placeholder={t('settings.notSetM')}
          onbrowse={pickDolphin}
          onsave={(dolphinPath) => savePath({ dolphinPath })}
        />

        <PathField
          label={t('settings.userFolder')}
          value={settings?.userFolderPath ?? ''}
          valid={settings?.userFolderValid ?? false}
          placeholder={t('settings.notSetF')}
          onbrowse={pickUserFolder}
          onsave={(userFolderPath) => savePath({ userFolderPath })}
        />

        <PathField
          label={t('settings.rom')}
          value={settings?.romPath ?? ''}
          valid={settings?.romValid ?? false}
          placeholder={t('settings.notSetF')}
          onbrowse={pickRom}
          onsave={(romPath) => savePath({ romPath })}
        />
      </div>

      {#if (settings?.detectedUserFolders?.length ?? 0) > 1}
        <p class="vk-faint detected">
          {t('settings.foundUserFolders', {
            folders: settings?.detectedUserFolders.join(' · ') ?? ''
          })}
        </p>
      {/if}
      {#if settings?.modFolder}
        <p class="vk-faint detected">
          {t('settings.modInstalledIn', { folder: settings.modFolder })}
        </p>
      {/if}
    </section>

    <section class="vk-card">
      <p class="vk-eyebrow">{t('settings.launchOptions')}</p>

      <SettingRow label={t('settings.separateSave')} hint={t('settings.separateSaveHint')}>
        <Switch
          checked={settings?.separateSavegame ?? true}
          label={t('settings.separateSave')}
          onchange={(next) => updatePreference({ separateSavegame: next })}
        />
      </SettingRow>
      <SettingRow label={t('settings.myStuff')} hint={t('settings.myStuffHint')}>
        <Switch
          checked={settings?.myStuffEnabled ?? true}
          label={t('settings.myStuff')}
          onchange={(next) => updatePreference({ myStuffEnabled: next })}
        />
      </SettingRow>
      <SettingRow label={t('settings.closeDolphin')} hint={t('settings.closeDolphinHint')}>
        <Switch
          checked={settings?.closeRunningDolphin ?? true}
          label={t('settings.closeDolphin')}
          onchange={(next) => updatePreference({ closeRunningDolphin: next })}
        />
      </SettingRow>
      <SettingRow label={t('settings.autoCheck')} hint={t('settings.autoCheckHint')}>
        <Switch
          checked={settings?.autoCheckUpdates ?? true}
          label={t('settings.autoCheck')}
          onchange={(next) => updatePreference({ autoCheckUpdates: next })}
        />
      </SettingRow>
      <SettingRow label={t('settings.concurrency')} hint={t('settings.concurrencyHint')}>
        <Slider
          showLabel={false}
          label={t('settings.concurrency')}
          value={concurrencyDraft ?? settings?.downloadConcurrency ?? 6}
          min={1}
          max={12}
          oninput={(value) => (concurrencyDraft = value)}
          onchange={async (value) => {
            await updatePreference({ downloadConcurrency: value });
            concurrencyDraft = null;
          }}
        />
      </SettingRow>
    </section>
  {:else if tab === 'controller'}
    <section class="vk-card">
      <div class="section-head">
        <div>
          <p class="vk-eyebrow">{t('settings.tab.controller')}</p>
          <p class="vk-subtitle">
            {t('settings.controllerHintBefore')}
            <code>GCPadNew.ini</code>{t('settings.controllerHintAfter')}
          </p>
        </div>
      </div>
      <ControllerPanel />
    </section>
  {:else if tab === 'about'}
    <!--
      La nostra roba: chi c'è dietro VanzaKart e dove trovarci. Sta in coda
      alle impostazioni perché non si tocca niente — si legge e si esce.
    -->
    <section class="vk-card vk-rainbow-top team-hero">
      <img class="team-logo" src={logo} alt={t('home.logoAlt')} />
      <div class="team-intro">
        <p class="vk-eyebrow">{t('team.project')}</p>
        <h2 class="team-title">VanzaKart</h2>
        <p class="vk-subtitle">{t('team.intro')}</p>
        <p class="vk-faint team-version">
          {t('team.versions', {
            launcher: app.status?.launcherVersion ?? t('common.dash'),
            channel: app.modState?.channel ?? 'Stable',
            modpack: app.modState?.installedVersion || t('common.dash')
          })}
        </p>
      </div>
    </section>

    <section class="vk-card">
      <p class="vk-eyebrow">{t('team.whereTitle')}</p>

      <ul class="team-links">
        <li class="team-link">
          <span class="team-icon"><Icon name="external" size={18} /></span>
          <div class="team-text">
            <p class="team-name">{t('team.websiteTitle')}</p>
            <p class="vk-faint">{t('team.websiteBody')}</p>
          </div>
          <button class="vk-btn" onclick={() => openLink(TEAM_LINKS.website)}>
            {t('common.open')}
          </button>
        </li>

        <li class="team-link">
          <span class="team-icon"><Icon name="friends" size={18} /></span>
          <div class="team-text">
            <p class="team-name">{t('team.discordTitle')}</p>
            <p class="vk-faint">{t('team.discordBody')}</p>
          </div>
          <button class="vk-btn" onclick={() => openLink(TEAM_LINKS.discord)}>
            {t('team.discordAction')}
          </button>
        </li>

        <li class="team-link">
          <span class="team-icon"><Icon name="heart" size={18} /></span>
          <div class="team-text">
            <p class="team-name">{t('team.donateTitle')}</p>
            <p class="vk-faint">{t('team.donateBody')}</p>
          </div>
          <button class="vk-btn vk-btn--primary" onclick={() => openLink(TEAM_LINKS.paypal)}>
            <Icon name="heart" size={14} />
            PayPal
          </button>
        </li>
      </ul>
    </section>

    <section class="vk-card">
      <p class="vk-eyebrow">{t('team.thanksTitle')}</p>
      <p class="vk-subtitle thanks">{t('team.thanksBody')}</p>
    </section>
  {:else if !canEditDolphin}
    <div class="vk-card vk-empty">
      <Icon name="folder" size={28} />
      <p>{t('settings.needUserFolder')}</p>
      <button class="vk-btn" onclick={() => (tab = 'paths')}>{t('settings.goToPaths')}</button>
    </div>
  {:else if dolphin && isDolphinTab(tab)}
    <section class="vk-card">
      <div class="section-head">
        <div class="section-title">
          <p class="vk-eyebrow">{tabLabel}</p>
          {#if offCount > 0}
            <span class="off" {@attach tooltip(t('settings.offRecommendedHint'))}>
              <span class="off-dot" aria-hidden="true"></span>
              {offCount === 1
                ? t('settings.offRecommendedOne')
                : t('settings.offRecommended', { count: offCount })}
            </span>
          {/if}
        </div>
        <MenuButton items={dolphinActions} label={t('common.more')} />
      </div>

      {#if tab === 'video'}
        <SettingRow label={t('settings.gfxBackend')} note={note('gfxBackend')}>
          <Select
            block
            label={t('settings.gfxBackend')}
            value={dolphin.gfxBackend}
            options={BACKENDS}
            onchange={(value) => set('gfxBackend', value)}
          />
        </SettingRow>
        <SettingRow
          label={t('settings.internalRes')}
          note={note('internalResolution', RESOLUTIONS)}
        >
          <Select
            block
            label={t('settings.internalRes')}
            value={dolphin.internalResolution}
            options={RESOLUTIONS}
            onchange={(value) => set('internalResolution', value)}
          />
        </SettingRow>
        <SettingRow label={t('settings.aspect')} note={note('aspectRatio', ASPECT_RATIOS)}>
          <Select
            block
            label={t('settings.aspect')}
            value={dolphin.aspectRatio}
            options={ASPECT_RATIOS}
            onchange={(value) => set('aspectRatio', value)}
          />
        </SettingRow>
      {:else if tab === 'audio'}
        <SettingRow label={t('settings.audioBackend')} note={note('audioBackend')}>
          <Select
            block
            label={t('settings.audioBackend')}
            value={dolphin.audioBackend}
            options={AUDIO_BACKENDS}
            onchange={(value) => set('audioBackend', value)}
          />
        </SettingRow>
        <SettingRow label={t('settings.volumeLabel')} note={note('audioVolume', undefined, '%')}>
          <Slider
            showLabel={false}
            label={t('settings.volumeLabel')}
            value={dolphin.audioVolume}
            min={0}
            max={100}
            format={(value) => `${value}%`}
            oninput={(value) => set('audioVolume', value)}
          />
        </SettingRow>
        <SettingRow
          label={t('settings.latencyLabel')}
          note={note('audioLatency', undefined, ' ms')}
        >
          <Slider
            showLabel={false}
            label={t('settings.latencyLabel')}
            value={dolphin.audioLatency}
            min={5}
            max={80}
            format={(value) => `${value} ms`}
            oninput={(value) => set('audioLatency', value)}
          />
        </SettingRow>
      {:else if tab === 'wii'}
        <SettingRow label={t('settings.region')} note={note('wiiRegion', REGIONS)}>
          <Select
            block
            label={t('settings.region')}
            value={dolphin.wiiRegion}
            options={REGIONS}
            onchange={(value) => set('wiiRegion', value)}
          />
        </SettingRow>
        <SettingRow label={t('settings.consoleLanguage')} note={note('wiiLanguage', LANGUAGES)}>
          <Select
            block
            label={t('settings.consoleLanguage')}
            value={dolphin.wiiLanguage}
            options={LANGUAGES}
            onchange={(value) => set('wiiLanguage', value)}
          />
        </SettingRow>
      {:else if tab === 'advanced'}
        <SettingRow label={t('settings.logLevel')} note={note('logLevel')}>
          <Select
            block
            label={t('settings.logLevel')}
            value={dolphin.logLevel}
            options={LOG_LEVELS}
            onchange={(value) => set('logLevel', value)}
          />
        </SettingRow>
      {/if}

      {#each SWITCHES[tab] as item (item.field)}
        <SettingRow label={t(item.label)} note={note(item.field)}>
          <Switch
            checked={dolphin[item.field]}
            label={t(item.label)}
            onchange={(next) => set(item.field, next)}
          />
        </SettingRow>
      {/each}

      {#if tab === 'performance'}
        <SettingRow
          label={t('settings.cpuClockLabel')}
          note={note('cpuClockRatio', undefined, '×')}
        >
          <Slider
            showLabel={false}
            label={t('settings.cpuClockLabel')}
            value={dolphin.cpuClockRatio}
            min={0.5}
            max={3}
            step={0.05}
            disabled={!dolphin.cpuOverride}
            format={(value) => `${value.toFixed(2)}×`}
            oninput={(value) => set('cpuClockRatio', value)}
          />
        </SettingRow>
      {/if}
    </section>

    {#if dirty}
      <div class="save-bar" role="status">
        <span class="save-dot" aria-hidden="true"></span>
        <span>{t('settings.unsaved')}</span>
        <span class="vk-spacer"></span>
        <button class="vk-btn" onclick={discard} disabled={saving}>{t('settings.discard')}</button>
        <button class="vk-btn vk-btn--primary" onclick={saveDolphin} disabled={saving}>
          <Icon name="save" size={14} />
          {saving ? t('common.saving') : t('common.save')}
        </button>
      </div>
    {/if}
  {/if}
</div>

<svelte:window onpaste={onBetaPaste} />

<Modal
  open={betaModalOpen}
  title={t('settings.betaTitle')}
  confirmLabel={t('settings.betaVerify')}
  cancelLabel={t('common.cancel')}
  busy={betaBusy}
  onconfirm={submitBetaToken}
  oncancel={() => {
    betaModalOpen = false;
    betaToken = '';
  }}
>
  <p>{t('settings.betaBody')}</p>
  <div class="beta-row">
    <input
      class="vk-input"
      type="password"
      bind:value={betaToken}
      bind:this={betaInput}
      placeholder={t('settings.betaPlaceholder')}
      autocomplete="off"
      spellcheck="false"
    />
    <button class="vk-btn" type="button" onclick={pasteBetaToken} disabled={betaBusy}>
      <Icon name="copy" size={16} />
      {t('settings.betaPaste')}
    </button>
  </div>
  {#if betaMessage}
    <p class="modal-message">{betaMessage}</p>
  {/if}
</Modal>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 880px;
    margin: 0 auto;
    padding-bottom: 12px;
  }

  /* --- Schede: controllo segmentato, come Mods e Time Trial --- */

  .tabs {
    display: flex;
    flex-wrap: wrap;
    align-self: flex-start;
    gap: 2px;
    padding: 4px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-pill);
    background: var(--vk-input);
    outline: none;
  }

  .tabs button {
    padding: 7px 16px;
    border: 1px solid transparent;
    border-radius: var(--vk-radius-pill);
    background: transparent;
    color: var(--vk-text-secondary);
    font: inherit;
    font-size: var(--vk-fs-small);
    font-weight: 800;
    cursor: pointer;
  }

  .tabs button:hover {
    color: var(--vk-text);
  }

  .tabs button.active {
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    color: var(--vk-text);
  }

  /* --- Scelte brevi (lingua, canale) --- */

  .segmented {
    display: inline-flex;
    padding: 3px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-pill);
    background: var(--vk-input);
  }

  .segmented button {
    padding: 6px 16px;
    border: 1px solid transparent;
    border-radius: var(--vk-radius-pill);
    background: transparent;
    color: var(--vk-text-secondary);
    font: inherit;
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    cursor: pointer;
  }

  .segmented button.active {
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    color: var(--vk-text);
  }

  /* --- Sezioni --- */

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 8px;
  }

  .section-head .vk-eyebrow {
    margin: 0;
  }

  .section-title {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }

  .off {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-micro);
    font-weight: 700;
    cursor: help;
  }

  .off-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--vk-warning);
    box-shadow: 0 0 8px rgb(255 209 102 / 0.6);
  }

  .paths {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-top: 8px;
  }

  .detected {
    margin: 14px 0 0;
    font-size: var(--vk-fs-micro);
    overflow-wrap: anywhere;
  }

  /* --- Barra delle modifiche non salvate --- */

  .save-bar {
    position: sticky;
    bottom: 12px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px 10px 18px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel-glass);
    box-shadow: var(--vk-shadow-modal);
    font-size: var(--vk-fs-small);
    font-weight: 700;
    backdrop-filter: blur(8px);
    animation: rise var(--vk-dur) var(--vk-ease);
  }

  .save-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--vk-warning);
    box-shadow: 0 0 8px rgb(255 209 102 / 0.6);
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .save-bar {
      animation: none;
    }
  }

  /* --- Scheda Team --- */

  .team-hero {
    position: relative;
    display: flex;
    align-items: center;
    gap: 22px;
  }

  .team-logo {
    width: 84px;
    height: 84px;
    flex: none;
    object-fit: contain;
  }

  .team-intro {
    min-width: 0;
  }

  .team-title {
    margin: 2px 0 6px;
    font-size: var(--vk-fs-section);
    font-weight: 900;
    letter-spacing: -0.02em;
  }

  .team-version {
    margin: 10px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .team-links {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 14px 0 0;
    padding: 0;
    list-style: none;
  }

  .team-link {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .team-link:hover {
    border-color: #3a4c74;
  }

  /* Pastiglia dell'icona: un anello arcobaleno, uguale per tutti i link. */
  .team-icon {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    flex: none;
    border: 1.5px solid transparent;
    border-radius: 50%;
    background:
      linear-gradient(var(--vk-panel-soft), var(--vk-panel-soft)) padding-box,
      var(--vk-rainbow-conic) border-box;
    color: var(--vk-text);
  }

  .team-text {
    min-width: 0;
    margin-right: auto;
  }

  .team-name {
    margin: 0;
    font-size: var(--vk-fs-body);
    font-weight: 800;
  }

  .team-text .vk-faint {
    margin: 3px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .thanks {
    margin-top: 10px;
  }

  @media (max-width: 760px) {
    .team-hero {
      flex-direction: column;
      text-align: center;
    }

    .team-link {
      flex-wrap: wrap;
    }
  }

  /* --- Token Beta --- */

  .beta-row {
    display: flex;
    gap: 8px;
  }

  .beta-row .vk-input {
    flex: 1;
    min-width: 0;
  }

  .modal-message {
    margin: 12px 0 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  code {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--vk-input);
    color: var(--vk-cyan-soft);
    font-family: var(--vk-font-mono);
    font-size: 0.92em;
  }
</style>
