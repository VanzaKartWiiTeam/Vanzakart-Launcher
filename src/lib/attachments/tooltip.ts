/**
 * Suggerimento del design system, al posto dell'attributo `title`.
 *
 * Il `title` lo disegna il sistema operativo: un rettangolino giallo o grigio
 * che arriva dopo un secondo, con il font di sistema, uguale in ogni
 * programma. Questo è un elemento del launcher, con i suoi colori.
 *
 * ```svelte
 * <button {@attach tooltip(t('mods.repairHint'))}>…</button>
 * ```
 *
 * - compare dopo una breve attesa col mouse, subito col fuoco da tastiera, e
 *   senza attesa passando da un elemento all'altro;
 * - sparisce uscendo, premendo, scorrendo o con Esc;
 * - mentre è visibile l'elemento lo cita in `aria-describedby`, così chi usa
 *   un lettore di schermo lo sente come faceva con `title`;
 * - uno solo per tutta l'app, in `document.body`, con `position: fixed`: non
 *   lo taglia nessun contenitore con `overflow`.
 */
import type { Attachment } from 'svelte/attachments';

/** Attesa col mouse prima di mostrarlo. */
const SHOW_DELAY = 450;
/** Entro questo tempo dall'ultimo nascosto, il successivo compare subito. */
const WARM_WINDOW = 500;
/** Distanza dall'elemento e dai bordi della finestra. */
const GAP = 8;
const MARGIN = 8;
const ID = 'vk-tooltip';

let bubble: HTMLDivElement | null = null;
let owner: HTMLElement | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
/** L'elemento che aspetta di mostrarlo. */
let scheduled: HTMLElement | null = null;
let hiddenAt = 0;
/** L'elemento che lo mostrava mentre il testo cambiava: lo riprende subito. */
let restoring: HTMLElement | null = null;

function element(): HTMLDivElement {
  if (bubble?.isConnected) return bubble;
  bubble = document.createElement('div');
  bubble.id = ID;
  bubble.className = 'vk-tooltip';
  bubble.setAttribute('role', 'tooltip');
  document.body.append(bubble);
  return bubble;
}

function place(node: HTMLElement, tip: HTMLElement) {
  const rect = node.getBoundingClientRect();
  const { offsetWidth: width, offsetHeight: height } = tip;

  let top = rect.top - GAP - height;
  let side = 'top';
  if (top < MARGIN) {
    top = rect.bottom + GAP;
    side = 'bottom';
  }
  const centered = rect.left + rect.width / 2 - width / 2;
  const left = Math.min(Math.max(centered, MARGIN), window.innerWidth - MARGIN - width);

  tip.style.transform = `translate(${Math.round(left)}px, ${Math.round(top)}px)`;
  tip.dataset.side = side;
}

function cancel() {
  clearTimeout(timer);
  scheduled = null;
}

function show(node: HTMLElement, text: string) {
  cancel();
  if (!node.isConnected) return;

  const tip = element();
  tip.textContent = text;
  owner = node;
  place(node, tip);
  tip.dataset.open = 'true';

  // Un'icona con `aria-label` uguale al testo lo direbbe due volte.
  if (node.getAttribute('aria-label') !== text) {
    const described = node.getAttribute('aria-describedby') ?? '';
    if (!described.split(' ').includes(ID)) {
      node.setAttribute('aria-describedby', `${described} ${ID}`.trim());
    }
  }

  window.addEventListener('keydown', onEscape, true);
  window.addEventListener('scroll', hide, true);
  window.addEventListener('wheel', hide, { passive: true });
}

function hide() {
  cancel();
  if (!owner) return;

  const described = (owner.getAttribute('aria-describedby') ?? '')
    .split(' ')
    .filter((id) => id && id !== ID)
    .join(' ');
  if (described) owner.setAttribute('aria-describedby', described);
  else owner.removeAttribute('aria-describedby');

  owner = null;
  hiddenAt = performance.now();
  if (bubble) bubble.dataset.open = 'false';

  window.removeEventListener('keydown', onEscape, true);
  window.removeEventListener('scroll', hide, true);
  window.removeEventListener('wheel', hide);
}

/** Esc chiude il suggerimento ma lascia passare il tasto: può servire anche ad altro. */
function onEscape(event: KeyboardEvent) {
  if (event.key === 'Escape') hide();
}

function schedule(node: HTMLElement, text: string) {
  cancel();
  const warm = owner !== null || performance.now() - hiddenAt < WARM_WINDOW;
  if (warm) {
    show(node, text);
    return;
  }
  scheduled = node;
  timer = setTimeout(() => show(node, text), SHOW_DELAY);
}

/**
 * Suggerimento su un elemento. Senza testo non fa niente: comodo per quelli
 * che ne hanno uno solo in certi stati.
 */
export function tooltip(text: string | null | undefined): Attachment<HTMLElement> {
  return (node) => {
    if (!text) return;
    const content = text;

    const onEnter = (event: PointerEvent) => {
      if (event.pointerType !== 'touch') schedule(node, content);
    };
    const onLeave = () => {
      if (owner === node) hide();
      else if (scheduled === node) cancel();
    };
    const onFocus = () => {
      if (node.matches(':focus-visible')) show(node, content);
    };

    node.addEventListener('pointerenter', onEnter);
    node.addEventListener('pointerleave', onLeave);
    node.addEventListener('pointerdown', hide);
    node.addEventListener('focus', onFocus);
    node.addEventListener('blur', onLeave);

    if (restoring === node) {
      restoring = null;
      show(node, content);
    }

    return () => {
      node.removeEventListener('pointerenter', onEnter);
      node.removeEventListener('pointerleave', onLeave);
      node.removeEventListener('pointerdown', hide);
      node.removeEventListener('focus', onFocus);
      node.removeEventListener('blur', onLeave);
      if (owner === node) {
        // Se l'elemento resta e cambia solo il testo, il nuovo testo prende
        // il posto del vecchio senza sparire.
        restoring = node;
        queueMicrotask(() => {
          if (restoring === node) {
            restoring = null;
            hide();
          }
        });
      } else if (scheduled === node) {
        cancel();
      }
    };
  };
}
