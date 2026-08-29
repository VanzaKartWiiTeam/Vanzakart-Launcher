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
  import { t, type TranslationKey } from '$lib/stores/i18n.svelte';
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

  /**
   * Le descrizioni delle azioni arrivano dal backend, che le ha in italiano.
   * Qui si traducono per id: se un giorno ne arriva una nuova, si mostra
   * comunque quella del backend invece di una casella vuota.
   */
  const ACTION_KEYS: Record<string, TranslationKey> = {
    drive: 'action.drive',
    brake: 'action.brake',
    drift: 'action.drift',
    item: 'action.item',
    look_back: 'action.look_back',
    pause: 'action.pause',
    steering: 'action.steering',
    trick_up: 'action.trick_up',
    trick_down: 'action.trick_down',
    trick_left: 'action.trick_left',
    trick_right: 'action.trick_right'
  };

  function describe(action: MarioKartAction): string {
    const key = ACTION_KEYS[action.id];
    return key ? t(key) : action.description;
  }
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
      app.toast(t('controller.unreadable'), api.errorMessage(error), 'warning');
    } finally {
      loading = false;
    }
  }

  /** Valore assegnato a un'azione: la prima chiave non vuota. */
  function bindingFor(action: MarioKartAction): string {
    if (!profile) return t('controller.unassigned');
    for (const key of action.dolphin_keys) {
      const value = profile.bindings[key];
      if (value && value.trim() !== '') return friendly(value);
    }
    return t('controller.unassigned');
  }

  /** Toglie i backtick della sintassi Dolphin e il moltiplicatore. */
  function friendly(raw: string): string {
    const unwrapped = raw.replace(/^\((.*) \* [\d.]+\)$/, '$1');
    return unwrapped.replace(/`/g, '').trim() || t('controller.unassigned');
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
    notice = t('controller.selected', { name: device.name });
  }

  async function capture(action: MarioKartAction) {
    if (!profile || listeningFor) return;
    listeningFor = action.id;
    notice = t('controller.pressInputFor', { action: action.title });

    try {
      const binding = await api.captureBinding(profile.device.dolphinDevice);
      if (binding === null) {
        notice = t('controller.noInput');
        return;
      }

      const bindings = { ...profile.bindings };
      for (const key of action.dolphin_keys) bindings[key] = binding;
      profile = { ...profile, bindings };
      notice = t('controller.assigned', { action: action.title, binding: friendly(binding) });
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
      notice = supported ? t('controller.rumbleSent') : t('controller.noRumble');
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
      notice = t('controller.saved');
    } catch (error) {
      app.toast(t('settings.saveFailed'), api.errorMessage(error), 'warning');
    } finally {
      saving = false;
    }
  }

  async function switchMode(next: ControllerMode) {
    try {
      mode = await api.setControllerMode(next);
      notice =
        next === 'configure-with-dolphin'
          ? t('controller.modeDolphin')
          : t('controller.modeLauncher');
    } catch (error) {
      app.toast(t('controller.modeFailed'), api.errorMessage(error), 'warning');
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
      <strong>{t('controller.fromLauncher')}</strong>
      <span class="vk-faint">{t('controller.fromLauncherHint')}</span>
    </button>
    <button
      class="mode"
      class:active={!launcherMode}
      onclick={() => switchMode('configure-with-dolphin')}
    >
      <strong>{t('controller.fromDolphin')}</strong>
      <span class="vk-faint">{t('controller.fromDolphinHint')}</span>
    </button>
  </div>

  {#if !launcherMode}
    <p class="vk-subtitle">
      {t('controller.dolphinModeBody')}
      <code>GCPadNew.ini</code>.
    </p>
  {:else if loading}
    <div class="vk-skeleton skeleton"></div>
  {:else if !profile}
    <p class="vk-subtitle">{t('controller.needUserFolder')}</p>
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
            {device.connected ? device.kind : t('controller.notConnected')}
            {device.supportsRumble ? t('controller.rumbleTag') : ''}
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
                <span class="vk-faint">{describe(action)}</span>
              </div>
              <button
                class="assign"
                class:listening={listeningFor === action.id}
                onclick={() => capture(action)}
                disabled={listeningFor !== null || !profile.device.connected}
              >
                {listeningFor === action.id ? t('controller.pressInput') : bindingFor(action)}
              </button>
            </div>
          {/each}
        </div>
      </section>
    {/each}

    <!-- PARAMETRI ANALOGICI -->
    <section class="section">
      <p class="vk-eyebrow">{t('controller.steering')}</p>
      <div class="sliders">
        <label>
          <span>{t('controller.deadzone')}: <strong>{profile.deadzone.toFixed(0)}%</strong></span>
          <input type="range" min="0" max="50" bind:value={profile.deadzone} />
        </label>
        <label>
          <span>
            {t('controller.sensitivity')}: <strong>{profile.sensitivity.toFixed(0)}%</strong>
          </span>
          <input type="range" min="50" max="150" bind:value={profile.sensitivity} />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={profile.vibration} />
          {t('controller.vibration')}
        </label>
      </div>
    </section>

    <div class="actions-row">
      <button class="vk-btn" onclick={load} disabled={saving}>
        <Icon name="refresh" size={14} />
        {t('controller.reload')}
      </button>
      <button
        class="vk-btn"
        onclick={testRumble}
        disabled={!profile.device.supportsRumble || !profile.device.connected}
      >
        {t('controller.testRumble')}
      </button>
      <span class="vk-spacer"></span>
      <button
        class="vk-btn vk-btn--primary"
        onclick={save}
        disabled={saving || conflicts.length > 0 || !profile.device.connected}
      >
        {saving ? t('common.saving') : t('controller.saveBindings')}
      </button>
    </div>

    {#if conflicts.length > 0}
      <p class="conflict-note">{t('controller.conflictNote')}</p>
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
