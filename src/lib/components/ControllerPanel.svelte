<script lang="ts">
  /**
   * Pannello controller.
   *
   * Ricalca `Views/MarioKartControllerPanel.xaml`: scelta del device, matrice
   * delle azioni raggruppate per sezione (RACING / MOVEMENT), acquisizione
   * "premi un input", deadzone, sensibilità e vibrazione.
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import { app } from '$lib/stores/app.svelte';
  import type {
    ControllerMode,
    ControllerProfile,
    ControllerView,
    MarioKartAction
  } from '$lib/api/types';

  let devices = $state<ControllerView[]>([]);
  let actions = $state<MarioKartAction[]>([]);
  let profile = $state<ControllerProfile | null>(null);
  let mode = $state<ControllerMode>('launcher-configuration');
  let loading = $state(true);
  let saving = $state(false);
  let listeningFor = $state<string | null>(null);
  let notice = $state('');

  const sections = $derived([...new Set(actions.map((action) => action.section))]);
  const launcherMode = $derived(mode === 'launcher-configuration');

  $effect(() => {
    void load();
  });

  async function load() {
    loading = true;
    try {
      [devices, actions, mode] = await Promise.all([
        api.scanControllers(),
        api.getControllerActions(),
        api.getControllerMode()
      ]);
      profile = await api.getControllerProfile();
    } catch (error) {
      app.toast('Controller non leggibili', api.errorMessage(error), 'warning');
    } finally {
      loading = false;
    }
  }

  /** Valore assegnato a un'azione: la prima chiave non vuota. */
  function bindingFor(action: MarioKartAction): string {
    if (!profile) return 'Non assegnato';
    for (const key of action.dolphin_keys) {
      const value = profile.bindings[key];
      if (value && value.trim() !== '') return friendly(value);
    }
    return 'Non assegnato';
  }

  /** Toglie i backtick della sintassi Dolphin e il moltiplicatore. */
  function friendly(raw: string): string {
    const unwrapped = raw.replace(/^\((.*) \* [\d.]+\)$/, '$1');
    return unwrapped.replace(/`/g, '').trim() || 'Non assegnato';
  }

  /**
   * Azioni che condividono lo stesso input, escluso brake+drift.
   *
   * Stessa regola del backend (`ControllerProfile::conflicts`): la UI la
   * ripete solo per evidenziare le righe prima del salvataggio.
   */
  const conflicts = $derived.by((): string[] => {
    if (!profile) return [];

    const assigned = actions
      .filter((action) => action.kind !== 'steering')
      .map((action) => ({
        id: action.id,
        value: action.dolphin_keys
          .map((key) => profile?.bindings[key])
          .find((item) => item && item.trim() !== '')
      }))
      .filter((entry): entry is { id: string; value: string } => entry.value !== undefined);

    const flagged: string[] = [];
    for (const entry of assigned) {
      const sharing = assigned.filter((other) => other.value === entry.value).map((o) => o.id);
      if (sharing.length < 2) continue;

      const isBrakeDrift =
        sharing.length === 2 && sharing.includes('brake') && sharing.includes('drift');
      if (!isBrakeDrift && !flagged.includes(entry.id)) flagged.push(entry.id);
    }
    return flagged;
  });

  async function selectDevice(device: ControllerView) {
    if (!profile) return;
    profile = {
      ...profile,
      device: {
        dolphinDevice: device.dolphinDevice,
        displayName: device.name,
        kind: device.kind,
        connected: device.connected,
        xinputSlot: -1,
        supportsRumble: device.supportsRumble
      },
      configuredDolphinDevice: device.dolphinDevice
    };
    notice = `Controller selezionato: ${device.name}. Salva per applicare.`;
  }

  async function capture(action: MarioKartAction) {
    if (!profile || listeningFor) return;
    listeningFor = action.id;
    notice = `Premi l’input da assegnare a "${action.title}"…`;

    try {
      const binding = await api.captureBinding(profile.device.dolphinDevice);
      if (binding === null) {
        notice = 'Nessun input rilevato: riprova.';
        return;
      }

      const bindings = { ...profile.bindings };
      for (const key of action.dolphin_keys) bindings[key] = binding;
      profile = { ...profile, bindings };
      notice = `"${action.title}" assegnato a ${friendly(binding)}.`;
    } catch (error) {
      notice = api.errorMessage(error);
    } finally {
      listeningFor = null;
    }
  }

  async function testRumble() {
    if (!profile) return;
    try {
      const supported = await api.rumbleController(profile.device.dolphinDevice);
      notice = supported
        ? 'Vibrazione inviata al controller.'
        : 'Questo controller non supporta la vibrazione.';
    } catch (error) {
      notice = api.errorMessage(error);
    }
  }

  async function save() {
    if (!profile) return;
    saving = true;
    try {
      await api.saveControllerProfile(profile);
      mode = await api.getControllerMode();
      notice = 'Binding salvati in GCPadNew.ini.';
    } catch (error) {
      app.toast('Salvataggio non riuscito', api.errorMessage(error), 'warning');
    } finally {
      saving = false;
    }
  }

  async function switchMode(next: ControllerMode) {
    try {
      mode = await api.setControllerMode(next);
      notice =
        next === 'configure-with-dolphin'
          ? 'Dolphin gestisce i controller: il launcher non tocca più i suoi file.'
          : 'Il launcher gestisce i controller.';
    } catch (error) {
      app.toast('Cambio modalità non riuscito', api.errorMessage(error), 'warning');
    }
  }
</script>

<div class="panel">
  {#if notice}
    <p class="notice">{notice}</p>
  {/if}

  <!-- MODALITÀ -->
  <div class="modes">
    <button
      class="mode"
      class:active={launcherMode}
      onclick={() => switchMode('launcher-configuration')}
    >
      <strong>Configura dal launcher</strong>
      <span class="vk-faint">Il launcher scrive i binding e attiva il pad GameCube.</span>
    </button>
    <button
      class="mode"
      class:active={!launcherMode}
      onclick={() => switchMode('configure-with-dolphin')}
    >
      <strong>Configura da Dolphin</strong>
      <span class="vk-faint">Dolphin resta l’unico proprietario dei suoi file.</span>
    </button>
  </div>

  {#if !launcherMode}
    <p class="vk-subtitle">
      In questa modalità i binding si impostano dentro Dolphin. Il launcher non modifica
      <code>GCPadNew.ini</code>.
    </p>
  {:else if loading}
    <div class="vk-skeleton skeleton"></div>
  {:else if !profile}
    <p class="vk-subtitle">Seleziona la cartella User di Dolphin per configurare i controller.</p>
  {:else}
    <!-- DEVICE -->
    <div class="devices">
      {#each devices as device (device.id)}
        <button
          class="device"
          class:active={device.dolphinDevice === profile.device.dolphinDevice}
          class:offline={!device.connected}
          onclick={() => selectDevice(device)}
        >
          <span class="device-name">{device.name}</span>
          <span class="vk-faint device-meta">
            {device.connected ? device.kind : 'non connesso'}
            {device.supportsRumble ? ' · vibrazione' : ''}
          </span>
        </button>
      {/each}
    </div>

    <!-- AZIONI -->
    {#each sections as section (section)}
      <section class="section">
        <p class="vk-eyebrow">{section}</p>
        <div class="bindings">
          {#each actions.filter((action) => action.section === section) as action (action.id)}
            <div class="binding" class:conflict={conflicts.includes(action.id)}>
              <span class="glyph" aria-hidden="true">{action.icon}</span>
              <div class="labels">
                <strong>{action.title}</strong>
                <span class="vk-faint">{action.description}</span>
              </div>
              <button
                class="assign"
                class:listening={listeningFor === action.id}
                onclick={() => capture(action)}
                disabled={listeningFor !== null || !profile.device.connected}
              >
                {listeningFor === action.id ? 'Premi un input…' : bindingFor(action)}
              </button>
            </div>
          {/each}
        </div>
      </section>
    {/each}

    <!-- PARAMETRI ANALOGICI -->
    <section class="section">
      <p class="vk-eyebrow">Sterzo</p>
      <div class="sliders">
        <label>
          <span>Zona morta: <strong>{profile.deadzone.toFixed(0)}%</strong></span>
          <input type="range" min="0" max="50" bind:value={profile.deadzone} />
        </label>
        <label>
          <span>Sensibilità: <strong>{profile.sensitivity.toFixed(0)}%</strong></span>
          <input type="range" min="50" max="150" bind:value={profile.sensitivity} />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={profile.vibration} />
          Vibrazione
        </label>
      </div>
    </section>

    <div class="actions-row">
      <button class="vk-btn" onclick={load} disabled={saving}>
        <Icon name="refresh" size={14} />
        Rileggi
      </button>
      <button
        class="vk-btn"
        onclick={testRumble}
        disabled={!profile.device.supportsRumble || !profile.device.connected}
      >
        Prova vibrazione
      </button>
      <span class="vk-spacer"></span>
      <button
        class="vk-btn vk-btn--primary"
        onclick={save}
        disabled={saving || conflicts.length > 0 || !profile.device.connected}
      >
        {saving ? 'Salvataggio…' : 'Salva binding'}
      </button>
    </div>

    {#if conflicts.length > 0}
      <p class="conflict-note">
        Alcune azioni condividono lo stesso input. Solo Brake e Drift possono farlo.
      </p>
    {/if}
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .notice {
    margin: 0;
    padding: 10px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-input);
    background: var(--vk-panel-soft);
    font-size: var(--vk-fs-small);
    color: var(--vk-cyan-soft);
  }

  .modes,
  .devices {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 12px;
  }

  .mode,
  .device {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 14px 16px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-input);
    background: var(--vk-panel-soft);
    text-align: left;
    font-size: var(--vk-fs-small);
  }

  .mode.active,
  .device.active {
    border-color: transparent;
    background:
      linear-gradient(var(--vk-panel-soft), var(--vk-panel-soft)) padding-box,
      var(--vk-rainbow) border-box;
    border: 1px solid transparent;
  }

  .device.offline {
    opacity: 0.6;
  }

  .device-name {
    font-weight: 800;
  }

  .device-meta,
  .mode span {
    font-size: var(--vk-fs-micro);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .bindings {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 10px;
  }

  .binding {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-input);
    background: var(--vk-panel-soft);
  }

  .binding.conflict {
    border-color: rgb(255 209 102 / 0.6);
    background: rgb(255 209 102 / 0.08);
  }

  .glyph {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: var(--vk-radius-badge);
    background: var(--vk-input);
    font-size: 15px;
  }

  .labels {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    font-size: var(--vk-fs-small);
  }

  .labels .vk-faint {
    font-size: var(--vk-fs-eyebrow);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .assign {
    min-width: 140px;
    padding: 7px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-input);
    color: var(--vk-text);
    font-size: var(--vk-fs-micro);
    font-weight: 700;
    text-align: center;
  }

  .assign:hover:not(:disabled) {
    border-color: var(--vk-cyan);
  }

  .assign.listening {
    border-color: transparent;
    background:
      linear-gradient(var(--vk-input), var(--vk-input)) padding-box,
      var(--vk-rainbow) border-box;
    border: 1px solid transparent;
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.6;
    }
  }

  .sliders {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
    align-items: end;
  }

  .sliders label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--vk-fs-small);
  }

  .check {
    flex-direction: row !important;
    align-items: center;
    gap: 10px;
  }

  input[type='range'] {
    accent-color: var(--vk-cyan);
  }

  input[type='checkbox'] {
    accent-color: var(--vk-cyan);
    width: 16px;
    height: 16px;
  }

  .actions-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .conflict-note {
    margin: 0;
    color: var(--vk-warning);
    font-size: var(--vk-fs-micro);
  }

  .skeleton {
    height: 180px;
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
