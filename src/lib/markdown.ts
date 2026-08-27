/**
 * Parser markdown del launcher.
 *
 * Erede di `MarkdownHelper.cs`, che riconosceva `#`, `##`, `- `, `**grassetto**`
 * e `*corsivo*` e trattava ogni a capo come un a capo vero. Qui c'è tutto
 * quello, più il resto di ciò che si trova in `news.json`: titoli fino a sei
 * livelli, righe orizzontali, citazioni, blocchi di codice, elenchi numerati e
 * annidati, tabelle, link, immagini, barrato e codice inline.
 *
 * Il parser non produce **mai** HTML: restituisce blocchi e span tipizzati che
 * `Markdown.svelte` rende con elementi Svelte. È la ragione per cui una news
 * ostile non può iniettare markup nella webview. Per lo stesso motivo gli URL
 * passano da `safeUrl`: solo `http`/`https` diventano link.
 */

// ---------------------------------------------------------------------------
// Modello
// ---------------------------------------------------------------------------

/** Porzione di testo con la sua formattazione. */
export interface Span {
  text: string;
  bold: boolean;
  italic: boolean;
  strike: boolean;
  code: boolean;
  /** `true` per `![alt](url)`: `text` è l'alt, `href` la sorgente. */
  image: boolean;
  href: string | null;
}

/** Allineamento di una colonna di tabella. */
export type Align = 'left' | 'center' | 'right';

/** Blocco di primo livello. `list` e `quote` contengono altri blocchi. */
export type Block =
  | { kind: 'heading'; level: 1 | 2 | 3 | 4 | 5 | 6; spans: Span[] }
  | { kind: 'paragraph'; lines: Span[][] }
  | { kind: 'list'; ordered: boolean; start: number; items: Block[][] }
  | { kind: 'quote'; blocks: Block[] }
  | { kind: 'code'; language: string; text: string }
  | { kind: 'rule' }
  | { kind: 'table'; align: Align[]; head: Span[][]; rows: Span[][][] };

/** Formattazione ereditata dagli span annidati. */
type Style = Omit<Span, 'text' | 'image'>;

const PLAIN: Style = { bold: false, italic: false, strike: false, code: false, href: null };

// ---------------------------------------------------------------------------
// Blocchi
// ---------------------------------------------------------------------------

const HEADING = /^ {0,3}(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/;
const RULE = /^ {0,3}([-*_])[ \t]*(?:\1[ \t]*){2,}$/;
const FENCE = /^ {0,3}(`{3,}|~{3,})[ \t]*(.*)$/;
const QUOTE = /^ {0,3}>[ ]?(.*)$/;
const LIST_ITEM = /^( *)([-*+]|\d{1,9}[.)])[ \t]+(.*)$/;
const TABLE_RULE = /^ {0,3}\|?[ \t]*:?-+:?[ \t]*(\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$/;

/** Divide il testo in blocchi. */
export function parseMarkdown(source: string): Block[] {
  const lines = source
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((line) => line.replace(/^\t+/, (tabs) => '    '.repeat(tabs.length)));

  return parseBlocks(lines);
}

function parseBlocks(lines: string[]): Block[] {
  const blocks: Block[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index]!;

    if (line.trim() === '') {
      index += 1;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const taken = takeCode(lines, index, fence[1]!, fence[2]!);
      blocks.push(taken.block);
      index = taken.next;
      continue;
    }

    if (RULE.test(line)) {
      blocks.push({ kind: 'rule' });
      index += 1;
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      blocks.push({
        kind: 'heading',
        level: heading[1]!.length as 1 | 2 | 3 | 4 | 5 | 6,
        spans: parseInline(heading[2]!)
      });
      index += 1;
      continue;
    }

    if (QUOTE.test(line)) {
      const taken = takeQuote(lines, index);
      blocks.push(taken.block);
      index = taken.next;
      continue;
    }

    if (startsTable(lines, index)) {
      const taken = takeTable(lines, index);
      blocks.push(taken.block);
      index = taken.next;
      continue;
    }

    if (LIST_ITEM.test(line)) {
      const taken = takeList(lines, index);
      blocks.push(taken.block);
      index = taken.next;
      continue;
    }

    const taken = takeParagraph(lines, index);
    blocks.push(taken.block);
    index = taken.next;
  }

  return blocks;
}

/** `true` se la riga apre un blocco diverso dal paragrafo in corso. */
function opensBlock(line: string): boolean {
  return (
    line.trim() === '' ||
    RULE.test(line) ||
    HEADING.test(line) ||
    FENCE.test(line) ||
    QUOTE.test(line) ||
    LIST_ITEM.test(line)
  );
}

/**
 * Paragrafo: ogni riga resta una riga, come nel launcher WPF, dove un `\n`
 * diventava un `LineBreak`. Unirle avrebbe cambiato l'impaginazione di tutte
 * le news già pubblicate.
 */
function takeParagraph(lines: string[], start: number): { block: Block; next: number } {
  const collected: Span[][] = [];
  let index = start;

  while (index < lines.length && !opensBlock(lines[index]!) && !startsTable(lines, index)) {
    collected.push(parseInline(lines[index]!.trim()));
    index += 1;
  }

  // Difesa contro un avanzamento nullo: senza, un blocco non riconosciuto
  // farebbe girare a vuoto il chiamante.
  if (collected.length === 0) {
    collected.push(parseInline(lines[start]!.trim()));
    index = start + 1;
  }

  return { block: { kind: 'paragraph', lines: collected }, next: index };
}

function takeCode(
  lines: string[],
  start: number,
  marker: string,
  language: string
): { block: Block; next: number } {
  const body: string[] = [];
  let index = start + 1;

  while (index < lines.length) {
    const candidate = lines[index]!.trim();
    const closes =
      candidate.length >= marker.length && [...candidate].every((char) => char === marker[0]);
    if (closes) {
      index += 1;
      break;
    }

    body.push(lines[index]!);
    index += 1;
  }

  return {
    block: { kind: 'code', language: language.trim(), text: body.join('\n') },
    next: index
  };
}

function takeQuote(lines: string[], start: number): { block: Block; next: number } {
  const inner: string[] = [];
  let index = start;

  while (index < lines.length) {
    const quoted = QUOTE.exec(lines[index]!);
    if (!quoted) break;
    inner.push(quoted[1]!);
    index += 1;
  }

  return { block: { kind: 'quote', blocks: parseBlocks(inner) }, next: index };
}

function indentOf(line: string): number {
  return /^ */.exec(line)![0].length;
}

function isOrdered(marker: string): boolean {
  return /\d/.test(marker);
}

/**
 * Elenco puntato o numerato. Le righe più indentate finiscono nell'elemento
 * corrente, che viene interpretato a sua volta: è così che funzionano gli
 * elenchi annidati e i paragrafi dentro un elemento.
 */
function takeList(lines: string[], start: number): { block: Block; next: number } {
  const opening = LIST_ITEM.exec(lines[start]!)!;
  const ordered = isOrdered(opening[2]!);
  const base = indentOf(lines[start]!);
  const first = ordered ? Number.parseInt(opening[2]!, 10) : 1;

  const items: Block[][] = [];
  let current: string[] = [];
  let index = start;

  const flush = () => {
    if (current.length > 0) {
      items.push(parseBlocks(current));
      current = [];
    }
  };

  while (index < lines.length) {
    const line = lines[index]!;
    const item = LIST_ITEM.exec(line);

    if (item && indentOf(line) <= base) {
      // Un marcatore di tipo diverso apre un elenco nuovo.
      if (isOrdered(item[2]!) !== ordered) break;
      flush();
      current.push(item[3]!);
      index += 1;
      continue;
    }

    if (line.trim() === '') {
      const next = lines[index + 1];
      const continues =
        next !== undefined && next.trim() !== '' && (indentOf(next) > base || LIST_ITEM.test(next));
      if (!continues) break;
      current.push('');
      index += 1;
      continue;
    }

    if (indentOf(line) > base) {
      current.push(line.slice(Math.min(indentOf(line), base + 2)));
      index += 1;
      continue;
    }

    break;
  }

  flush();
  return { block: { kind: 'list', ordered, start: first, items }, next: index };
}

function startsTable(lines: string[], index: number): boolean {
  const header = lines[index];
  const divider = lines[index + 1];
  return (
    header !== undefined &&
    divider !== undefined &&
    header.includes('|') &&
    divider.includes('-') &&
    TABLE_RULE.test(divider)
  );
}

function splitCells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim());
}

function takeTable(lines: string[], start: number): { block: Block; next: number } {
  const head = splitCells(lines[start]!).map((cell) => parseInline(cell));
  const align: Align[] = splitCells(lines[start + 1]!).map((cell) => {
    const left = cell.startsWith(':');
    const right = cell.endsWith(':');
    if (left && right) return 'center';
    return right ? 'right' : 'left';
  });

  const rows: Span[][][] = [];
  let index = start + 2;
  while (index < lines.length && lines[index]!.includes('|') && lines[index]!.trim() !== '') {
    rows.push(splitCells(lines[index]!).map((cell) => parseInline(cell)));
    index += 1;
  }

  return { block: { kind: 'table', align, head, rows }, next: index };
}

// ---------------------------------------------------------------------------
// Testo
// ---------------------------------------------------------------------------

const ESCAPABLE = '\\`*_{}[]()#+-.!|~><';
const LINK = /^(!?)\[([^\]]*)\]\([ \t]*<?([^)<>\s]+)>?(?:[ \t]+"[^"]*")?[ \t]*\)/;
const AUTOLINK = /^<((?:https?):\/\/[^>\s]+)>/i;
const BARE_URL = /^https?:\/\/[^\s<>]+/i;

/** Delimitatori di enfasi, dal più lungo al più corto. */
const RUNS: readonly (readonly [string, Partial<Style>])[] = [
  ['***', { bold: true, italic: true }],
  ['___', { bold: true, italic: true }],
  ['**', { bold: true }],
  ['__', { bold: true }],
  ['~~', { strike: true }],
  ['*', { italic: true }],
  ['_', { italic: true }]
];

/**
 * URL utilizzabile come link, oppure `null`.
 *
 * Solo `http` e `https`: `javascript:`, `data:` e `file:` non devono mai
 * finire in un `href` costruito da un file remoto.
 */
export function safeUrl(candidate: string): string | null {
  const value = candidate.trim();
  if (value === '') return null;

  try {
    const parsed = new URL(value);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:' ? parsed.href : null;
  } catch {
    return null;
  }
}

/** Riconosce la formattazione dentro una riga. */
export function parseInline(source: string, style: Style = PLAIN): Span[] {
  const spans: Span[] = [];
  let buffer = '';
  let index = 0;

  const flush = () => {
    if (buffer !== '') {
      spans.push({ ...style, image: false, text: buffer });
      buffer = '';
    }
  };

  while (index < source.length) {
    const rest = source.slice(index);
    const char = source[index]!;

    if (char === '\\' && ESCAPABLE.includes(source[index + 1] ?? '\0')) {
      buffer += source[index + 1];
      index += 2;
      continue;
    }

    if (char === '`') {
      const marker = /^`+/.exec(rest)![0];
      const close = source.indexOf(marker, index + marker.length);
      if (close !== -1) {
        flush();
        spans.push({
          ...style,
          code: true,
          image: false,
          text: source.slice(index + marker.length, close).trim()
        });
        index = close + marker.length;
        continue;
      }
    }

    if (char === '[' || char === '!') {
      const link = LINK.exec(rest);
      const url = link ? safeUrl(link[3]!) : null;
      if (link && url) {
        flush();
        if (link[1] === '!') {
          spans.push({ ...style, image: true, href: url, text: link[2]! });
        } else {
          const label = link[2]!.trim() === '' ? url : link[2]!;
          spans.push(...parseInline(label, { ...style, href: url }));
        }
        index += link[0]!.length;
        continue;
      }
    }

    if (char === '<') {
      const auto = AUTOLINK.exec(rest);
      const url = auto ? safeUrl(auto[1]!) : null;
      if (auto && url) {
        flush();
        spans.push({ ...style, href: url, image: false, text: auto[1]! });
        index += auto[0]!.length;
        continue;
      }
    }

    if ((char === 'h' || char === 'H') && BARE_URL.test(rest)) {
      // La punteggiatura finale appartiene alla frase, non all'indirizzo.
      const raw = BARE_URL.exec(rest)![0].replace(/[.,;:!?'"*_)\]}]+$/, '');
      const url = safeUrl(raw);
      if (url) {
        flush();
        spans.push({ ...style, href: url, image: false, text: raw });
        index += raw.length;
        continue;
      }
    }

    const emphasis = matchEmphasis(rest, index > 0 ? source[index - 1]! : '');
    if (emphasis) {
      flush();
      spans.push(...parseInline(emphasis.inner, { ...style, ...emphasis.style }));
      index += emphasis.length;
      continue;
    }

    buffer += char;
    index += 1;
  }

  flush();
  return spans;
}

function matchEmphasis(
  rest: string,
  previous: string
): { inner: string; length: number; style: Partial<Style> } | null {
  for (const [token, style] of RUNS) {
    if (!rest.startsWith(token)) continue;

    // `snake_case` non è corsivo: l'underscore dentro una parola non apre nulla.
    if (token.startsWith('_') && /[\p{L}\p{N}]/u.test(previous)) continue;

    const from = token.length;
    if (/\s/.test(rest[from] ?? ' ')) continue;

    const close = findCloser(rest, token, from);
    if (close <= from) continue;
    if (/\s/.test(rest[close - 1] ?? ' ')) continue;

    return { inner: rest.slice(from, close), length: close + token.length, style };
  }

  return null;
}

/**
 * Posizione del delimitatore di chiusura.
 *
 * Cerca una sequenza **della stessa lunghezza**: senza questo, in
 * `*testo con **grassetto** dentro*` il corsivo si chiuderebbe sul primo
 * asterisco del grassetto.
 */
function findCloser(text: string, token: string, from: number): number {
  const char = token[0]!;
  let index = from;

  while (index < text.length) {
    if (text[index] !== char) {
      index += 1;
      continue;
    }

    let end = index;
    while (end < text.length && text[end] === char) end += 1;
    if (end - index === token.length) return index;
    index = end;
  }

  return -1;
}
