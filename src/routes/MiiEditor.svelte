<script lang="ts">
  /**
   * Editor Mii.
   *
   * Porta `Launcher/MiiEditorWindow.xaml(.cs)`, come modale a tutta pagina
   * invece che come finestra separata (`docs/decisions.md` §U-05). Ciò che si
   * modifica è il Mii dentro `RFL_DB.dat`: salvare scrive nel database di
   * Dolphin (§D-037).
   *
   * Tre colonne, come il Canale Mii: le categorie a sinistra, il Mii al
   * centro, i controlli della categoria a destra. L'editor è nativo (§D-092,
   * §D-095):
   *
   * - l'anteprima la disegna il **renderer nativo** del launcher, senza rete:
   *   si gira trascinandola, si inclina, si avvicina con la rotellina, e a Mii
   *   fermo arriva un'immagine supercampionata dai contorni lisci. Passando
   *   il mouse su un tratto lo si vede già addosso al Mii, prima di sceglierlo;
   * - le scelte di ogni tratto sono **icone**, tutte in una griglia;
   * - ogni modifica finita si annulla e si ripete (Ctrl+Z, Ctrl+Y), e
   *   chiudere con modifiche non salvate chiede cosa farne;
   * - ogni cursore prende i limiti dal backend, gli stessi che il gioco
   *   accetta: nessun controllo può produrre un Mii che il gioco rifiuta.
   *
   * Senza runtime resta il render di Mii Studio, più lento, e l'editor
   * propone di installare quello nativo.
   */
  import { tick } from 'svelte';
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';

  import * as api from '$lib/api';
  import {
    CATEGORIES,
    FACIAL_FEATURES,
    NAME_SYMBOLS,
    clampState,
    daysInMonth,
    rangeOf,
    sliderPosition,
    sliderValue,
    toLimits
  } from '$lib/mii/categories';
  import type { Category, Control, Limits } from '$lib/mii/categories';
  import { EditHistory } from '$lib/mii/history';
  import { PALETTES, iconColors, loadIcons } from '$lib/mii/icons';
  import type { IconSet, PaletteName, PartIconKind } from '$lib/mii/icons';
  import { appearanceKey, forget as forgetRenders, renderSize, renderState } from '$lib/mii/render';
  import Icon from '$lib/components/Icon.svelte';
  import MenuButton, { type MenuItem } from '$lib/components/MenuButton.svelte';
  import MiiPartIcon from '$lib/components/MiiPartIcon.svelte';
  import Switch from '$lib/components/Switch.svelte';
  import miiSilhouette from '$lib/assets/mii_silhouette.png';
  import { app } from '$lib/stores/app.svelte';
  import { operations } from '$lib/stores/operations.svelte';
  import { i18n, t, type TranslationKey } from '$lib/stores/i18n.svelte';
  import type {
    MiiBooleanField,
    MiiEditorState,
    MiiNumericField,
    MiiRendererStatus
  } from '$lib/api/types';
  import type { MiiExpression, MiiRenderKind } from '$lib/api';

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
  /** Si sta chiudendo con modifiche non salvate: si chiede cosa farne. */
  let confirmClose = $state(false);

  const current = $derived(CATEGORIES[category] ?? CATEGORIES[0]);
  const dirty = $derived(editor !== null && JSON.stringify(editor) !== original);
  const native = $derived(renderer?.nativeReady === true);
  const accent = $derived(editor ? (favorites[editor.favoriteColorIndex] ?? '#00f2ff') : '#00f2ff');

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
      history = new EditHistory(editor);
      syncHistory();
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
  // Annulla e ripeti
  // -------------------------------------------------------------------------

  let history: EditHistory<MiiEditorState> | null = null;
  let canUndo = $state(false);
  let canRedo = $state(false);

  function syncHistory() {
    canUndo = history?.canUndo ?? false;
    canRedo = history?.canRedo ?? false;
  }

  /** Registra una modifica finita: un clic, il rilascio di un cursore. */
  function commit() {
    if (!history || !editor) return;
    history.commit($state.snapshot(editor) as MiiEditorState);
    syncHistory();
  }

  function undo() {
    const state = history?.undo();
    if (state) editor = state;
    syncHistory();
  }

  function redo() {
    const state = history?.redo();
    if (state) editor = state;
    syncHistory();
  }

  // -------------------------------------------------------------------------
  // Modifica
  // -------------------------------------------------------------------------

  /**
   * Cambia un campo e riporta lo stato dentro i limiti. `record` è falso
   * mentre un cursore scorre: la modifica si registra al rilascio.
   */
  function setNumber(field: MiiNumericField, value: number, record = true) {
    if (!editor || editor[field] === value) return;
    editor = clampState({ ...editor, [field]: value }, limits);
    if (record) commit();
  }

  function setFlag(field: MiiBooleanField, value: boolean) {
    if (!editor || editor[field] === value) return;
    editor = { ...editor, [field]: value };
    commit();
  }

  function step(control: Extract<Control, { kind: 'slider' }>, delta: number) {
    if (!editor) return;
    const range = rangeOf(control.field, editor, limits);
    const position = sliderPosition(editor[control.field], range, control.invert);
    setNumber(control.field, sliderValue(position + delta, range, control.invert));
  }

  /** Doppio clic su un cursore: torna al valore con cui il Mii si è aperto. */
  function resetField(field: MiiNumericField) {
    if (!original) return;
    const initial = (JSON.parse(original) as MiiEditorState)[field];
    setNumber(field, initial);
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

  /**
   * Frecce dentro una griglia di scelte: sinistra e destra di uno, su e giù
   * di una riga. La griglia sa quante colonne ha; le si chiede a lei.
   */
  async function onChoiceKey(
    event: KeyboardEvent,
    field: MiiNumericField,
    values: number[]
  ): Promise<void> {
    if (!editor) return;
    const group = event.currentTarget as HTMLElement;
    const columns =
      getComputedStyle(group).gridTemplateColumns.split(' ').filter(Boolean).length || 1;
    const moves: Record<string, number> = {
      ArrowLeft: -1,
      ArrowRight: 1,
      ArrowUp: -columns,
      ArrowDown: columns
    };
    const move = moves[event.key];
    if (move === undefined) return;
    event.preventDefault();

    const index = values.indexOf(editor[field]);
    const next = values[Math.min(values.length - 1, Math.max(0, index + move))];
    if (next === undefined) return;
    setNumber(field, next);
    await tick();
    group.querySelector<HTMLElement>('[aria-checked="true"]')?.focus();
  }

  // -------------------------------------------------------------------------
  // Anteprima
  // -------------------------------------------------------------------------

  /** Inquadratura: ritratto o figura intera, come nel WPF. */
  let shot = $state<MiiRenderKind>('face');
  /** Rotazione attorno all'asse verticale e inclinazione, in gradi. */
  let yaw = $state(0);
  let pitch = $state(0);
  /** Ingrandimento: 1 è l'inquadratura normale. */
  let zoom = $state(1);
  let expression = $state<MiiExpression>('normal');
  let dragging = $state(false);
  /** L'anteprima gira da sola: inerzia dopo un lancio, o ritorno di fronte. */
  let animating = $state(false);
  /** Tenendo premuto "Originale" si vede il Mii com'era all'apertura. */
  let comparing = $state(false);
  /** Il tratto sotto il mouse: si vede addosso al Mii prima di sceglierlo. */
  let hover = $state<{ field: MiiNumericField; value: number } | null>(null);

  const EXPRESSIONS: { value: MiiExpression; label: TranslationKey }[] = [
    { value: 'normal', label: 'editor.expr.normal' },
    { value: 'smile', label: 'editor.expr.smile' },
    { value: 'anger', label: 'editor.expr.anger' },
    { value: 'sorrow', label: 'editor.expr.sorrow' },
    { value: 'surprise', label: 'editor.expr.surprise' },
    { value: 'blink', label: 'editor.expr.blink' },
    { value: 'open_mouth', label: 'editor.expr.openMouth' }
  ];

  const MAX_PITCH = 25;
  const MIN_ZOOM = 0.7;
  const MAX_ZOOM = 1.8;

  /** Il Mii che l'anteprima mostra adesso. */
  const shown = $derived.by((): MiiEditorState | null => {
    if (!editor) return null;
    if (comparing && original) return JSON.parse(original) as MiiEditorState;
    if (hover && native) return clampState({ ...editor, [hover.field]: hover.value }, limits);
    return editor;
  });

  let canvas = $state<HTMLCanvasElement | null>(null);
  /** Render di Mii Studio, quando il renderer nativo non c'è. */
  let fallback = $state<string | null>(null);
  let drawn = $state(false);
  let previewStatus = $state('');

  interface PreviewJob {
    state: MiiEditorState;
    kind: MiiRenderKind;
    rotation: number;
    size: number;
    view: api.MiiPreviewView;
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

  /**
   * Il PNG va su un canvas con `createImageBitmap`: la decodifica avviene
   * prima di toccare ciò che si vede, e il fotogramma nuovo sostituisce il
   * vecchio senza lampi (§D-095).
   */
  async function draw(image: Blob) {
    const bitmap = await createImageBitmap(image);
    const target = canvas;
    const context = target?.getContext('2d');
    if (target && context) {
      context.clearRect(0, 0, target.width, target.height);
      context.imageSmoothingQuality = 'high';
      context.drawImage(bitmap, 0, 0, target.width, target.height);
      drawn = true;
    }
    bitmap.close();
  }

  async function pump() {
    if (pumping) return;
    pumping = true;
    try {
      while (pending) {
        const job = pending;
        pending = null;
        const image = await api
          .renderMiiPreview(job.state, job.kind, job.rotation, job.size, job.view)
          .catch(() => null);
        if (image) {
          await draw(image);
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
  const STAGE_PIXELS = 340;
  const canvasPixels = $derived(renderSize(STAGE_PIXELS));

  let lastJobKey = '';

  $effect(() => {
    const state = shown;
    const kind = shot;
    const rotation = Math.round(yaw);
    const tilt = Math.round(pitch);
    const magnify = Math.round(zoom * 100) / 100;
    const face = expression;
    const moving = dragging || animating;
    const hovering = hover !== null;
    const useNative = native;
    if (!state) return;

    const snapshot = $state.snapshot(state) as MiiEditorState;
    // Mentre il Mii si muove basta un render da metà lato, senza
    // supercampionamento: arriva in pochi millisecondi. Fermo, quello pieno
    // e liscio. Passando sulle scelte, lato pieno ma senza supercampionare.
    const full = canvasPixels;
    const size = moving ? Math.max(128, full / 2) : full;
    const quality = moving || hovering ? 'draft' : 'final';

    // Scrivere il nome non cambia la faccia: nessun render.
    const key = `${useNative}:${kind}:${rotation}:${tilt}:${magnify}:${face}:${size}:${quality}:${appearanceKey(snapshot)}`;
    if (key === lastJobKey) return;
    lastJobKey = key;

    if (useNative) {
      pending = {
        state: snapshot,
        kind,
        rotation,
        size,
        view: { pitch: tilt, zoom: magnify, expression: face, quality }
      };
      void pump();
      return;
    }

    // Senza renderer nativo ogni render è una richiesta a Mii Studio: si
    // aspettano 260 ms dall'ultima modifica, come `QueuePreviewRender`.
    previewStatus = t('editor.queued');
    let alive = true;
    const timer = setTimeout(() => {
      if (!alive) return;
      if (!fallback) previewStatus = t('editor.rendering');
      void renderState(snapshot, kind, rotation, size).then((image) => {
        if (!alive) return;
        fallback = image;
        previewStatus = image ? '' : t('editor.noRenderer');
      });
    }, 260);

    return () => {
      alive = false;
      clearTimeout(timer);
    };
  });

  // --- Trascinamento, inerzia, zoom ----------------------------------------

  const YAW_SENSITIVITY = 0.8;
  const PITCH_SENSITIVITY = 0.35;
  let dragStart = { x: 0, y: 0, yaw: 0, pitch: 0 };
  /** Ultimi campioni del trascinamento, per la velocità del lancio. */
  let samples: { x: number; time: number }[] = [];
  let frame = 0;

  const reducedMotion =
    typeof window !== 'undefined' &&
    window.matchMedia?.('(prefers-reduced-motion: reduce)').matches === true;

  function stopAnimation() {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    animating = false;
  }

  function onPointerDown(event: PointerEvent) {
    if (!native || event.button !== 0) return;
    stopAnimation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    dragging = true;
    dragStart = { x: event.clientX, y: event.clientY, yaw, pitch };
    samples = [{ x: event.clientX, time: event.timeStamp }];
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging) return;
    yaw = wrapDegrees(dragStart.yaw + (event.clientX - dragStart.x) * YAW_SENSITIVITY);
    pitch = clamp(
      dragStart.pitch + (event.clientY - dragStart.y) * PITCH_SENSITIVITY,
      -MAX_PITCH,
      MAX_PITCH
    );
    samples = [
      ...samples.filter((sample) => event.timeStamp - sample.time < 90),
      {
        x: event.clientX,
        time: event.timeStamp
      }
    ];
  }

  /** Al rilascio il Mii continua a girare e rallenta, se è stato lanciato. */
  function onPointerUp(event: PointerEvent) {
    if (!dragging) return;
    dragging = false;

    const first = samples[0];
    const elapsed = first ? event.timeStamp - first.time : 0;
    if (reducedMotion || !first || elapsed <= 0) return;
    let velocity = ((event.clientX - first.x) / elapsed) * YAW_SENSITIVITY; // gradi al ms
    if (Math.abs(velocity) < 0.25) return;

    animating = true;
    let last = performance.now();
    const spin = (now: number) => {
      const delta = now - last;
      last = now;
      yaw = wrapDegrees(yaw + velocity * delta);
      velocity *= Math.pow(0.94, delta / 16);
      if (Math.abs(velocity) < 0.02) {
        stopAnimation();
        return;
      }
      frame = requestAnimationFrame(spin);
    };
    frame = requestAnimationFrame(spin);
  }

  /** Rimette il Mii di fronte, con un breve movimento. */
  function faceFront() {
    stopAnimation();
    if (reducedMotion) {
      yaw = 0;
      pitch = 0;
      zoom = 1;
      return;
    }
    const from = { yaw, pitch, zoom };
    const start = performance.now();
    animating = true;
    const ease = (now: number) => {
      const progress = Math.min(1, (now - start) / 260);
      const eased = 1 - Math.pow(1 - progress, 3);
      yaw = from.yaw * (1 - eased);
      pitch = from.pitch * (1 - eased);
      zoom = from.zoom + (1 - from.zoom) * eased;
      if (progress < 1) frame = requestAnimationFrame(ease);
      else stopAnimation();
    };
    frame = requestAnimationFrame(ease);
  }

  /**
   * La rotellina avvicina e allontana. Si ascolta a mano per poter fermare lo
   * scorrimento della pagina: un ascoltatore passivo non potrebbe.
   */
  function wheelZoom(node: HTMLElement) {
    const onWheel = (event: WheelEvent) => {
      if (!native) return;
      event.preventDefault();
      zoom = clamp(zoom * (1 - event.deltaY * 0.0012), MIN_ZOOM, MAX_ZOOM);
    };
    node.addEventListener('wheel', onWheel, { passive: false });
    return { destroy: () => node.removeEventListener('wheel', onWheel) };
  }

  function onStageKey(event: KeyboardEvent) {
    if (!native) return;
    if (event.key === 'ArrowLeft') yaw = wrapDegrees(yaw - 15);
    else if (event.key === 'ArrowRight') yaw = wrapDegrees(yaw + 15);
    else if (event.key === 'ArrowUp') pitch = clamp(pitch - 5, -MAX_PITCH, MAX_PITCH);
    else if (event.key === 'ArrowDown') pitch = clamp(pitch + 5, -MAX_PITCH, MAX_PITCH);
    else if (event.key === '+' || event.key === '=') zoom = clamp(zoom + 0.1, MIN_ZOOM, MAX_ZOOM);
    else if (event.key === '-') zoom = clamp(zoom - 0.1, MIN_ZOOM, MAX_ZOOM);
    else if (event.key === 'Home') faceFront();
    else return;
    event.preventDefault();
  }

  function wrapDegrees(value: number): number {
    return ((((value + 180) % 360) + 360) % 360) - 180;
  }

  function clamp(value: number, min: number, max: number): number {
    return Math.min(max, Math.max(min, value));
  }

  $effect(() => () => stopAnimation());

  async function installNative() {
    installing = true;
    try {
      renderer = await operations.run('mii-renderer', () => api.installMiiRenderer());
      if (renderer.nativeReady) {
        // Le facce "non riuscite" della cache di sessione ora riescono.
        forgetRenders();
        lastJobKey = '';
        app.toast(t('editor.installed'), t('editor.nativeReady'), 'success');
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
    hover = null;
  }

  /** Frecce su e giù nella barra delle categorie. */
  async function onRailKey(event: KeyboardEvent) {
    const move = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;
    if (!move) return;
    event.preventDefault();
    selectCategory(category + move);
    await tick();
    (event.currentTarget as HTMLElement)
      .querySelector<HTMLElement>('[aria-selected="true"]')
      ?.focus();
  }

  /** Il segno di una categoria nella barra: l'icona del tratto scelto. */
  function railIcon(item: Category) {
    if (!item.icon || !icons || !editor) return null;
    return icons[item.icon.kind][editor[item.icon.field]] ?? null;
  }

  // -------------------------------------------------------------------------
  // Nome e data
  // -------------------------------------------------------------------------

  /** Inserisce un simbolo nel nome, come `InsertNameSymbol`. */
  function insertSymbol(symbol: string) {
    if (!editor) return;

    const input = nameInput;
    const start = input?.selectionStart ?? editor.name.length;
    const end = input?.selectionEnd ?? start;
    const next = editor.name.slice(0, start) + symbol + editor.name.slice(end);

    editor.name = [...next].slice(0, 10).join('');
    commit();
    symbolsOpen = false;
    input?.focus();
  }

  const nameLength = $derived(editor ? [...editor.name].length : 0);

  /** I giorni del mese scelto: febbraio non arriva al 31. */
  const days = $derived(
    Array.from({ length: editor ? daysInMonth(editor.birthMonth) : 31 }, (_, index) => index + 1)
  );

  const months = $derived(
    Array.from({ length: 12 }, (_, index) =>
      new Intl.DateTimeFormat(i18n.tag, { month: 'long' }).format(new Date(2024, index, 1))
    )
  );

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
      commit();
    } catch (err) {
      app.toast(t('editor.randomFailed'), api.errorMessage(err), 'warning');
    } finally {
      busy = false;
    }
  }

  function reset() {
    if (!original) return;
    editor = JSON.parse(original) as MiiEditorState;
    commit();
  }

  async function persist() {
    if (!editor) return;
    if (!editor.name.trim()) {
      error = t('editor.needName');
      confirmClose = false;
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
      confirmClose = false;
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

  const moreActions = $derived<MenuItem[]>([
    { label: t('editor.random'), icon: 'swap', disabled: busy, onselect: () => void randomize() },
    {
      label: t('editor.resetAll'),
      icon: 'refresh',
      disabled: busy || !dirty,
      onselect: reset
    },
    ...(miiId
      ? [
          {
            label: t('editor.exportFile'),
            icon: 'save' as const,
            disabled: busy,
            onselect: () => void exportMii()
          }
        ]
      : [])
  ]);

  /** Chiudere con modifiche non salvate chiede prima cosa farne. */
  function close() {
    if (busy) return;
    if (dirty) {
      confirmClose = true;
      return;
    }
    onclose(false);
  }

  function discard() {
    confirmClose = false;
    onclose(false);
  }

  function isTextField(target: EventTarget | null): boolean {
    return target instanceof HTMLInputElement && target.type === 'text';
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (symbolsOpen) symbolsOpen = false;
      else if (confirmClose) confirmClose = false;
      else close();
      return;
    }

    // Ctrl+Z e Ctrl+Y dell'editor; in un campo di testo resta quello del
    // campo, che annulla le lettere.
    if (!(event.ctrlKey || event.metaKey) || isTextField(event.target)) return;
    const key = event.key.toLowerCase();
    if (key === 'z' && !event.shiftKey) undo();
    else if (key === 'y' || (key === 'z' && event.shiftKey)) redo();
    else if (key === 's') void persist();
    else return;
    event.preventDefault();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="overlay">
  <button class="backdrop" aria-label={t('editor.close')} onclick={close} disabled={busy}></button>

  <div
    class="sheet vk-rainbow-top"
    role="dialog"
    aria-modal="true"
    aria-label={miiId ? t('editor.edit') : t('editor.new')}
  >
    <header class="head">
      <div class="head-id">
        <p class="vk-eyebrow">{miiId ? t('editor.edit') : t('editor.new')}</p>
        <h2 class="head-title">{editor?.name.trim() || 'Mii'}</h2>
      </div>

      <div class="head-actions">
        <button
          class="vk-btn icon-btn"
          onclick={undo}
          disabled={!canUndo || busy}
          title={t('editor.undo')}
          aria-label={t('editor.undo')}
        >
          <Icon name="undo" size={16} />
        </button>
        <button
          class="vk-btn icon-btn"
          onclick={redo}
          disabled={!canRedo || busy}
          title={t('editor.redo')}
          aria-label={t('editor.redo')}
        >
          <Icon name="redo" size={16} />
        </button>
        <MenuButton items={moreActions} label={t('common.more')} />
        <span class="divider" aria-hidden="true"></span>
        <button class="vk-btn" onclick={close} disabled={busy}>{t('common.cancel')}</button>
        <button
          class="vk-btn vk-btn--primary save"
          onclick={persist}
          disabled={busy || !dirty || !editor}
          title={t('editor.saveHint')}
        >
          <Icon name="save" size={15} />
          {busy ? t('common.saving') : t('common.save')}
        </button>
      </div>
    </header>

    {#if loading}
      <div class="vk-skeleton loading"></div>
    {:else if !editor}
      <div class="vk-error">{error || t('editor.unreadable')}</div>
    {:else}
      {#if error}
        <p class="vk-error inline">{error}</p>
      {/if}

      <div class="body">
        <!-- ── Categorie ────────────────────────────────────────────── -->
        <div
          class="rail"
          role="tablist"
          aria-orientation="vertical"
          aria-label={t('editor.categories')}
          tabindex="-1"
          onkeydown={onRailKey}
        >
          {#each CATEGORIES as item, index (item.key)}
            {@const glyph = railIcon(item)}
            <button
              class="rail-item"
              class:active={category === index}
              role="tab"
              aria-selected={category === index}
              tabindex={category === index ? 0 : -1}
              title={t(item.hint)}
              onclick={() => selectCategory(index)}
            >
              <span class="rail-tile">
                {#if glyph}
                  <MiiPartIcon icon={glyph} colors={iconPalette[item.icon!.kind] ?? []} />
                {:else if item.key === 'colors'}
                  <span class="rail-color" style="--swatch: {accent}"></span>
                {:else if item.key === 'mole'}
                  <span class="rail-mole"></span>
                {:else}
                  <img src={miiSilhouette} alt="" draggable="false" />
                {/if}
              </span>
              <span class="rail-label">{t(item.label)}</span>
            </button>
          {/each}
        </div>

        <!-- ── Il Mii ───────────────────────────────────────────────── -->
        <section class="stage-col">
          <!-- L'anteprima è un cursore: frecce e trascinamento girano il Mii. -->
          <div
            class="stage"
            class:rotatable={native}
            class:dragging
            class:comparing
            style="--accent: {accent}"
            role="slider"
            aria-label={t('editor.previewOf', { name: editor.name })}
            aria-valuemin={-180}
            aria-valuemax={180}
            aria-valuenow={Math.round(yaw)}
            aria-valuetext={`${Math.round(yaw)}°`}
            aria-disabled={!native}
            title={native ? t('editor.stageHint') : undefined}
            tabindex="0"
            use:wheelZoom
            onpointerdown={onPointerDown}
            onpointermove={onPointerMove}
            onpointerup={onPointerUp}
            onpointercancel={onPointerUp}
            ondblclick={faceFront}
            onkeydown={onStageKey}
          >
            <span class="floor" aria-hidden="true"></span>
            {#if native}
              <canvas
                bind:this={canvas}
                width={canvasPixels}
                height={canvasPixels}
                class:hidden={!drawn}
              ></canvas>
              {#if !drawn}
                <img class="silhouette" src={miiSilhouette} alt="" draggable="false" />
              {/if}
            {:else if fallback}
              <img src={fallback} alt="" draggable="false" />
            {:else}
              <img class="silhouette" src={miiSilhouette} alt="" draggable="false" />
            {/if}

            {#if comparing}
              <span class="stage-badge">{t('editor.original')}</span>
            {/if}
          </div>

          <div class="stage-bar">
            <div class="segmented" role="radiogroup" aria-label={t('editor.shot')}>
              <button
                role="radio"
                aria-checked={shot === 'face'}
                class:active={shot === 'face'}
                onclick={() => (shot = 'face')}
              >
                {t('editor.shotFace')}
              </button>
              <button
                role="radio"
                aria-checked={shot === 'all_body'}
                class:active={shot === 'all_body'}
                onclick={() => (shot = 'all_body')}
              >
                {t('editor.shotBody')}
              </button>
            </div>

            {#if native}
              <select
                class="vk-input expression"
                bind:value={expression}
                aria-label={t('editor.expression')}
                title={t('editor.expressionHint')}
              >
                {#each EXPRESSIONS as item (item.value)}
                  <option value={item.value}>{t(item.label)}</option>
                {/each}
              </select>
              <button
                class="vk-btn icon-btn"
                onclick={faceFront}
                disabled={yaw === 0 && pitch === 0 && zoom === 1}
                title={t('editor.resetView')}
                aria-label={t('editor.resetView')}
              >
                <Icon name="refresh" size={14} />
              </button>
            {/if}
            <button
              class="vk-btn icon-btn"
              class:active={comparing}
              disabled={!dirty}
              title={t('editor.compare')}
              aria-label={t('editor.compare')}
              aria-pressed={comparing}
              onpointerdown={() => (comparing = true)}
              onpointerup={() => (comparing = false)}
              onpointerleave={() => (comparing = false)}
              onkeydown={(event) => {
                if (event.key === ' ' || event.key === 'Enter') comparing = true;
              }}
              onkeyup={() => (comparing = false)}
            >
              <Icon name="swap" size={14} />
            </button>
          </div>

          {#if previewStatus}
            <p class="vk-faint status">{previewStatus}</p>
          {/if}

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

          <div class="identity">
            <div class="field">
              <span class="field-head">
                <span class="vk-eyebrow">{t('editor.name')}</span>
                <span class="counter" class:full={nameLength >= 10}>{nameLength}/10</span>
              </span>
              <div class="name-row">
                <input
                  class="vk-input"
                  maxlength="10"
                  bind:value={editor.name}
                  bind:this={nameInput}
                  onchange={commit}
                />
                <button
                  class="vk-btn symbol-btn"
                  title={t('editor.insertSymbol')}
                  aria-label={t('editor.insertSymbol')}
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
              <input
                class="vk-input"
                maxlength="10"
                bind:value={editor.creatorName}
                onchange={commit}
              />
            </label>
          </div>
        </section>

        <!-- ── Controlli della categoria ─────────────────────────────── -->
        <div class="panel vk-card" role="tabpanel" aria-label={t(current.label)}>
          <header class="panel-head">
            <h3 class="panel-title">{t(current.label)}</h3>
            <p class="vk-faint panel-hint">{t(current.hint)}</p>
          </header>

          {#each current.controls as control (control.field)}
            {#if control.kind === 'parts'}
              {@const values = valuesOf(control.field, editor)}
              <section class="group">
                <p class="group-title vk-eyebrow">{t(control.label)}</p>
                <div
                  class="parts"
                  role="radiogroup"
                  aria-label={t(control.label)}
                  tabindex="-1"
                  onkeydown={(event) => onChoiceKey(event, control.field, values)}
                >
                  {#each values as value (value)}
                    {@const glyph = icons?.[control.icon][value]}
                    {@const selected = editor[control.field] === value}
                    <button
                      class="part"
                      class:selected
                      role="radio"
                      aria-checked={selected}
                      tabindex={selected ? 0 : -1}
                      title={`${t(control.label)} ${value + 1}`}
                      onclick={() => setNumber(control.field, value)}
                      onpointerenter={() => (hover = { field: control.field, value })}
                      onpointerleave={() => (hover = null)}
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
              {@const values = valuesOf(control.field, editor)}
              <section class="group">
                <p class="group-title vk-eyebrow">{t(control.label)}</p>
                <div
                  class="swatches"
                  role="radiogroup"
                  aria-label={t(control.label)}
                  tabindex="-1"
                  onkeydown={(event) => onChoiceKey(event, control.field, values)}
                >
                  {#each values as value (value)}
                    {@const selected = editor[control.field] === value}
                    <button
                      class="swatch"
                      class:selected
                      role="radio"
                      aria-checked={selected}
                      tabindex={selected ? 0 : -1}
                      aria-label={`${t(control.label)} ${value + 1}`}
                      title={`${t(control.label)} ${value + 1}`}
                      style="--swatch: {palette[value] ?? '#000'}"
                      onclick={() => setNumber(control.field, value)}
                      onpointerenter={() => (hover = { field: control.field, value })}
                      onpointerleave={() => (hover = null)}
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
                <div class="segmented wide" role="radiogroup" aria-label={t(control.label)}>
                  {#each [false, true] as value (value)}
                    <button
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
              <div class="toggle">
                <span>{t(control.label)}</span>
                <Switch
                  checked={editor[control.field]}
                  label={t(control.label)}
                  onchange={(next) => setFlag(control.field, next)}
                />
              </div>
            {:else if control.kind === 'date'}
              <section class="group">
                <p class="group-title vk-eyebrow">{t(control.label)}</p>
                <div class="date">
                  <select
                    class="vk-input"
                    aria-label={t('miicat.birthMonth')}
                    value={editor.birthMonth}
                    onchange={(event) => setNumber('birthMonth', Number(event.currentTarget.value))}
                  >
                    {#each months as month, index (index)}
                      <option value={index + 1}>{month}</option>
                    {/each}
                  </select>
                  <select
                    class="vk-input day"
                    aria-label={t('miicat.birthDay')}
                    value={editor.birthDay}
                    onchange={(event) => setNumber('birthDay', Number(event.currentTarget.value))}
                  >
                    {#each days as day (day)}
                      <option value={day}>{day}</option>
                    {/each}
                  </select>
                </div>
              </section>
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
                    title={t('editor.sliderHint')}
                    style="--fill: {((position - range.min) / Math.max(1, range.max - range.min)) *
                      100}%"
                    oninput={(event) =>
                      setNumber(
                        control.field,
                        sliderValue(Number(event.currentTarget.value), range, control.invert),
                        false
                      )}
                    onchange={commit}
                    ondblclick={() => resetField(control.field)}
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
    {/if}

    {#if confirmClose}
      <div class="confirm-layer">
        <div
          class="confirm"
          role="alertdialog"
          aria-modal="true"
          aria-label={t('editor.discardTitle')}
        >
          <h3>{t('editor.discardTitle')}</h3>
          <p class="vk-faint">
            {t('editor.discardBody', { name: editor?.name.trim() || 'Mii' })}
          </p>
          <div class="confirm-actions">
            <button class="vk-btn vk-btn--danger" onclick={discard}>{t('editor.discard')}</button>
            <span class="vk-spacer"></span>
            <button class="vk-btn" onclick={() => (confirmClose = false)}>
              {t('editor.keepEditing')}
            </button>
            <button class="vk-btn vk-btn--primary" onclick={persist} disabled={busy}>
              <Icon name="save" size={14} />
              {t('common.save')}
            </button>
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
    width: min(1260px, 100%);
    height: min(880px, 100%);
    padding: 18px 20px 20px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel);
    box-shadow: var(--vk-shadow-modal);
    overflow: hidden;
  }

  /* ---- Testata ---- */

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding-bottom: 14px;
  }

  .head-id {
    min-width: 0;
  }

  .head-id .vk-eyebrow {
    margin: 0;
  }

  .head-title {
    margin: 2px 0 0;
    font-size: 22px;
    font-weight: 900;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }

  .divider {
    width: 1px;
    height: 24px;
    margin: 0 4px;
    background: var(--vk-stroke);
  }

  .icon-btn {
    padding: 8px 10px;
  }

  .icon-btn.active {
    border-color: var(--vk-cyan);
    color: var(--vk-cyan-soft);
  }

  .save {
    min-width: 120px;
  }

  .loading {
    height: 100%;
  }

  .inline {
    margin: 0 0 12px;
    padding: 10px 12px;
    font-size: var(--vk-fs-micro);
  }

  .body {
    display: grid;
    grid-template-columns: 92px 360px minmax(0, 1fr);
    gap: 16px;
    min-height: 0;
    flex: 1;
  }

  /* ---- Categorie ---- */

  .rail {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    overflow-y: auto;
    padding-right: 2px;
  }

  .rail-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 5px 4px;
    border: 1px solid transparent;
    border-radius: var(--vk-radius-badge);
    background: transparent;
    color: var(--vk-text-secondary);
    cursor: pointer;
    transition:
      background var(--vk-dur-fast) var(--vk-ease),
      border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .rail-item:hover {
    background: rgb(255 255 255 / 0.04);
    color: var(--vk-text);
  }

  .rail-item.active {
    border-color: transparent;
    background:
      linear-gradient(var(--vk-active-surface), var(--vk-active-surface)) padding-box,
      var(--vk-rainbow) border-box;
    color: var(--vk-text);
  }

  /* Le icone stanno su un fondo chiaro, come nel Canale Mii: un sopracciglio
     nero su una tessera scura non si vedrebbe. */
  .rail-tile {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 4px;
    border-radius: 10px;
    background: #dfe6f3;
  }

  .rail-tile :global(svg),
  .rail-tile img {
    width: 100%;
    height: 100%;
  }

  .rail-tile img {
    object-fit: contain;
    opacity: 0.55;
  }

  .rail-color {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--swatch);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.25);
  }

  .rail-mole {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #3b2a24;
  }

  .rail-label {
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    text-align: center;
    line-height: 1.15;
  }

  /* ---- Il Mii ---- */

  .stage-col {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    overflow-y: auto;
  }

  .stage {
    position: relative;
    display: grid;
    place-items: center;
    width: 340px;
    height: 340px;
    margin: 0 auto;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    /* Un palco con la luce del colore preferito del Mii. */
    background:
      radial-gradient(
        circle at 50% 38%,
        color-mix(in srgb, var(--accent) 26%, transparent),
        transparent 62%
      ),
      linear-gradient(180deg, #16223a, #0c1322);
    outline: none;
    touch-action: none;
    user-select: none;
    overflow: hidden;
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .stage.rotatable {
    cursor: grab;
  }

  .stage.dragging {
    cursor: grabbing;
  }

  .stage.comparing {
    border-color: var(--vk-warning);
  }

  .stage:focus-visible {
    box-shadow: 0 0 0 2px var(--vk-cyan);
  }

  .floor {
    position: absolute;
    left: 50%;
    bottom: 22px;
    width: 58%;
    height: 26px;
    border-radius: 50%;
    background: radial-gradient(closest-side, rgb(0 0 0 / 0.5), transparent);
    transform: translateX(-50%);
    pointer-events: none;
  }

  .stage canvas,
  .stage img {
    position: relative;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }

  .stage canvas.hidden {
    position: absolute;
    visibility: hidden;
  }

  .silhouette {
    opacity: 0.3;
    transform: scale(0.6);
  }

  .stage-badge {
    position: absolute;
    top: 10px;
    left: 10px;
    padding: 3px 9px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--vk-warning) 22%, #0c1322);
    color: var(--vk-warning);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 900;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .stage-bar {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .segmented {
    display: inline-flex;
    padding: 3px;
    border: 1px solid var(--vk-stroke);
    border-radius: 999px;
    background: var(--vk-input);
  }

  .segmented button {
    padding: 5px 12px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--vk-text-secondary);
    font: inherit;
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    cursor: pointer;
  }

  .segmented button.active {
    background: var(--vk-active-surface);
    color: var(--vk-text);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--vk-cyan) 40%, transparent);
  }

  .segmented.wide button {
    padding: 7px 22px;
  }

  .expression {
    width: auto;
    padding: 6px 8px;
    font-size: var(--vk-fs-micro);
  }

  .status {
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

  .identity {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 4px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .field-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .counter {
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    color: var(--vk-text-faint);
    font-variant-numeric: tabular-nums;
  }

  .counter.full {
    color: var(--vk-warning);
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
    grid-template-columns: repeat(9, 1fr);
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

  /* ---- Controlli ---- */

  .panel {
    min-height: 0;
    overflow-y: auto;
  }

  .panel-head {
    margin-bottom: 4px;
  }

  .panel-title {
    margin: 0;
    font-size: 20px;
    font-weight: 900;
  }

  .panel-hint {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .group {
    margin-top: 16px;
  }

  .group-title {
    margin: 0 0 8px;
  }

  .parts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(58px, 1fr));
    gap: 8px;
    outline: none;
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

  .part:focus-visible {
    outline: 2px solid var(--vk-cyan);
    outline-offset: 2px;
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
    display: grid;
    grid-template-columns: repeat(auto-fill, 38px);
    gap: 8px;
    outline: none;
  }

  .swatch {
    width: 38px;
    height: 38px;
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

  .swatch:focus-visible {
    outline: 2px solid var(--vk-cyan);
    outline-offset: 2px;
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

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    max-width: 520px;
    margin-top: 16px;
    padding: 10px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    font-size: var(--vk-fs-small);
    font-weight: 700;
  }

  .date {
    display: flex;
    gap: 8px;
    max-width: 360px;
  }

  .date select {
    flex: 1;
  }

  .date .day {
    flex: 0 0 90px;
  }

  .slider {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: 520px;
    margin-top: 14px;
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
    font-variant-numeric: tabular-nums;
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

  /* Il cursore si riempie di arcobaleno fino al valore scelto. */
  .slider input {
    width: 100%;
    height: 6px;
    border-radius: 999px;
    background:
      linear-gradient(90deg, var(--vk-cyan), #ff0066) 0 0 / var(--fill) 100% no-repeat,
      var(--vk-input);
    accent-color: var(--vk-cyan);
    appearance: none;
    cursor: pointer;
  }

  .slider input::-webkit-slider-thumb {
    width: 16px;
    height: 16px;
    border: 2px solid #fff;
    border-radius: 50%;
    background: var(--vk-cyan);
    box-shadow: 0 0 10px rgb(0 242 255 / 0.5);
    appearance: none;
  }

  /* ---- Chiusura con modifiche ---- */

  .confirm-layer {
    position: absolute;
    inset: 0;
    z-index: 5;
    display: grid;
    place-items: center;
    background: rgb(4 7 14 / 0.6);
    animation: fade var(--vk-dur-fast) var(--vk-ease);
  }

  .confirm {
    width: min(460px, calc(100% - 40px));
    padding: 20px 22px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-card);
    background: var(--vk-panel);
    box-shadow: var(--vk-shadow-modal);
  }

  .confirm h3 {
    margin: 0;
    font-size: 18px;
    font-weight: 900;
  }

  .confirm p {
    margin: 8px 0 0;
    font-size: var(--vk-fs-small);
  }

  .confirm-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 18px;
  }

  @media (max-width: 1080px) {
    .body {
      grid-template-columns: 80px minmax(0, 1fr);
      overflow-y: auto;
    }

    .panel {
      grid-column: 1 / -1;
      overflow: visible;
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

  @media (prefers-reduced-motion: reduce) {
    .overlay,
    .confirm-layer {
      animation: none;
    }

    .part:hover,
    .swatch:hover {
      transform: none;
    }
  }
</style>
