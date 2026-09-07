<script lang="ts">
  /**
   * Settings.
   *
   * Ricalca il `SettingsView` del WPF: prima i tre percorsi obbligatori, poi
   * le opzioni di avvio, le impostazioni di Dolphin per categoria e infine il
   * canale di rilascio.
   */
  import { open } from '@tauri-apps/plugin-dialog';

  import * as api from '$lib/api';
  import ControllerPanel from '$lib/components/ControllerPanel.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import PathField from '$lib/components/PathField.svelte';
  import logo from '$lib/assets/logo.png';
  import { app, TEAM_LINKS } from '$lib/stores/app.svelte';
  import { i18n, t, LOCALES, LOCALE_LABELS } from '$lib/stores/i18n.svelte';
  import type { Channel, DolphinSettings } from '$lib/api/types';

  type Tab =
    'paths' | 'video' | 'audio' | 'controller' | 'wii' | 'performance' | 'advanced' | 'about';

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

  const RESOLUTIONS = $derived([
    { value: 0, label: t('settings.res.native') },
    { value: 1, label: '1× (480p)' },
    { value: 2, label: '2× (720p)' },
    { value: 3, label: '3× (1080p)' },
    { value: 4, label: '4× (1440p)' },
    { value: 5, label: '5×' },
    { value: 6, label: '6× (4K)' }
  ]);

  const BACKENDS = ['Vulkan', 'D3D11', 'D3D12', 'OpenGL', 'Null'];
  const AUDIO_BACKENDS = ['Cubeb', 'WASAPI', 'OpenAL', 'XAudio2', 'Null'];
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
  const LOG_LEVELS = ['Notice', 'Error', 'Warning', 'Info', 'Debug'];

  let tab = $state<Tab>('paths');
  let dolphin = $state<DolphinSettings | null>(null);
  let dirty = $state(false);
  let saving = $state(false);
  let notice = $state('');

  const settings = $derived(app.settings);
  const canEditDolphin = $derived(settings?.userFolderValid ?? false);

  $effect(() => {
    if (canEditDolphin && dolphin === null) void loadDolphin();
  });

  async function loadDolphin() {
    try {
      dolphin = await api.getDolphinSettings();
      dirty = false;
    } catch (error) {
      app.toast(t('settings.dolphinSettings'), api.errorMessage(error), 'warning');
    }
  }

  function touch() {
    dirty = true;
    notice = '';
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
      notice = t('settings.pathUpdated');
      return null;
    } catch (error) {
      return api.errorMessage(error);
    }
  }

  async function autoDetect() {
    try {
      app.settings = await api.detectDolphin();
      await app.refresh();
      notice = app.settings.dolphinValid ? t('settings.detected') : t('settings.notDetected');
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

  async function saveDolphin() {
    if (!dolphin) return;
    saving = true;
    try {
      await api.saveDolphinSettings(dolphin);
      dirty = false;
      notice = t('settings.dolphinSaved');
    } catch (error) {
      app.toast(t('settings.saveFailed'), api.errorMessage(error), 'danger');
    } finally {
      saving = false;
    }
  }

  async function optimize() {
    saving = true;
    try {
      dolphin = await api.optimizeDolphin(window.screen.width || 1920);
      dirty = false;
      notice = t('settings.optimized');
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
      notice = t('settings.categoryReset', { category });
    } catch (error) {
      app.toast(t('settings.resetFailed'), api.errorMessage(error), 'warning');
    } finally {
      saving = false;
    }
  }

  async function backupConfig() {
    try {
      await api.backupDolphinConfig();
      notice = t('settings.backupDone');
    } catch (error) {
      app.toast(t('settings.backupFailed'), api.errorMessage(error), 'warning');
    }
  }

  async function removeGameSettings() {
    try {
      const removed = await api.deleteGameSettings();
      notice =
        removed.length === 0
          ? t('settings.noGameSettings')
          : t('settings.gameSettingsRemoved', {
              count: removed.length,
              files: removed.join(', ')
            });
    } catch (error) {
      app.toast(t('settings.operationFailed'), api.errorMessage(error), 'warning');
    }
  }
</script>

<div class="page">
  {#if notice}
    <div class="vk-card notice vk-rainbow-top">{notice}</div>
  {/if}

  <nav class="tabs" aria-label={t('settings.tabsAria')}>
    {#each TABS as item (item.id)}
      <button class="tab" class:active={tab === item.id} onclick={() => (tab = item.id)}>
        {item.label}
      </button>
    {/each}
  </nav>

  {#if tab === 'paths'}
    <!--
      La lingua sta in cima alla prima scheda: è la scelta che cambia tutto il
      resto di quello che si legge, quindi si trova prima di leggerlo.
    -->
    <section class="vk-card">
      <p class="vk-eyebrow">{t('settings.language')}</p>
      <p class="vk-subtitle">{t('settings.languageHint')}</p>

      <div class="channels">
        {#each LOCALES as code (code)}
          <button
            class="channel"
            class:active={i18n.locale === code}
            onclick={() => i18n.set(code)}
            lang={code}
          >
            <span class="channel-name">{LOCALE_LABELS[code]}</span>
            <span class="vk-faint channel-note">
              {code === 'it' ? t('settings.langNote.it') : t('settings.langNote.en')}
            </span>
          </button>
        {/each}
      </div>
    </section>

    <section class="vk-card">
      <div class="section-head">
        <div>
          <p class="vk-eyebrow">{t('settings.pathsTitle')}</p>
          <p class="vk-subtitle">{t('settings.pathsSubtitle')}</p>
        </div>
        <button class="vk-btn" onclick={autoDetect}>
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

      {#if settings?.detectedUserFolders?.length}
        <p class="vk-faint detected">
          {t('settings.foundUserFolders', { folders: settings.detectedUserFolders.join(' · ') })}
        </p>
      {/if}

      <p class="vk-faint detected">
        {t('settings.modInstalledIn', { folder: settings?.modFolder ?? t('common.dash') })}
      </p>
    </section>

    <section class="vk-card">
      <div class="section-head">
        <div>
          <p class="vk-eyebrow">{t('settings.channel')}</p>
          <p class="vk-subtitle">{t('settings.channelHint')}</p>
        </div>
      </div>

      <div class="channels">
        {#each ['Stable', 'Beta'] as const as item (item)}
          <button
            class="channel"
            class:active={channel === item}
            onclick={() => switchChannel(item)}
          >
            <span class="channel-name">{item}</span>
            <span class="vk-faint channel-note">
              {item === 'Stable' ? t('settings.channelRecommended') : t('settings.channelToken')}
            </span>
          </button>
        {/each}
      </div>
    </section>

    <section class="vk-card">
      <p class="vk-eyebrow">{t('settings.launchOptions')}</p>
      <div class="switches">
        <label class="switch">
          <input
            type="checkbox"
            checked={settings?.separateSavegame ?? true}
            onchange={(event) =>
              updatePreference({ separateSavegame: event.currentTarget.checked })}
          />
          <span>
            <strong>{t('settings.separateSave')}</strong>
            <span class="vk-faint">{t('settings.separateSaveHint')}</span>
          </span>
        </label>

        <label class="switch">
          <input
            type="checkbox"
            checked={settings?.myStuffEnabled ?? true}
            onchange={(event) => updatePreference({ myStuffEnabled: event.currentTarget.checked })}
          />
          <span>
            <strong>{t('settings.myStuff')}</strong>
            <span class="vk-faint">{t('settings.myStuffHint')}</span>
          </span>
        </label>

        <label class="switch">
          <input
            type="checkbox"
            checked={settings?.autoCheckUpdates ?? true}
            onchange={(event) =>
              updatePreference({ autoCheckUpdates: event.currentTarget.checked })}
          />
          <span>
            <strong>{t('settings.autoCheck')}</strong>
            <span class="vk-faint">{t('settings.autoCheckHint')}</span>
          </span>
        </label>
      </div>

      <label class="slider-row">
        <span>
          {t('settings.concurrency')}: <strong>{settings?.downloadConcurrency ?? 6}</strong>
        </span>
        <input
          type="range"
          min="1"
          max="12"
          value={settings?.downloadConcurrency ?? 6}
          onchange={(event) =>
            updatePreference({ downloadConcurrency: Number(event.currentTarget.value) })}
        />
      </label>
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
          <span class="team-icon cyan"><Icon name="external" size={18} /></span>
          <div class="team-text">
            <p class="team-name">{t('team.websiteTitle')}</p>
            <p class="vk-faint">{t('team.websiteBody')}</p>
          </div>
          <button class="vk-btn" onclick={() => openLink(TEAM_LINKS.website)}>
            {t('common.open')}
          </button>
        </li>

        <li class="team-link">
          <span class="team-icon violet"><Icon name="friends" size={18} /></span>
          <div class="team-text">
            <p class="team-name">{t('team.discordTitle')}</p>
            <p class="vk-faint">{t('team.discordBody')}</p>
          </div>
          <button class="vk-btn" onclick={() => openLink(TEAM_LINKS.discord)}>
            {t('team.discordAction')}
          </button>
        </li>

        <li class="team-link">
          <span class="team-icon pink"><Icon name="heart" size={18} /></span>
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
    <section class="vk-card">
      <p class="vk-subtitle">{t('settings.needUserFolder')}</p>
    </section>
  {:else if dolphin}
    <section class="vk-card">
      <div class="section-head">
        <div>
          <p class="vk-eyebrow">{TABS.find((item) => item.id === tab)?.label}</p>
          <p class="vk-subtitle">{t('settings.iniHint')}</p>
        </div>
        <div class="vk-row">
          <button class="vk-btn" onclick={() => resetCategory(tab)} disabled={saving}>
            {t('settings.reset')}
          </button>
          <button class="vk-btn vk-btn--primary" onclick={saveDolphin} disabled={saving || !dirty}>
            {saving ? t('common.saving') : t('common.save')}
          </button>
        </div>
      </div>

      <div class="fields">
        {#if tab === 'video'}
          <label class="field">
            <span>{t('settings.gfxBackend')}</span>
            <select class="vk-input" bind:value={dolphin.gfxBackend} onchange={touch}>
              {#each BACKENDS as backend (backend)}<option value={backend}>{backend}</option>{/each}
            </select>
          </label>

          <label class="field">
            <span>{t('settings.internalRes')}</span>
            <select class="vk-input" bind:value={dolphin.internalResolution} onchange={touch}>
              {#each RESOLUTIONS as item (item.value)}
                <option value={item.value}>{item.label}</option>
              {/each}
            </select>
          </label>

          <label class="field">
            <span>{t('settings.aspect')}</span>
            <select class="vk-input" bind:value={dolphin.aspectRatio} onchange={touch}>
              {#each ASPECT_RATIOS as item (item.value)}
                <option value={item.value}>{item.label}</option>
              {/each}
            </select>
          </label>

          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.fullscreen} onchange={touch} />
            {t('settings.fullscreen')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.vsync} onchange={touch} />
            {t('settings.vsync')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.widescreenHack} onchange={touch} />
            {t('settings.widescreenHack')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.removeBlur} onchange={touch} />
            {t('settings.removeBlur')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.showFps} onchange={touch} />
            {t('settings.showFps')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.loadCustomTextures} onchange={touch} />
            {t('settings.customTextures')}</label
          >
        {:else if tab === 'audio'}
          <label class="field">
            <span>{t('settings.audioBackend')}</span>
            <select class="vk-input" bind:value={dolphin.audioBackend} onchange={touch}>
              {#each AUDIO_BACKENDS as backend (backend)}<option value={backend}>{backend}</option
                >{/each}
            </select>
          </label>

          <label class="field">
            <span>{t('settings.volume', { value: dolphin.audioVolume })}</span>
            <input
              type="range"
              min="0"
              max="100"
              bind:value={dolphin.audioVolume}
              onchange={touch}
            />
          </label>

          <label class="field">
            <span>{t('settings.latency', { value: dolphin.audioLatency })}</span>
            <input
              type="range"
              min="5"
              max="80"
              bind:value={dolphin.audioLatency}
              onchange={touch}
            />
          </label>

          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.audioStretching} onchange={touch} />
            {t('settings.audioStretching')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.dspLle} onchange={touch} />
            {t('settings.dspLle')}</label
          >
        {:else if tab === 'wii'}
          <label class="field">
            <span>{t('settings.region')}</span>
            <select class="vk-input" bind:value={dolphin.wiiRegion} onchange={touch}>
              {#each REGIONS as item (item.value)}<option value={item.value}>{item.label}</option
                >{/each}
            </select>
          </label>

          <label class="field">
            <span>{t('settings.consoleLanguage')}</span>
            <select class="vk-input" bind:value={dolphin.wiiLanguage} onchange={touch}>
              {#each LANGUAGES as item (item.value)}<option value={item.value}>{item.label}</option
                >{/each}
            </select>
          </label>

          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.enableRiivolution} onchange={touch} />
            {t('settings.riivolution')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.enableCheats} onchange={touch} />
            {t('settings.cheats')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.enableSdCard} onchange={touch} />
            {t('settings.sdCard')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.forceDisableWiimote} onchange={touch} />
            {t('settings.disableWiimoteSpeaker')}</label
          >
        {:else if tab === 'performance'}
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.dualCore} onchange={touch} />
            {t('settings.dualCore')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.skipIdle} onchange={touch} />
            {t('settings.skipIdle')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.fastDiscSpeed} onchange={touch} />
            {t('settings.fastDisc')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.cpuOverride} onchange={touch} />
            {t('settings.cpuOverride')}</label
          >

          <label class="field">
            <span>{t('settings.cpuClock', { value: dolphin.cpuClockRatio.toFixed(2) })}</span>
            <input
              type="range"
              min="0.5"
              max="3"
              step="0.05"
              bind:value={dolphin.cpuClockRatio}
              onchange={touch}
            />
          </label>

          <button class="vk-btn vk-btn--primary optimize" onclick={optimize} disabled={saving}>
            {t('settings.optimize')}
          </button>
        {:else if tab === 'advanced'}
          <label class="field">
            <span>{t('settings.logLevel')}</span>
            <select class="vk-input" bind:value={dolphin.logLevel} onchange={touch}>
              {#each LOG_LEVELS as level (level)}<option value={level}>{level}</option>{/each}
            </select>
          </label>

          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.logToFile} onchange={touch} />
            {t('settings.logToFile')}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.backendMultithreading} onchange={touch} />
            {t('settings.backendMultithread')}</label
          >
          <label class="check"
            ><input
              type="checkbox"
              bind:checked={dolphin.waitForShadersBeforeStarting}
              onchange={touch}
            />
            {t('settings.waitShaders')}</label
          >

          <div class="tools">
            <button class="vk-btn" onclick={backupConfig}>{t('settings.backupConfig')}</button>
            <button class="vk-btn" onclick={removeGameSettings}>
              {t('settings.removeGameSettings')}
            </button>
            <button class="vk-btn" onclick={() => api.openFolder('logs')}>
              <Icon name="folder" size={14} />
              {t('settings.openLogs')}
            </button>
          </div>
        {/if}
      </div>
    </section>
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

  /* Pastiglia colorata dell'icona: ogni link ha il suo colore, come le card. */
  .team-icon {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    flex: none;
    border-radius: 50%;
    border: 1px solid currentcolor;
  }

  .team-icon.cyan {
    color: var(--vk-cyan-soft);
    background: rgb(0 242 255 / 0.12);
  }

  .team-icon.violet {
    color: #b79bff;
    background: rgb(157 92 255 / 0.14);
  }

  .team-icon.pink {
    color: #ff77a8;
    background: rgb(255 0 102 / 0.14);
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

  .channels {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-top: 14px;
  }

  .channel {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    padding: 12px 16px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .channel:hover {
    border-color: #3a4c74;
  }

  .channel.active {
    border-color: transparent;
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    background-size:
      auto,
      220% 100%;
    animation: vk-rainbow-edge 8s ease-in-out infinite;
    box-shadow:
      0 0 14px rgb(255 0 102 / 0.22),
      0 0 14px rgb(0 242 255 / 0.18);
    color: var(--vk-text);
  }

  .channel-name {
    font-size: var(--vk-fs-body);
    font-weight: 900;
  }

  .channel-note {
    font-size: var(--vk-fs-eyebrow);
  }

  .channel.active .channel-note {
    color: rgb(255 255 255 / 0.85);
  }

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

  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 980px;
    margin: 0 auto;
    padding-bottom: 12px;
  }

  .notice {
    position: relative;
    padding: 12px 16px;
    font-size: var(--vk-fs-small);
    color: var(--vk-success);
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .tab {
    padding: 8px 16px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-pill);
    background: transparent;
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-micro);
    font-weight: 700;
  }

  .tab:hover {
    color: var(--vk-text);
  }

  .tab.active {
    background: var(--vk-tab-active);
    border-color: #3a4c74;
    color: var(--vk-text);
  }

  .section-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  .paths {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .detected {
    margin: 14px 0 0;
    font-size: var(--vk-fs-micro);
    overflow-wrap: anywhere;
  }

  .switches {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-top: 14px;
  }

  .switch {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    cursor: pointer;
  }

  .switch span {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--vk-fs-small);
  }

  .switch .vk-faint {
    font-size: var(--vk-fs-micro);
  }

  .slider-row {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 18px;
    font-size: var(--vk-fs-small);
  }

  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 16px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--vk-fs-small);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--vk-fs-small);
    cursor: pointer;
  }

  .optimize,
  .tools {
    grid-column: 1 / -1;
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  code {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--vk-input);
    color: var(--vk-cyan-soft);
    font-family: var(--vk-font-mono);
    font-size: 0.92em;
  }

  input[type='range'] {
    accent-color: var(--vk-cyan);
  }

  input[type='checkbox'] {
    accent-color: var(--vk-cyan);
    width: 16px;
    height: 16px;
    flex: none;
  }
</style>
