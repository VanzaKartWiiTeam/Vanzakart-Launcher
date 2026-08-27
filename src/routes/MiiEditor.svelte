<script lang="ts">
  /**
   * Editor Mii.
   *
   * Porta `Launcher/MiiEditorWindow.xaml(.cs)`, come modale a tutta pagina
   * invece che come finestra separata (`docs/decisions.md` §U-05): una webview
   * sola, nessun secondo runtime. Ciò che si modifica è il Mii dentro
   * `RFL_DB.dat`: salvare scrive nel database di Dolphin (§D-037).
   *
   * Tutto il resto è il legacy:
   *
   * - l'anteprima a sinistra è un **render vero**, chiesto al servizio
   *   immagini di Mii Studio con la "studio data" del Mii che si sta
   *   costruendo, e si aggiorna 260 ms dopo l'ultima modifica come
   *   `QueuePreviewRender`;
   * - le scelte di ogni categoria sono **miniature renderizzate**: il Mii
   *   corrente con quel solo tratto cambiato, sei per pagina come
   *   `OptionsPerPage`;
   * - i cursori di rifinitura stanno dietro al pulsante "Regola", che è il
   *   popup "Adjust" del WPF.
   */
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';

  import * as api from '$lib/api';
  import { CATEGORIES, NAME_SYMBOLS, OPTIONS_PER_PAGE } from '$lib/mii/categories';
  import type { OptionGroup } from '$lib/mii/categories';
  import { appearanceKey, renderState } from '$lib/mii/render';
  import Icon from '$lib/components/Icon.svelte';
  import miiSilhouette from '$lib/assets/mii_silhouette.png';
  import { app } from '$lib/stores/app.svelte';
  import type { MiiEditorState, MiiNumericField } from '$lib/api/types';
  import type { MiiRenderKind } from '$lib/api';

  interface Props {
    /** Id del Mii da modificare, `null` per crearne uno nuovo. */
    miiId: string | null;
    onclose: (changed: boolean) => void;
  }

  const { miiId, onclose }: Props = $props();

  let editor = $state<MiiEditorState | null>(null);
  let original = $state('');
  let colors = $state<string[]>([]);
  let category = $state(0);
  let page = $state(0);
  let adjusting = $state(false);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state('');

  /** Inquadratura dell'anteprima: ritratto o figura intera, come nel WPF. */
  let shot = $state<MiiRenderKind>('face');

  let preview = $state<string | null>(null);
  let previewStatus = $state('Anteprima in coda…');
  /** Miniature delle opzioni, per chiave `campo:valore`. */
  let thumbnails = $state<Record<string, string>>({});

  let nameInput = $state<HTMLInputElement | null>(null);
  let symbolsOpen = $state(false);

  const current = $derived(CATEGORIES[category] ?? CATEGORIES[0]);
  const dirty = $derived(editor !== null && JSON.stringify(editor) !== original);
  const title = $derived(miiId ? 'Modifica Mii' : 'Nuovo Mii');

  /** Il legacy impagina ogni categoria tranne la barba, che mostra tutto. */
  const paginated = $derived(current.groups.length === 1 && current.groups[0]?.kind !== 'color');

  $effect(() => {
    void load();
  });

  async function load() {
    loading = true;
    error = '';
    try {
      const [state, palette] = await Promise.all([
        miiId ? api.getMiiEditorState(miiId) : api.defaultMiiState('Vanza Mii', 4, false),
        api.getMiiFavoriteColors()
      ]);
      editor = state;
      original = JSON.stringify(state);
      colors = palette;
    } catch (err) {
      error = api.errorMessage(err);
    } finally {
      loading = false;
    }
  }

  // -------------------------------------------------------------------------
  // Anteprima
  // -------------------------------------------------------------------------

  /**
   * Render dell'anteprima, 260 ms dopo l'ultima modifica.
   *
   * L'attesa è quella di `QueuePreviewRender`: trascinare un cursore cambia lo
   * stato decine di volte al secondo, e ogni cambio è una richiesta di render.
   */
  let lastPreviewKey = '';

  $effect(() => {
    const state = editor;
    const kind = shot;
    if (!state) return;

    const snapshot = $state.snapshot(state) as MiiEditorState;

    // Scrivere il nome non cambia la faccia: la richiesta parte lo stesso e
    // la coda la serve dalla cache, ma dire "in coda" farebbe lampeggiare uno
    // stato che non descrive nulla.
    const key = `${kind}:${appearanceKey(state)}`;
    if (key !== lastPreviewKey) {
      lastPreviewKey = key;
      previewStatus = 'Anteprima in coda…';
    }

    let alive = true;
    const timer = setTimeout(() => {
      if (!alive) return;
      if (!preview) previewStatus = 'Render in corso…';

      void renderState(snapshot, kind).then((image) => {
        if (!alive) return;
        preview = image;
        previewStatus = image ? 'Anteprima pronta' : 'Il renderer non risponde: resta la sagoma.';
      });
    }, 260);

    return () => {
      alive = false;
      clearTimeout(timer);
    };
  });

  // -------------------------------------------------------------------------
  // Griglia delle scelte
  // -------------------------------------------------------------------------

  interface GridOption {
    key: string;
    label: string;
    title: string;
    selected: boolean;
    /** Tinta piatta invece del render, per la tavolozza. */
    color?: string;
    /** Stato da renderizzare nella miniatura. */
    state?: MiiEditorState;
    apply: () => void;
  }

  function withNumber(
    state: MiiEditorState,
    field: MiiNumericField,
    value: number
  ): MiiEditorState {
    return { ...state, [field]: value };
  }

  function buildOptions(group: OptionGroup, state: MiiEditorState): GridOption[] {
    if (group.kind === 'color') {
      return colors.map((color, index) => ({
        key: `favoriteColorIndex:${index}`,
        label: `${index + 1}`,
        title: `${group.label} ${index + 1}`,
        selected: state.favoriteColorIndex === index,
        color,
        apply: () => {
          if (editor) editor.favoriteColorIndex = index;
        }
      }));
    }

    if (group.kind === 'switch') {
      return [false, true].map((value) => ({
        key: `${group.field}:${value}`,
        label: value ? group.on : group.off,
        title: value ? group.on : group.off,
        selected: state[group.field] === value,
        state: { ...state, [group.field]: value },
        apply: () => {
          if (editor) editor[group.field] = value;
        }
      }));
    }

    const options: GridOption[] = [];
    for (let value = group.min; value <= group.max; value += 1) {
      options.push({
        key: `${group.field}:${value}`,
        label: `${value + 1}`,
        title: `${group.label} ${value + 1}`,
        selected: state[group.field] === value,
        state: withNumber(state, group.field, value),
        apply: () => {
          if (editor) editor[group.field] = value;
        }
      });
    }
    return options;
  }

  const grid = $derived.by(() => {
    const state = editor;
    if (!state) return [] as { label: string; options: GridOption[] }[];

    return current.groups.map((group) => ({
      label: group.label,
      options: buildOptions(group, state)
    }));
  });

  const pageCount = $derived(
    paginated ? Math.max(1, Math.ceil((grid[0]?.options.length ?? 0) / OPTIONS_PER_PAGE)) : 1
  );

  const visible = $derived.by(() => {
    if (!paginated) return grid;

    const only = grid[0];
    if (!only) return [];

    const from = Math.min(page, pageCount - 1) * OPTIONS_PER_PAGE;
    return [{ label: only.label, options: only.options.slice(from, from + OPTIONS_PER_PAGE) }];
  });

  /**
   * Chiavi già richieste. Non è stato reattivo di proposito: serve solo a non
   * chiedere due volte la stessa miniatura, e renderlo reattivo farebbe
   * ripartire l'effetto a ogni immagine che arriva.
   */
  let requested: Record<string, true> = {};
  let lastAppearance = '';

  /**
   * Chiede il render di ogni miniatura visibile, e le butta quando invecchiano.
   *
   * Una miniatura mostra il Mii corrente con **un** tratto cambiato: appena
   * cambia qualunque altro tratto non vale più, e tenerla mostrerebbe una
   * faccia che non esiste. Il tratto che la griglia sta variando è l'unico che
   * non le invalida — è proprio quello che distingue una miniatura dall'altra.
   */
  $effect(() => {
    const state = editor;
    const groups = visible;
    if (!state) return;

    const signature = thumbnailSignature(state);
    if (signature !== lastAppearance) {
      lastAppearance = signature;
      requested = {};
      thumbnails = {};
    }

    for (const group of groups) {
      for (const option of group.options) {
        if (!option.state || requested[option.key]) continue;

        requested[option.key] = true;
        const snapshot = $state.snapshot(option.state) as MiiEditorState;
        const key = option.key;
        void renderState(snapshot, 'face').then((image) => {
          if (image) thumbnails = { ...thumbnails, [key]: image };
        });
      }
    }
  });

  /** Firma dell'aspetto senza i campi che la griglia aperta sta variando. */
  function thumbnailSignature(state: MiiEditorState): string {
    const varying = new Set<string>(
      current.groups.flatMap((group) => (group.kind === 'color' ? [] : [group.field as string]))
    );

    return Object.entries(state)
      .filter(([field]) => !varying.has(field))
      .map(([field, value]) => `${field}=${String(value)}`)
      .join('|');
  }

  function selectCategory(index: number) {
    category = (index + CATEGORIES.length) % CATEGORIES.length;
    page = 0;
    adjusting = false;
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
      editor = { ...random, miiId: editor.miiId, systemId: editor.systemId };
    } catch (err) {
      app.toast('Mii casuale non riuscito', api.errorMessage(err), 'warning');
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
      error = 'Scegli un nome per il Mii.';
      return;
    }

    busy = true;
    error = '';
    try {
      const saved = miiId
        ? await api.updateMii(miiId, editor)
        : await api.createMiiFromState(editor);
      app.toast('Mii salvato', `${saved.name} è pronto.`, 'success');
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
      title: 'Esporta il Mii',
      defaultPath: `${editor?.name.trim() || 'mii'}.mii`,
      filters: [
        { name: 'Mii Wii', extensions: ['mii', 'rcd', 'rsd'] },
        { name: 'Profilo del launcher', extensions: ['json'] }
      ]
    });
    if (typeof destination !== 'string') return;

    busy = true;
    try {
      const written = await api.exportMii(miiId, destination);
      app.toast('Mii esportato', written, 'success');
    } catch (err) {
      app.toast('Export non riuscito', api.errorMessage(err), 'warning');
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
  <button class="backdrop" aria-label="Chiudi l’editor Mii" onclick={close} disabled={busy}
  ></button>

  <div class="sheet vk-rainbow-top" role="dialog" aria-modal="true" aria-label={title}>
    <header class="head">
      <div>
        <h2 class="head-title">Mii Studio</h2>
        <p class="vk-subtitle">{title} — ogni modifica resta nel launcher finché non salvi.</p>
      </div>
      <button class="vk-btn" onclick={close} disabled={busy}>
        <Icon name="close" size={14} />
        Chiudi
      </button>
    </header>

    {#if loading}
      <div class="vk-skeleton loading"></div>
    {:else if !editor}
      <div class="vk-error">{error || 'Mii non leggibile.'}</div>
    {:else}
      <div class="body">
        <aside class="side">
          <div class="preview vk-card">
            <div class="stage" class:body-shot={shot === 'all_body'}>
              {#if preview}
                <img src={preview} alt="Anteprima di {editor.name}" />
              {:else}
                <img class="silhouette" src={miiSilhouette} alt="" />
              {/if}
            </div>

            <div class="shots">
              <button
                class="vk-btn shot"
                class:active={shot === 'face'}
                aria-pressed={shot === 'face'}
                onclick={() => (shot = 'face')}
              >
                Volto
              </button>
              <button
                class="vk-btn shot"
                class:active={shot === 'all_body'}
                aria-pressed={shot === 'all_body'}
                onclick={() => (shot = 'all_body')}
              >
                Figura
              </button>
            </div>

            <p class="preview-name">{editor.name || 'Mii'}</p>
            <p class="vk-faint preview-meta">
              {editor.isFemale ? 'Femmina' : 'Maschio'} · Colore {editor.favoriteColorIndex + 1} ·
              {editor.birthMonth}/{editor.birthDay}
            </p>
            <p class="vk-faint preview-status">{previewStatus}</p>
          </div>

          <div class="field">
            <span class="vk-eyebrow">Nome del Mii</span>
            <div class="name-row">
              <input
                class="vk-input"
                maxlength="10"
                bind:value={editor.name}
                bind:this={nameInput}
              />
              <button
                class="vk-btn symbol-btn"
                title="Inserisci un simbolo"
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
            <span class="vk-eyebrow">Creatore</span>
            <input class="vk-input" maxlength="10" bind:value={editor.creatorName} />
          </label>

          {#if error}
            <p class="vk-error inline">{error}</p>
          {/if}

          <div class="actions">
            <button class="vk-btn vk-btn--primary" onclick={persist} disabled={busy || !dirty}>
              Salva
            </button>
            <button class="vk-btn" onclick={close} disabled={busy}>Annulla</button>
            <button class="vk-btn" onclick={randomize} disabled={busy}>Casuale</button>
            <button class="vk-btn" onclick={reset} disabled={busy || !dirty}>Ripristina</button>
            {#if miiId}
              <button class="vk-btn export" onclick={exportMii} disabled={busy}>
                Esporta file .mii
              </button>
            {/if}
          </div>

          <p class="vk-faint saved-hint">
            {dirty ? 'Modifiche non salvate.' : 'Nessuna modifica in sospeso.'}
          </p>
        </aside>

        <div class="editor">
          <nav class="rail" aria-label="Categorie">
            {#each CATEGORIES as item, index (item.key)}
              <button
                class="rail-item"
                class:active={category === index}
                title={item.hint}
                onclick={() => selectCategory(index)}
              >
                {item.label}
              </button>
            {/each}
          </nav>

          <div class="panel vk-card">
            <div class="panel-head">
              <div>
                <p class="panel-title">{current.label}</p>
                <p class="vk-subtitle">{current.hint}</p>
              </div>

              <div class="panel-tools">
                {#if paginated && pageCount > 1}
                  <div class="pager">
                    <button
                      class="vk-btn pager-btn"
                      aria-label="Pagina precedente"
                      onclick={() => (page = Math.max(0, page - 1))}
                      disabled={page <= 0}
                    >
                      ‹
                    </button>
                    <span class="vk-mono page-label"
                      >{Math.min(page, pageCount - 1) + 1}/{pageCount}</span
                    >
                    <button
                      class="vk-btn pager-btn"
                      aria-label="Pagina successiva"
                      onclick={() => (page = Math.min(pageCount - 1, page + 1))}
                      disabled={page >= pageCount - 1}
                    >
                      ›
                    </button>
                  </div>
                {/if}

                {#if current.sliders.length > 0 || current.toggles.length > 0}
                  <button
                    class="vk-btn"
                    aria-expanded={adjusting}
                    onclick={() => (adjusting = !adjusting)}
                  >
                    Regola
                  </button>
                {/if}
              </div>
            </div>

            {#each visible as group (group.label)}
              {#if current.groups.length > 1}
                <p class="group-title vk-eyebrow">{group.label}</p>
              {/if}

              <div class="options">
                {#each group.options as option (option.key)}
                  <button
                    class="option"
                    class:selected={option.selected}
                    title={option.title}
                    aria-pressed={option.selected}
                    onclick={option.apply}
                  >
                    {#if option.color}
                      <span class="swatch" style="--swatch: {option.color}"></span>
                    {:else if thumbnails[option.key]}
                      <img src={thumbnails[option.key]} alt="" />
                    {:else}
                      <span class="thumb-placeholder">
                        <img class="silhouette" src={miiSilhouette} alt="" />
                      </span>
                    {/if}
                    <span class="option-label">{option.label}</span>
                  </button>
                {/each}
              </div>
            {/each}

            {#if adjusting}
              <div class="adjust">
                {#if current.toggles.length > 0}
                  <div class="toggles">
                    {#each current.toggles as toggle (toggle.field)}
                      <label class="toggle">
                        <input type="checkbox" bind:checked={editor[toggle.field]} />
                        <span>{toggle.label}</span>
                      </label>
                    {/each}
                  </div>
                {/if}

                <div class="sliders">
                  {#each current.sliders as slider (slider.field)}
                    <label class="slider">
                      <span class="slider-head">
                        <span>{slider.label}</span>
                        <strong>{editor[slider.field]}</strong>
                      </span>
                      <input
                        type="range"
                        min={slider.min}
                        max={slider.max}
                        step="1"
                        bind:value={editor[slider.field]}
                      />
                    </label>
                  {/each}
                </div>
              </div>
            {/if}
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
    width: 190px;
    height: 190px;
  }

  .stage.body-shot {
    height: 240px;
  }

  .stage img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    filter: drop-shadow(0 6px 18px rgb(0 0 0 / 0.35));
  }

  .silhouette {
    opacity: 0.35;
  }

  .shots {
    display: flex;
    gap: 6px;
  }

  .shot {
    padding: 4px 12px;
    font-size: var(--vk-fs-micro);
  }

  .shot.active {
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
    padding: 8px 14px;
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

  .panel-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .panel-title {
    margin: 0;
    font-size: 22px;
    font-weight: 900;
  }

  .panel-tools {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .pager {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .pager-btn {
    padding: 4px 12px;
    font-size: 15px;
    line-height: 1;
  }

  .page-label {
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .group-title {
    margin: 16px 0 6px;
  }

  .options {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(126px, 1fr));
    gap: 12px;
    margin-top: 16px;
  }

  .option {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: #111a2c;
    color: inherit;
    cursor: pointer;
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .option:hover {
    border-color: #3a4c74;
  }

  .option.selected {
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

  .option img,
  .option .thumb-placeholder,
  .option .swatch {
    display: grid;
    place-items: center;
    width: 100%;
    aspect-ratio: 1;
    border-radius: 10px;
    object-fit: contain;
  }

  .option .thumb-placeholder img {
    width: 60%;
    height: 60%;
  }

  .option .swatch {
    background: var(--swatch);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.35);
  }

  .option-label {
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
  }

  .adjust {
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid var(--vk-stroke);
  }

  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: var(--vk-fs-small);
    font-weight: 700;
  }

  .toggle input {
    width: 16px;
    height: 16px;
    accent-color: var(--vk-cyan);
  }

  .sliders {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 14px 22px;
    margin-top: 16px;
  }

  .slider {
    display: flex;
    flex-direction: column;
    gap: 4px;
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
