<script lang="ts">
  /**
   * Editor Mii.
   *
   * Porta `Launcher/MiiEditorWindow.xaml(.cs)`, come modale a tutta pagina
   * invece che come finestra separata (`docs/decisions.md` §U-05). Ciò che si
   * modifica è il Mii dentro `RFL_DB.dat`: salvare scrive nel database di
   * Dolphin (§D-037).
   *
   * L'editor è nativo (§D-092):
   *
   * - l'anteprima la disegna il **renderer nativo** del launcher: ogni
   *   modifica si vede subito, senza rete, e il Mii si gira trascinandolo.
   *   Senza runtime installato resta il render di Mii Studio, più lento, e
   *   l'editor propone di installarlo;
   * - le scelte di ogni tratto sono **icone**, tutte in una griglia, invece
   *   di miniature renderizzate sei per pagina;
   * - ogni cursore prende i limiti dal backend, gli stessi che il gioco
   *   accetta: nessun controllo può produrre un Mii che il gioco rifiuta.
   */
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';

  import * as api from '$lib/api';
  import {
    CATEGORIES,
    FACIAL_FEATURES,
    NAME_SYMBOLS,
    clampState,
    rangeOf,
    sliderPosition,
    sliderValue,
    toLimits
  } from '$lib/mii/categories';
  import type { Control, Limits } from '$lib/mii/categories';
  import { PALETTES, iconColors, loadIcons } from '$lib/mii/icons';
  import type { IconSet, PaletteName, PartIconKind } from '$lib/mii/icons';
  import { appearanceKey, forget as forgetRenders, renderSize, renderState } from '$lib/mii/render';
  import Icon from '$lib/components/Icon.svelte';
  import MiiPartIcon from '$lib/components/MiiPartIcon.svelte';
  import miiSilhouette from '$lib/assets/mii_silhouette.png';
  import { app } from '$lib/stores/app.svelte';
  import { operations } from '$lib/stores/operations.svelte';
  import { t } from '$lib/stores/i18n.svelte';
  import type {
    MiiBooleanField,
    MiiEditorState,
    MiiNumericField,
    MiiRendererStatus
  } from '$lib/api/types';
  import type { MiiRenderKind } from '$lib/api';

  interface Props {
    /** Id del Mii da modificare, `null` per crearne uno nuovo. */
    miiId: string | null;
    onclose: (changed: boolean) => void;
  }

  const { miiId, onclose }: Props = $props();

  let editor = $state<MiiEditorState | null>(null);
  let original = $state('');
  let favorites = $state<string[]>([]);
  let limits = $state<Limits>({});
  let icons = $state<IconSet | null>(null);
  let renderer = $state<MiiRendererStatus | null>(null);

  let category = $state(0);
  let loading = $state(true);
  let busy = $state(false);
  let installing = $state(false);
  let error = $state('');

  let nameInput = $state<HTMLInputElement | null>(null);
  let symbolsOpen = $state(false);

  const current = $derived(CATEGORIES[category] ?? CATEGORIES[0]);
  const dirty = $derived(editor !== null && JSON.stringify(editor) !== original);
  const title = $derived(miiId ? t('editor.edit') : t('editor.new'));
  const native = $derived(renderer?.nativeReady === true);

  $effect(() => {
    void load();
  });

  async function load() {
    loading = true;
    error = '';
    try {
      const [state, palette, table, status] = await Promise.all([
        miiId ? api.getMiiEditorState(miiId) : api.defaultMiiState('Vanza Mii', 4, false),
        api.getMiiFavoriteColors(),
        api.getMiiEditorLimits(),
        api.getMiiRendererStatus()
      ]);
      limits = toLimits(table);
      favorites = palette;
      renderer = status;
      // Il backend manda già uno stato valido; ripassarlo qui costa nulla e
      // protegge da uno stato nuovo costruito con valori fuori scala.
      editor = clampState(state, limits);
      original = JSON.stringify(editor);
    } catch (err) {
      error = api.errorMessage(err);
    } finally {
      loading = false;
    }

    // Le icone arrivano dopo, senza bloccare l'editor: fino ad allora ogni
    // scelta mostra il suo numero.
    loadIcons()
      .then((set) => (icons = set))
      .catch(() => (icons = null));
  }

  // -------------------------------------------------------------------------
  // Modifica
  // -------------------------------------------------------------------------

  /** Cambia un campo e riporta lo stato dentro i limiti. */
  function setNumber(field: MiiNumericField, value: number) {
    if (!editor || editor[field] === value) return;
    editor = clampState({ ...editor, [field]: value }, limits);
  }

  function setFlag(field: MiiBooleanField, value: boolean) {
    if (!editor || editor[field] === value) return;
    editor = { ...editor, [field]: value };
  }

  function step(control: Extract<Control, { kind: 'slider' }>, delta: number) {
    if (!editor) return;
    const range = rangeOf(control.field, editor, limits);
    const position = sliderPosition(editor[control.field], range, control.invert);
    setNumber(control.field, sliderValue(position + delta, range, control.invert));
  }

  /** Le scelte di una griglia: ogni valore fra i limiti del campo. */
  function valuesOf(field: MiiNumericField, state: MiiEditorState): number[] {
    const { min, max } = rangeOf(field, state, limits);
    return Array.from({ length: Math.max(0, max - min + 1) }, (_, index) => min + index);
  }

  function paletteOf(name: PaletteName): readonly string[] {
    return name === 'favorite' ? favorites : PALETTES[name];
  }

  const ICON_KINDS: PartIconKind[] = [
    'face',
    'hair',
    'eye',
    'eyebrow',
    'nose',
    'mouth',
    'mustache',
    'beard',
    'glasses'
  ];

  /** Colori di ruolo di ogni tipo di icona, per il Mii corrente. */
  const iconPalette = $derived.by(() => {
    const state = editor;
    const palette = {} as Record<PartIconKind, string[]>;
    if (!state) return palette;
    for (const kind of ICON_KINDS) palette[kind] = iconColors(kind, state, favorites);
    return palette;
  });

  // -------------------------------------------------------------------------
  // Anteprima
  // -------------------------------------------------------------------------

  /** Inquadratura: ritratto o figura intera, come nel WPF. */
  let shot = $state<MiiRenderKind>('face');
  /** Rotazione del Mii in gradi, attorno all'asse verticale. */
  let yaw = $state(0);
  let dragging = $state(false);

  let preview = $state<string | null>(null);
  let previewStatus = $state('');

  interface PreviewJob {
    state: MiiEditorState;
    kind: MiiRenderKind;
    rotation: number;
    size: number;
  }

  /**
   * Render nativo: uno alla volta, e solo l'ultimo chiesto.
   *
   * Trascinare un cursore o il Mii chiede un render per ogni movimento; con
   * il renderer nativo ognuno costa pochi millisecondi, ma metterli tutti in
   * fila farebbe rincorrere al Mii il mouse. Mentre uno è in corso, le
   * richieste nuove si sostituiscono a vicenda: quando finisce parte
   * l'ultima.
   */
  let pending: PreviewJob | null = null;
  let pumping = false;
  /** L'URL `blob:` dell'anteprima mostrata, da liberare quando cambia. */
  let previewUrl: string | null = null;

  function showBlob(image: Blob) {
    const url = URL.createObjectURL(image);
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    previewUrl = url;
    preview = url;
  }

  $effect(() => {
    return () => {
      if (previewUrl) URL.revokeObjectURL(previewUrl);
    };
  });

  async function pump() {
    if (pumping) return;
    pumping = true;
    try {
      while (pending) {
        const job = pending;
        pending = null;
        const image = await api
          .renderMiiPreview(job.state, job.kind, job.rotation, job.size)
          .catch(() => null);
        if (image) {
          showBlob(image);
          previewStatus = '';
        } else if (!pending) {
          previewStatus = t('editor.noRenderer');
        }
      }
    } finally {
      pumping = false;
    }
  }

  /** Lato del riquadro dell'anteprima, in pixel CSS: vedi `.stage`. */
  const STAGE_PIXELS = { face: 220, all_body: 260 } as const;

  let lastJobKey = '';

  $effect(() => {
    const state = editor;
    const kind = shot;
    const rotation = Math.round(yaw);
    const moving = dragging;
    const useNative = native;
    if (!state) return;

    const snapshot = $state.snapshot(state) as MiiEditorState;
    // Si renderizzano i pixel che il riquadro mostra davvero. Durante il
    // trascinamento ne basta la metà: arriva prima, e quella piena parte
    // appena il Mii si ferma.
    const full = renderSize(STAGE_PIXELS[kind]);
    const size = moving ? Math.max(128, full / 2) : full;

    // Scrivere il nome non cambia la faccia: nessun render.
    const key = `${useNative}:${kind}:${rotation}:${size}:${appearanceKey(snapshot)}`;
    if (key === lastJobKey) return;
    lastJobKey = key;

    if (useNative) {
      pending = { state: snapshot, kind, rotation, size };
      void pump();
      return;
    }

    // Senza renderer nativo ogni render è una richiesta a Mii Studio: si
    // aspettano 260 ms dall'ultima modifica, come `QueuePreviewRender`.
    previewStatus = t('editor.queued');
    let alive = true;
    const timer = setTimeout(() => {
      if (!alive) return;
      if (!preview) previewStatus = t('editor.rendering');
      void renderState(snapshot, kind, rotation, size).then((image) => {
        if (!alive) return;
        preview = image;
        previewStatus = image ? t('editor.ready') : t('editor.noRenderer');
      });
    }, 260);

    return () => {
      alive = false;
      clearTimeout(timer);
    };
  });

  // Rotazione con il trascinamento del mouse.
  const YAW_SENSITIVITY = 0.8;
  let dragStartX = 0;
  let dragStartYaw = 0;

  function onPointerDown(event: PointerEvent) {
    if (!native || event.button !== 0) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    dragging = true;
    dragStartX = event.clientX;
    dragStartYaw = yaw;
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging) return;
    yaw = wrapDegrees(dragStartYaw + (event.clientX - dragStartX) * YAW_SENSITIVITY);
  }

  function onPointerUp() {
    dragging = false;
  }

  function onStageKey(event: KeyboardEvent) {
    if (!native) return;
    if (event.key === 'ArrowLeft') yaw = wrapDegrees(yaw - 15);
    else if (event.key === 'ArrowRight') yaw = wrapDegrees(yaw + 15);
    else if (event.key === 'Home') yaw = 0;
    else return;
    event.preventDefault();
  }

  function wrapDegrees(value: number): number {
    return ((((value + 180) % 360) + 360) % 360) - 180;
  }

  async function installNative() {
    installing = true;
    try {
      renderer = await operations.run('mii-renderer', () => api.installMiiRenderer());
      if (renderer.nativeReady) {
        // Le facce "non riuscite" della cache di sessione ora riescono.
        forgetRenders();
        lastJobKey = '';
        app.toast(t('editor.nativeReady'), '', 'success');
      }
    } catch (err) {
      app.toast(t('editor.installFailed'), api.errorMessage(err), 'warning');
    } finally {
      installing = false;
    }
  }

  // -------------------------------------------------------------------------
  // Tratti del viso: miniature renderizzate
  // -------------------------------------------------------------------------

  /** Miniature dei tratti del viso, per valore. */
  let features = $state<Record<number, string>>({});
  let featuresSignature = '';

  /**
   * Un'icona non sa mostrare trucco e rughe: questi restano miniature vere,
   * il Mii corrente con quel solo tratto cambiato. Valgono finché non cambia
   * qualcos'altro della faccia.
   */
  $effect(() => {
    const state = editor;
    const open = current.controls.some((control) => control.kind === 'features');
    if (!state || !open) return;

    const snapshot = $state.snapshot(state) as MiiEditorState;
    const signature = appearanceKey({ ...snapshot, facialFeature: 0 });
    if (signature === featuresSignature) return;
    featuresSignature = signature;
    features = {};

    for (const value of valuesOf('facialFeature', snapshot)) {
      void renderState({ ...snapshot, facialFeature: value }, 'face', 0, 128).then((image) => {
        if (image && featuresSignature === signature) features = { ...features, [value]: image };
      });
    }
  });

  function selectCategory(index: number) {
    category = (index + CATEGORIES.length) % CATEGORIES.length;
  }

  // -------------------------------------------------------------------------
  // Nome
  // -------------------------------------------------------------------------

  /** Inserisce un simbolo nel nome, come `InsertNameSymbol`. */
  function insertSymbol(symbol: string) {
    if (!editor) return;

    const input = nameInput;
    const start = input?.selectionStart ?? editor.name.length;
    const end = input?.selectionEnd ?? start;
    const next = editor.name.slice(0, start) + symbol + editor.name.slice(end);

    editor.name = [...next].slice(0, 10).join('');
    symbolsOpen = false;
    input?.focus();
  }

  // -------------------------------------------------------------------------
  // Azioni
  // -------------------------------------------------------------------------

  async function randomize() {
    if (!editor) return;
    busy = true;
    try {
      const random = await api.randomMiiState(editor.name);
      // L'identità non si tocca: un Mii che esiste già mantiene il suo id.
      editor = clampState({ ...random, miiId: editor.miiId, systemId: editor.systemId }, limits);
    } catch (err) {
      app.toast(t('editor.randomFailed'), api.errorMessage(err), 'warning');
    } finally {
      busy = false;
    }
  }

  function reset() {
    if (original) editor = JSON.parse(original) as MiiEditorState;
  }

  async function persist() {
    if (!editor) return;
    if (!editor.name.trim()) {
      error = t('editor.needName');
      return;
    }

    busy = true;
    error = '';
    try {
      const saved = miiId
        ? await api.updateMii(miiId, editor)
        : await api.createMiiFromState(editor);
      app.toast(t('editor.saved'), t('editor.savedBody', { name: saved.name }), 'success');
      onclose(true);
    } catch (err) {
      error = api.errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function exportMii() {
    if (!miiId) return;

    const destination = await saveDialog({
      title: t('lic.exportMii'),
      defaultPath: `${editor?.name.trim() || 'mii'}.mii`,
      filters: [
        { name: t('editor.wiiMiiFilter'), extensions: ['mii', 'rcd', 'rsd'] },
        { name: t('lic.profileFilter'), extensions: ['json'] }
      ]
    });
    if (typeof destination !== 'string') return;

    busy = true;
    try {
      const written = await api.exportMii(miiId, destination);
      app.toast(t('lic.miiExported'), written, 'success');
    } catch (err) {
      app.toast(t('lic.exportFailed'), api.errorMessage(err), 'warning');
    } finally {
      busy = false;
    }
  }

  function close() {
    if (busy) return;
    onclose(false);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape') return;
    if (symbolsOpen) {
      symbolsOpen = false;
      return;
    }
    close();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="overlay">
  <button class="backdrop" aria-label={t('editor.close')} onclick={close} disabled={busy}></button>

  <div class="sheet vk-rainbow-top" role="dialog" aria-modal="true" aria-label={title}>
    <header class="head">
      <div>
        <h2 class="head-title">Mii Studio</h2>
        <p class="vk-subtitle">{t('editor.subtitle', { title })}</p>
      </div>
      <button class="vk-btn" onclick={close} disabled={busy}>
        <Icon name="close" size={14} />
        {t('common.close')}
      </button>
    </header>

    {#if loading}
      <div class="vk-skeleton loading"></div>
    {:else if !editor}
      <div class="vk-error">{error || t('editor.unreadable')}</div>
    {:else}
      <div class="body">
        <aside class="side">
          <div class="preview vk-card">
            <!-- L'anteprima è un cursore: le frecce e il trascinamento girano il Mii. -->
            <div
              class="stage"
              class:body-shot={shot === 'all_body'}
              class:rotatable={native}
              class:dragging
              role="slider"
              aria-label={t('editor.previewOf', { name: editor.name })}
              aria-valuemin={-180}
              aria-valuemax={180}
              aria-valuenow={Math.round(yaw)}
              aria-valuetext={`${Math.round(yaw)}°`}
              aria-disabled={!native}
              title={native ? t('editor.rotateHint') : undefined}
              tabindex="0"
              onpointerdown={onPointerDown}
              onpointermove={onPointerMove}
              onpointerup={onPointerUp}
              onpointercancel={onPointerUp}
              ondblclick={() => (yaw = 0)}
              onkeydown={onStageKey}
            >
              {#if preview}
                <img src={preview} alt="" draggable="false" />
              {:else}
                <img class="silhouette" src={miiSilhouette} alt="" draggable="false" />
              {/if}
            </div>

            <div class="shots">
              <button
                class="vk-btn shot"
                class:active={shot === 'face'}
                aria-pressed={shot === 'face'}
                onclick={() => (shot = 'face')}
              >
                {t('editor.shotFace')}
              </button>
              <button
                class="vk-btn shot"
                class:active={shot === 'all_body'}
                aria-pressed={shot === 'all_body'}
                onclick={() => (shot = 'all_body')}
              >
                {t('editor.shotBody')}
              </button>
              {#if yaw !== 0}
                <button class="vk-btn shot" onclick={() => (yaw = 0)}>
                  {t('editor.resetView')}
                </button>
              {/if}
            </div>

            <p class="preview-name">{editor.name || 'Mii'}</p>
            <p class="vk-faint preview-meta">
              {t('editor.meta', {
                sex: editor.isFemale ? t('miicat.female') : t('miicat.male'),
                color: editor.favoriteColorIndex + 1,
                month: editor.birthMonth,
                day: editor.birthDay
              })}
            </p>
            {#if native}
              <p class="vk-faint preview-status">{previewStatus || t('editor.native')}</p>
            {:else}
              <p class="vk-faint preview-status">{previewStatus}</p>
            {/if}
          </div>

          {#if renderer && !native}
            <div class="native-offer">
              <p class="native-title">{t('editor.online')}</p>
              <p class="vk-faint native-body">
                {t('editor.onlineBody', { host: renderer.runtimeHost || 'web.archive.org' })}
              </p>
              <button
                class="vk-btn vk-btn--primary"
                onclick={installNative}
                disabled={installing || busy}
              >
                <Icon name="download" size={14} />
                {installing ? t('editor.installing') : t('editor.installNative')}
              </button>
            </div>
          {/if}

          <div class="field">
            <span class="vk-eyebrow">{t('editor.name')}</span>
            <div class="name-row">
              <input
                class="vk-input"
                maxlength="10"
                bind:value={editor.name}
                bind:this={nameInput}
              />
              <button
                class="vk-btn symbol-btn"
                title={t('editor.insertSymbol')}
                aria-expanded={symbolsOpen}
                onclick={() => (symbolsOpen = !symbolsOpen)}
              >
                ★
              </button>
            </div>
            {#if symbolsOpen}
              <div class="symbols">
                {#each NAME_SYMBOLS as symbol (symbol)}
                  <button class="symbol" onclick={() => insertSymbol(symbol)}>{symbol}</button>
                {/each}
              </div>
            {/if}
          </div>

          <label class="field">
            <span class="vk-eyebrow">{t('editor.creator')}</span>
            <input class="vk-input" maxlength="10" bind:value={editor.creatorName} />
          </label>

          {#if error}
            <p class="vk-error inline">{error}</p>
          {/if}

          <div class="actions">
            <button class="vk-btn vk-btn--primary" onclick={persist} disabled={busy || !dirty}>
              {t('common.save')}
            </button>
            <button class="vk-btn" onclick={close} disabled={busy}>{t('common.cancel')}</button>
            <button class="vk-btn" onclick={randomize} disabled={busy}>{t('editor.random')}</button>
            <button class="vk-btn" onclick={reset} disabled={busy || !dirty}>
              {t('settings.reset')}
            </button>
            {#if miiId}
              <button class="vk-btn export" onclick={exportMii} disabled={busy}>
                {t('editor.exportFile')}
              </button>
            {/if}
          </div>

          <p class="vk-faint saved-hint">
            {dirty ? t('editor.unsaved') : t('editor.noChanges')}
          </p>
        </aside>

        <div class="editor">
          <nav class="rail" aria-label={t('editor.categories')}>
            {#each CATEGORIES as item, index (item.key)}
              <button
                class="rail-item"
                class:active={category === index}
                title={t(item.hint)}
                aria-pressed={category === index}
                onclick={() => selectCategory(index)}
              >
                {#if item.icon && icons}
                  {@const glyph = icons[item.icon.kind][editor[item.icon.field]]}
                  {#if glyph}
                    <span class="rail-icon">
                      <MiiPartIcon icon={glyph} colors={iconPalette[item.icon.kind] ?? []} />
                    </span>
                  {/if}
                {/if}
                {t(item.label)}
              </button>
            {/each}
          </nav>

          <div class="panel vk-card">
            <div class="panel-head">
              <p class="panel-title">{t(current.label)}</p>
              <p class="vk-subtitle">{t(current.hint)}</p>
            </div>

            {#each current.controls as control (control.field)}
              {#if control.kind === 'parts'}
                <section class="group">
                  <p class="group-title vk-eyebrow">{t(control.label)}</p>
                  <div class="parts" role="radiogroup" aria-label={t(control.label)}>
                    {#each valuesOf(control.field, editor) as value (value)}
                      {@const glyph = icons?.[control.icon][value]}
                      <button
                        class="part"
                        class:selected={editor[control.field] === value}
                        role="radio"
                        aria-checked={editor[control.field] === value}
                        title={`${t(control.label)} ${value + 1}`}
                        onclick={() => setNumber(control.field, value)}
                      >
                        {#if glyph}
                          <span class="glyph">
                            <MiiPartIcon icon={glyph} colors={iconPalette[control.icon] ?? []} />
                          </span>
                        {:else}
                          <span class="part-number">{value + 1}</span>
                        {/if}
                      </button>
                    {/each}
                  </div>
                </section>
              {:else if control.kind === 'swatches'}
                {@const palette = paletteOf(control.palette)}
                <section class="group">
                  <p class="group-title vk-eyebrow">{t(control.label)}</p>
                  <div class="swatches" role="radiogroup" aria-label={t(control.label)}>
                    {#each valuesOf(control.field, editor) as value (value)}
                      <button
                        class="swatch"
                        class:selected={editor[control.field] === value}
                        role="radio"
                        aria-checked={editor[control.field] === value}
                        aria-label={`${t(control.label)} ${value + 1}`}
                        title={`${t(control.label)} ${value + 1}`}
                        style="--swatch: {palette[value] ?? '#000'}"
                        onclick={() => setNumber(control.field, value)}
                      ></button>
                    {/each}
                  </div>
                </section>
              {:else if control.kind === 'features'}
                <section class="group">
                  <p class="group-title vk-eyebrow">{t(control.label)}</p>
                  <div class="features" role="radiogroup" aria-label={t(control.label)}>
                    {#each valuesOf(control.field, editor) as value (value)}
                      <button
                        class="feature"
                        class:selected={editor.facialFeature === value}
                        role="radio"
                        aria-checked={editor.facialFeature === value}
                        onclick={() => setNumber('facialFeature', value)}
                      >
                        {#if features[value]}
                          <img src={features[value]} alt="" />
                        {:else}
                          <span class="feature-placeholder">
                            <img class="silhouette" src={miiSilhouette} alt="" />
                          </span>
                        {/if}
                        <span class="feature-label">
                          {t(FACIAL_FEATURES[value] ?? 'miicat.features')}
                        </span>
                      </button>
                    {/each}
                  </div>
                </section>
              {:else if control.kind === 'switch'}
                <section class="group">
                  <p class="group-title vk-eyebrow">{t(control.label)}</p>
                  <div class="segmented" role="radiogroup" aria-label={t(control.label)}>
                    {#each [false, true] as value (value)}
                      <button
                        class="vk-btn segment"
                        class:active={editor[control.field] === value}
                        role="radio"
                        aria-checked={editor[control.field] === value}
                        onclick={() => setFlag(control.field, value)}
                      >
                        {value ? t(control.on) : t(control.off)}
                      </button>
                    {/each}
                  </div>
                </section>
              {:else if control.kind === 'toggle'}
                <label class="toggle">
                  <input
                    type="checkbox"
                    checked={editor[control.field]}
                    onchange={(event) => setFlag(control.field, event.currentTarget.checked)}
                  />
                  <span>{t(control.label)}</span>
                </label>
              {:else if control.kind === 'slider'}
                {@const range = rangeOf(control.field, editor, limits)}
                {@const position = sliderPosition(editor[control.field], range, control.invert)}
                <div class="slider">
                  <span class="slider-head">
                    <span>{t(control.label)}</span>
                    <strong>{position}</strong>
                  </span>
                  <div class="slider-row">
                    <button
                      class="vk-btn step"
                      aria-label={`${t('editor.decrease')}: ${t(control.label)}`}
                      disabled={position <= range.min}
                      onclick={() => step(control, -1)}
                    >
                      −
                    </button>
                    <input
                      type="range"
                      min={range.min}
                      max={range.max}
                      step="1"
                      value={position}
                      aria-label={t(control.label)}
                      oninput={(event) =>
                        setNumber(
                          control.field,
                          sliderValue(Number(event.currentTarget.value), range, control.invert)
                        )}
                    />
                    <button
                      class="vk-btn step"
                      aria-label={`${t('editor.increase')}: ${t(control.label)}`}
                      disabled={position >= range.max}
                      onclick={() => step(control, 1)}
                    >
                      +
                    </button>
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 20px;
    animation: fade var(--vk-dur) var(--vk-ease);
  }

  .backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: rgb(4 7 14 / 0.78);
    backdrop-filter: blur(4px);
    cursor: default;
  }

  .sheet {
    position: relative;
    display: flex;
    flex-direction: column;
    width: min(1180px, 100%);
    height: min(880px, 100%);
    padding: 20px 22px 22px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel);
    box-shadow: var(--vk-shadow-modal);
    overflow: hidden;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding-bottom: 14px;
  }

  .head-title {
    margin: 0;
    font-size: 22px;
    font-weight: 900;
  }

  .head .vk-subtitle {
    margin-top: 2px;
    font-size: var(--vk-fs-micro);
  }

  .loading {
    height: 100%;
  }

  .body {
    display: grid;
    grid-template-columns: 300px 1fr;
    gap: 18px;
    min-height: 0;
    flex: 1;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    overflow-y: auto;
    padding-right: 4px;
  }

  .preview {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    background: var(--vk-card-gradient);
  }

  .stage {
    display: grid;
    place-items: center;
    width: 220px;
    height: 220px;
    border-radius: 14px;
    outline: none;
    touch-action: none;
    user-select: none;
  }

  .stage.rotatable {
    cursor: grab;
  }

  .stage.dragging {
    cursor: grabbing;
  }

  .stage:focus-visible {
    box-shadow: 0 0 0 2px var(--vk-cyan);
  }

  .stage.body-shot {
    height: 260px;
  }

  .stage img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    filter: drop-shadow(0 6px 18px rgb(0 0 0 / 0.35));
    pointer-events: none;
  }

  .silhouette {
    opacity: 0.35;
  }

  .shots {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
  }

  .shot {
    padding: 4px 12px;
    font-size: var(--vk-fs-micro);
  }

  .shot.active,
  .segment.active {
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

  .preview-name {
    margin: 4px 0 0;
    font-size: 20px;
    font-weight: 900;
  }

  .preview-meta,
  .preview-status {
    margin: 0;
    font-size: var(--vk-fs-eyebrow);
    text-align: center;
  }

  .native-offer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid color-mix(in srgb, var(--vk-cyan) 45%, transparent);
    border-radius: var(--vk-radius-badge);
    background: color-mix(in srgb, var(--vk-cyan) 8%, transparent);
  }

  .native-title {
    margin: 0;
    font-size: var(--vk-fs-small);
    font-weight: 900;
  }

  .native-body {
    margin: 0;
    font-size: var(--vk-fs-eyebrow);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .name-row {
    display: flex;
    gap: 6px;
  }

  .name-row .vk-input {
    flex: 1;
    min-width: 0;
  }

  .symbol-btn {
    padding: 0 12px;
    font-size: 15px;
  }

  .symbols {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 4px;
    padding: 8px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: #111a2c;
  }

  .symbol {
    height: 26px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: inherit;
    font-size: 14px;
    font-weight: 900;
    cursor: pointer;
  }

  .symbol:hover {
    border-color: #3a4c74;
  }

  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .actions .export {
    grid-column: 1 / -1;
  }

  .inline {
    padding: 10px 12px;
    font-size: var(--vk-fs-micro);
  }

  .saved-hint {
    margin: 0;
    font-size: var(--vk-fs-eyebrow);
  }

  .editor {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
  }

  .rail {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .rail-item {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: #111a2c;
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .rail-item:hover {
    border-color: #3a4c74;
  }

  .rail-icon {
    display: inline-block;
    width: 22px;
    height: 22px;
    padding: 2px;
    border-radius: 6px;
    background: #dfe6f3;
  }

  .rail-item.active {
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

  .panel {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .panel-title {
    margin: 0;
    font-size: 22px;
    font-weight: 900;
  }

  .group {
    margin-top: 16px;
  }

  .group-title {
    margin: 0 0 8px;
  }

  /* Le icone stanno su un fondo chiaro, come nel Canale Mii: un sopracciglio
     nero su una tessera scura non si vedrebbe. */
  .parts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(62px, 1fr));
    gap: 8px;
  }

  .part {
    position: relative;
    display: grid;
    place-items: center;
    aspect-ratio: 1;
    padding: 9px;
    border: 2px solid transparent;
    border-radius: 12px;
    background: #dfe6f3;
    color: #24252d;
    cursor: pointer;
    transition:
      transform var(--vk-dur-fast) var(--vk-ease),
      border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .part:hover {
    transform: translateY(-1px);
    border-color: #8fb6ff;
  }

  .part.selected {
    border-color: var(--vk-cyan);
    background: #f4f8ff;
    box-shadow:
      0 0 0 2px rgb(0 242 255 / 0.25),
      0 0 16px rgb(0 242 255 / 0.3);
  }

  /* L'icona sta dentro la tessera invece di dimensionarla: un'acconciatura
     più alta che larga non deve allungare la sua riga. */
  .glyph {
    position: absolute;
    inset: 9px;
  }

  .part-number {
    font-size: var(--vk-fs-micro);
    font-weight: 900;
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .swatch {
    width: 34px;
    height: 34px;
    border: 2px solid rgb(255 255 255 / 0.18);
    border-radius: 50%;
    background: var(--swatch);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.35);
    cursor: pointer;
    transition: transform var(--vk-dur-fast) var(--vk-ease);
  }

  .swatch:hover {
    transform: scale(1.08);
  }

  .swatch.selected {
    border-color: var(--vk-text);
    box-shadow:
      0 0 0 3px var(--vk-cyan),
      inset 0 0 0 1px rgb(0 0 0 / 0.35);
  }

  .features {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    gap: 10px;
  }

  .feature {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 6px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: #111a2c;
    color: inherit;
    cursor: pointer;
  }

  .feature:hover {
    border-color: #3a4c74;
  }

  .feature.selected {
    border-color: var(--vk-cyan);
    box-shadow: 0 0 14px rgb(0 242 255 / 0.25);
  }

  .feature img,
  .feature-placeholder {
    display: grid;
    place-items: center;
    width: 100%;
    aspect-ratio: 1;
    object-fit: contain;
  }

  .feature-placeholder img {
    width: 60%;
    height: 60%;
  }

  .feature-label {
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    text-align: center;
  }

  .segmented {
    display: inline-flex;
    gap: 6px;
  }

  .segment {
    padding: 6px 16px;
    font-size: var(--vk-fs-micro);
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    margin-right: 18px;
    font-size: var(--vk-fs-small);
    font-weight: 700;
  }

  .toggle input {
    width: 16px;
    height: 16px;
    accent-color: var(--vk-cyan);
  }

  .slider {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 14px;
    max-width: 520px;
  }

  .slider-head {
    display: flex;
    justify-content: space-between;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .slider-head strong {
    color: var(--vk-cyan-soft);
    font-weight: 900;
  }

  .slider-row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 10px;
  }

  .step {
    width: 30px;
    height: 30px;
    padding: 0;
    font-size: 16px;
    font-weight: 900;
    line-height: 1;
  }

  .slider input {
    width: 100%;
    accent-color: var(--vk-cyan);
  }

  @media (max-width: 980px) {
    .body {
      grid-template-columns: 1fr;
      overflow-y: auto;
    }
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>
