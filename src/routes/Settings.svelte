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
  import { app } from '$lib/stores/app.svelte';
  import type { Channel, DolphinSettings } from '$lib/api/types';

  type Tab = 'paths' | 'video' | 'audio' | 'controller' | 'wii' | 'performance' | 'advanced';

  /** Canale di rilascio: si sceglie qui, non nella pagina Mods (§D-039). */
  let betaModalOpen = $state(false);
  let betaToken = $state('');
  let betaBusy = $state(false);
  let betaMessage = $state('');

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
      app.toast('Cambio canale non riuscito', api.errorMessage(error), 'warning');
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
        app.toast('Canale Beta attivo', status.message, 'success');
      }
    } catch (error) {
      betaMessage = api.errorMessage(error);
    } finally {
      betaBusy = false;
    }
  }

  const TABS: { id: Tab; label: string }[] = [
    { id: 'paths', label: 'Percorsi' },
    { id: 'video', label: 'Video' },
    { id: 'audio', label: 'Audio' },
    { id: 'controller', label: 'Controller' },
    { id: 'wii', label: 'Wii' },
    { id: 'performance', label: 'Prestazioni' },
    { id: 'advanced', label: 'Avanzate' }
  ];

  const RESOLUTIONS = [
    { value: 0, label: 'Nativa (Wii)' },
    { value: 1, label: '1× (480p)' },
    { value: 2, label: '2× (720p)' },
    { value: 3, label: '3× (1080p)' },
    { value: 4, label: '4× (1440p)' },
    { value: 5, label: '5×' },
    { value: 6, label: '6× (4K)' }
  ];

  const BACKENDS = ['Vulkan', 'D3D11', 'D3D12', 'OpenGL', 'Null'];
  const AUDIO_BACKENDS = ['Cubeb', 'WASAPI', 'OpenAL', 'XAudio2', 'Null'];
  const ASPECT_RATIOS = [
    { value: 0, label: 'Auto' },
    { value: 1, label: 'Forza 16:9' },
    { value: 2, label: 'Forza 4:3' },
    { value: 3, label: 'Estendi' }
  ];
  const REGIONS = [
    { value: 0, label: 'NTSC-J' },
    { value: 1, label: 'NTSC-U' },
    { value: 2, label: 'PAL' },
    { value: 3, label: 'NTSC-K' }
  ];
  const LANGUAGES = [
    { value: 0, label: 'Giapponese' },
    { value: 1, label: 'Inglese' },
    { value: 2, label: 'Tedesco' },
    { value: 3, label: 'Francese' },
    { value: 4, label: 'Spagnolo' },
    { value: 5, label: 'Italiano' },
    { value: 6, label: 'Olandese' }
  ];
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
      app.toast('Impostazioni Dolphin', api.errorMessage(error), 'warning');
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
      title: 'Seleziona l’eseguibile di Dolphin',
      filters: [{ name: 'Dolphin', extensions: ['exe', 'app', 'AppImage', '*'] }]
    });
    if (typeof selected === 'string') await applyPath({ dolphinPath: selected });
  }

  async function pickRom() {
    const selected = await open({
      multiple: false,
      directory: false,
      title: 'Seleziona la ROM di Mario Kart Wii',
      filters: [
        { name: 'Immagini disco Wii', extensions: ['wbfs', 'iso', 'rvz', 'ciso', 'gcm', 'wia'] }
      ]
    });
    if (typeof selected === 'string') await applyPath({ romPath: selected });
  }

  async function pickUserFolder() {
    const selected = await open({
      multiple: false,
      directory: true,
      title: 'Seleziona la cartella User di Dolphin'
    });
    if (typeof selected === 'string') await applyPath({ userFolderPath: selected });
  }

  /** Come `savePath`, ma per il selettore: lì l'errore va nell'avviso. */
  async function applyPath(paths: Parameters<typeof api.updatePaths>[0]) {
    const failure = await savePath(paths);
    if (failure) app.toast('Percorso non valido', failure, 'warning');
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
      notice = 'Percorso aggiornato.';
      return null;
    } catch (error) {
      return api.errorMessage(error);
    }
  }

  async function autoDetect() {
    try {
      app.settings = await api.detectDolphin();
      await app.refresh();
      notice = app.settings.dolphinValid
        ? 'Dolphin rilevato automaticamente.'
        : 'Nessuna installazione di Dolphin trovata: selezionala a mano.';
    } catch (error) {
      app.toast('Rilevamento non riuscito', api.errorMessage(error), 'warning');
    }
  }

  async function updatePreference(patch: Parameters<typeof api.updatePreferences>[0]) {
    try {
      app.settings = await api.updatePreferences(patch);
    } catch (error) {
      app.toast('Preferenza non salvata', api.errorMessage(error), 'warning');
    }
  }

  async function saveDolphin() {
    if (!dolphin) return;
    saving = true;
    try {
      await api.saveDolphinSettings(dolphin);
      dirty = false;
      notice = 'Impostazioni di Dolphin salvate.';
    } catch (error) {
      app.toast('Salvataggio non riuscito', api.errorMessage(error), 'danger');
    } finally {
      saving = false;
    }
  }

  async function optimize() {
    saving = true;
    try {
      dolphin = await api.optimizeDolphin(window.screen.width || 1920);
      dirty = false;
      notice = 'Preset "VanzaKart Recommended" applicato e salvato.';
    } catch (error) {
      app.toast('Ottimizzazione non riuscita', api.errorMessage(error), 'warning');
    } finally {
      saving = false;
    }
  }

  async function resetCategory(category: string) {
    saving = true;
    try {
      dolphin = await api.resetDolphinCategory(category);
      dirty = false;
      notice = `Categoria ${category} riportata ai valori predefiniti.`;
    } catch (error) {
      app.toast('Reset non riuscito', api.errorMessage(error), 'warning');
    } finally {
      saving = false;
    }
  }

  async function backupConfig() {
    try {
      await api.backupDolphinConfig();
      notice = 'Backup della configurazione creato nella cartella Backup.';
    } catch (error) {
      app.toast('Backup non riuscito', api.errorMessage(error), 'warning');
    }
  }

  async function removeGameSettings() {
    try {
      const removed = await api.deleteGameSettings();
      notice =
        removed.length === 0
          ? 'Nessun file GameSettings di Mario Kart trovato.'
          : `Rimossi ${removed.length} file: ${removed.join(', ')}`;
    } catch (error) {
      app.toast('Operazione non riuscita', api.errorMessage(error), 'warning');
    }
  }
</script>

<div class="page">
  {#if notice}
    <div class="vk-card notice vk-rainbow-top">{notice}</div>
  {/if}

  <nav class="tabs" aria-label="Categorie impostazioni">
    {#each TABS as item (item.id)}
      <button class="tab" class:active={tab === item.id} onclick={() => (tab = item.id)}>
        {item.label}
      </button>
    {/each}
  </nav>

  {#if tab === 'paths'}
    <section class="vk-card">
      <div class="section-head">
        <div>
          <p class="vk-eyebrow">Percorsi obbligatori</p>
          <p class="vk-subtitle">Servono tutti e tre per poter avviare il gioco.</p>
        </div>
        <button class="vk-btn" onclick={autoDetect}>
          <Icon name="refresh" size={14} />
          Rileva automaticamente
        </button>
      </div>

      <div class="paths">
        <PathField
          label="Eseguibile di Dolphin"
          value={settings?.dolphinPath ?? ''}
          valid={settings?.dolphinValid ?? false}
          placeholder="Non configurato"
          onbrowse={pickDolphin}
          onsave={(dolphinPath) => savePath({ dolphinPath })}
        />

        <PathField
          label="Cartella User di Dolphin"
          value={settings?.userFolderPath ?? ''}
          valid={settings?.userFolderValid ?? false}
          placeholder="Non configurata"
          onbrowse={pickUserFolder}
          onsave={(userFolderPath) => savePath({ userFolderPath })}
        />

        <PathField
          label="ROM di Mario Kart Wii"
          value={settings?.romPath ?? ''}
          valid={settings?.romValid ?? false}
          placeholder="Non configurata"
          onbrowse={pickRom}
          onsave={(romPath) => savePath({ romPath })}
        />
      </div>

      {#if settings?.detectedUserFolders?.length}
        <p class="vk-faint detected">
          Cartelle User trovate: {settings.detectedUserFolders.join(' · ')}
        </p>
      {/if}

      <p class="vk-faint detected">Modpack installata in: {settings?.modFolder ?? '—'}</p>
    </section>

    <section class="vk-card">
      <div class="section-head">
        <div>
          <p class="vk-eyebrow">Canale di rilascio</p>
          <p class="vk-subtitle">
            Stable e Beta si installano in cartelle diverse: cambiare canale non tocca l'altra
            installazione né i tuoi dati.
          </p>
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
              {item === 'Stable' ? 'Consigliato' : 'Richiede un token'}
            </span>
          </button>
        {/each}
      </div>
    </section>

    <section class="vk-card">
      <p class="vk-eyebrow">Opzioni di avvio</p>
      <div class="switches">
        <label class="switch">
          <input
            type="checkbox"
            checked={settings?.separateSavegame ?? true}
            onchange={(event) =>
              updatePreference({ separateSavegame: event.currentTarget.checked })}
          />
          <span>
            <strong>Salvataggio separato</strong>
            <span class="vk-faint"
              >La modpack usa un salvataggio proprio, distinto dal gioco base.</span
            >
          </span>
        </label>

        <label class="switch">
          <input
            type="checkbox"
            checked={settings?.myStuffEnabled ?? true}
            onchange={(event) => updatePreference({ myStuffEnabled: event.currentTarget.checked })}
          />
          <span>
            <strong>Abilita "My Stuff"</strong>
            <span class="vk-faint">Carica texture e addon personali dalla cartella My Stuff.</span>
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
            <strong>Controlla aggiornamenti all'avvio</strong>
            <span class="vk-faint">Interroga il server all'apertura del launcher.</span>
          </span>
        </label>
      </div>

      <label class="slider-row">
        <span>Download in parallelo: <strong>{settings?.downloadConcurrency ?? 6}</strong></span>
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
          <p class="vk-eyebrow">Controller</p>
          <p class="vk-subtitle">
            I binding finiscono in <code>GCPadNew.ini</code>; le chiavi che il launcher non gestisce
            restano intatte.
          </p>
        </div>
      </div>
      <ControllerPanel />
    </section>
  {:else if !canEditDolphin}
    <section class="vk-card">
      <p class="vk-subtitle">
        Seleziona prima la cartella User di Dolphin nella scheda Percorsi: le impostazioni
        dell'emulatore vivono nei suoi file INI.
      </p>
    </section>
  {:else if dolphin}
    <section class="vk-card">
      <div class="section-head">
        <div>
          <p class="vk-eyebrow">{TABS.find((item) => item.id === tab)?.label}</p>
          <p class="vk-subtitle">
            Le modifiche vengono scritte negli INI di Dolphin senza toccare le chiavi che il
            launcher non gestisce.
          </p>
        </div>
        <div class="vk-row">
          <button class="vk-btn" onclick={() => resetCategory(tab)} disabled={saving}
            >Ripristina</button
          >
          <button class="vk-btn vk-btn--primary" onclick={saveDolphin} disabled={saving || !dirty}>
            {saving ? 'Salvataggio…' : 'Salva'}
          </button>
        </div>
      </div>

      <div class="fields">
        {#if tab === 'video'}
          <label class="field">
            <span>Backend grafico</span>
            <select class="vk-input" bind:value={dolphin.gfxBackend} onchange={touch}>
              {#each BACKENDS as backend (backend)}<option value={backend}>{backend}</option>{/each}
            </select>
          </label>

          <label class="field">
            <span>Risoluzione interna</span>
            <select class="vk-input" bind:value={dolphin.internalResolution} onchange={touch}>
              {#each RESOLUTIONS as item (item.value)}
                <option value={item.value}>{item.label}</option>
              {/each}
            </select>
          </label>

          <label class="field">
            <span>Proporzioni</span>
            <select class="vk-input" bind:value={dolphin.aspectRatio} onchange={touch}>
              {#each ASPECT_RATIOS as item (item.value)}
                <option value={item.value}>{item.label}</option>
              {/each}
            </select>
          </label>

          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.fullscreen} onchange={touch} /> Schermo intero</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.vsync} onchange={touch} /> V-Sync</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.widescreenHack} onchange={touch} /> Widescreen
            hack</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.removeBlur} onchange={touch} /> Rimuovi sfocatura</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.showFps} onchange={touch} /> Mostra FPS</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.loadCustomTextures} onchange={touch} /> Texture
            personalizzate</label
          >
        {:else if tab === 'audio'}
          <label class="field">
            <span>Backend audio</span>
            <select class="vk-input" bind:value={dolphin.audioBackend} onchange={touch}>
              {#each AUDIO_BACKENDS as backend (backend)}<option value={backend}>{backend}</option
                >{/each}
            </select>
          </label>

          <label class="field">
            <span>Volume: {dolphin.audioVolume}%</span>
            <input
              type="range"
              min="0"
              max="100"
              bind:value={dolphin.audioVolume}
              onchange={touch}
            />
          </label>

          <label class="field">
            <span>Latenza: {dolphin.audioLatency} ms</span>
            <input
              type="range"
              min="5"
              max="80"
              bind:value={dolphin.audioLatency}
              onchange={touch}
            />
          </label>

          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.audioStretching} onchange={touch} /> Audio stretching</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.dspLle} onchange={touch} /> DSP LLE (più accurato,
            più lento)</label
          >
        {:else if tab === 'wii'}
          <label class="field">
            <span>Regione</span>
            <select class="vk-input" bind:value={dolphin.wiiRegion} onchange={touch}>
              {#each REGIONS as item (item.value)}<option value={item.value}>{item.label}</option
                >{/each}
            </select>
          </label>

          <label class="field">
            <span>Lingua della console</span>
            <select class="vk-input" bind:value={dolphin.wiiLanguage} onchange={touch}>
              {#each LANGUAGES as item (item.value)}<option value={item.value}>{item.label}</option
                >{/each}
            </select>
          </label>

          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.enableRiivolution} onchange={touch} /> Abilita
            Riivolution</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.enableCheats} onchange={touch} /> Abilita cheat</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.enableSdCard} onchange={touch} /> Scheda SD</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.forceDisableWiimote} onchange={touch} /> Disattiva
            speaker Wiimote</label
          >
        {:else if tab === 'performance'}
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.dualCore} onchange={touch} /> Dual core</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.skipIdle} onchange={touch} /> Salta cicli inattivi</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.fastDiscSpeed} onchange={touch} /> Lettura disco
            veloce</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.cpuOverride} onchange={touch} /> Override clock
            CPU</label
          >

          <label class="field">
            <span>Clock CPU: {dolphin.cpuClockRatio.toFixed(2)}×</span>
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
            Ottimizza per VanzaKart
          </button>
        {:else if tab === 'advanced'}
          <label class="field">
            <span>Livello di log</span>
            <select class="vk-input" bind:value={dolphin.logLevel} onchange={touch}>
              {#each LOG_LEVELS as level (level)}<option value={level}>{level}</option>{/each}
            </select>
          </label>

          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.logToFile} onchange={touch} /> Scrivi il log
            su file</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={dolphin.backendMultithreading} onchange={touch} /> Backend
            multithread</label
          >
          <label class="check"
            ><input
              type="checkbox"
              bind:checked={dolphin.waitForShadersBeforeStarting}
              onchange={touch}
            /> Attendi la compilazione degli shader</label
          >

          <div class="tools">
            <button class="vk-btn" onclick={backupConfig}>Backup configurazione</button>
            <button class="vk-btn" onclick={removeGameSettings}>Rimuovi GameSettings RMC*</button>
            <button class="vk-btn" onclick={() => api.openFolder('logs')}>
              <Icon name="folder" size={14} />
              Apri cartella log
            </button>
          </div>
        {/if}
      </div>
    </section>
  {/if}
</div>

<Modal
  open={betaModalOpen}
  title="Token di accesso Beta"
  confirmLabel="Verifica"
  cancelLabel="Annulla"
  busy={betaBusy}
  onconfirm={submitBetaToken}
  oncancel={() => {
    betaModalOpen = false;
    betaToken = '';
  }}
>
  <p>Il canale Beta richiede un token fornito dallo staff VanzaKart.</p>
  <input
    class="vk-input"
    type="password"
    bind:value={betaToken}
    placeholder="Incolla qui il token"
    autocomplete="off"
  />
  {#if betaMessage}
    <p class="modal-message">{betaMessage}</p>
  {/if}
</Modal>

<style>
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
