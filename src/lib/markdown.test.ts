import { describe, expect, it } from 'vitest';

import { parseInline, parseMarkdown, safeUrl, type Block, type Span } from './markdown';

/** Testo di tutti gli span, concatenato. */
function text(spans: Span[]): string {
  return spans.map((span) => span.text).join('');
}

/** Testo di un blocco, righe comprese, per le asserzioni di struttura. */
function blockText(block: Block): string {
  switch (block.kind) {
    case 'heading':
      return text(block.spans);
    case 'paragraph':
      return block.lines.map(text).join('\n');
    case 'code':
      return block.text;
    case 'quote':
      return block.blocks.map(blockText).join('\n');
    case 'list':
      return block.items.map((item) => item.map(blockText).join('\n')).join('\n');
    case 'table':
      return [block.head, ...block.rows].map((row) => row.map(text).join('|')).join('\n');
    case 'rule':
      return '---';
  }
}

describe('parseMarkdown', () => {
  it('riconosce i sei livelli di titolo', () => {
    const blocks = parseMarkdown('# Uno\n## Due\n### Tre\n#### Quattro\n##### Cinque\n###### Sei');

    expect(blocks).toHaveLength(6);
    expect(blocks.map((block) => (block.kind === 'heading' ? block.level : 0))).toEqual([
      1, 2, 3, 4, 5, 6
    ]);
  });

  it('richiede lo spazio dopo i cancelletti', () => {
    expect(parseMarkdown('#NoSpazio')[0]?.kind).toBe('paragraph');
  });

  it('tiene ogni riga del paragrafo come riga a sé, come il launcher WPF', () => {
    const blocks = parseMarkdown('prima riga\nseconda riga\n\naltro paragrafo');

    expect(blocks).toHaveLength(2);
    expect(blocks[0]?.kind).toBe('paragraph');
    if (blocks[0]?.kind === 'paragraph') {
      expect(blocks[0].lines).toHaveLength(2);
      expect(text(blocks[0].lines[1]!)).toBe('seconda riga');
    }
  });

  it('raggruppa gli elenchi puntati in un solo blocco', () => {
    const blocks = parseMarkdown('- uno\n- due\n* tre\n+ quattro');

    expect(blocks).toHaveLength(1);
    expect(blocks[0]?.kind).toBe('list');
    if (blocks[0]?.kind === 'list') {
      expect(blocks[0].ordered).toBe(false);
      expect(blocks[0].items).toHaveLength(4);
      expect(blockText(blocks[0].items[3]![0]!)).toBe('quattro');
    }
  });

  it('separa elenchi numerati e puntati e ricorda il primo numero', () => {
    const blocks = parseMarkdown('3. tre\n4. quattro\n- puntato');

    expect(blocks.map((block) => block.kind)).toEqual(['list', 'list']);
    if (blocks[0]?.kind === 'list') {
      expect(blocks[0].ordered).toBe(true);
      expect(blocks[0].start).toBe(3);
      expect(blocks[0].items).toHaveLength(2);
    }
  });

  it('annida gli elenchi in base all indentazione', () => {
    const blocks = parseMarkdown('- padre\n  - figlio\n- zio');

    expect(blocks).toHaveLength(1);
    if (blocks[0]?.kind === 'list') {
      expect(blocks[0].items).toHaveLength(2);
      expect(blocks[0].items[0]!.map((block) => block.kind)).toEqual(['paragraph', 'list']);
    }
  });

  it('chiude elenco e paragrafo quando cambia il contesto', () => {
    const blocks = parseMarkdown('testo\n- voce\ndopo');

    expect(blocks.map((block) => block.kind)).toEqual(['paragraph', 'list', 'paragraph']);
  });

  it('riconosce la riga orizzontale e non la scambia per un elenco', () => {
    expect(parseMarkdown('---')[0]?.kind).toBe('rule');
    expect(parseMarkdown('***')[0]?.kind).toBe('rule');
    expect(parseMarkdown('- - -')[0]?.kind).toBe('rule');
    expect(parseMarkdown('- voce')[0]?.kind).toBe('list');
  });

  it('tiene il blocco di codice alla lettera', () => {
    const blocks = parseMarkdown('```json\n{\n  "a": 1\n}\n```\ndopo');

    expect(blocks[0]).toMatchObject({ kind: 'code', language: 'json' });
    if (blocks[0]?.kind === 'code') expect(blocks[0].text).toBe('{\n  "a": 1\n}');
    expect(blocks[1]?.kind).toBe('paragraph');
  });

  it('interpreta il contenuto di una citazione', () => {
    const blocks = parseMarkdown('> # dentro\n> testo');

    expect(blocks[0]?.kind).toBe('quote');
    if (blocks[0]?.kind === 'quote') {
      expect(blocks[0].blocks.map((block) => block.kind)).toEqual(['heading', 'paragraph']);
    }
  });

  it('legge le tabelle con gli allineamenti', () => {
    const blocks = parseMarkdown('| a | b |\n| :- | -: |\n| 1 | 2 |');

    expect(blocks[0]?.kind).toBe('table');
    if (blocks[0]?.kind === 'table') {
      expect(blocks[0].align).toEqual(['left', 'right']);
      expect(blocks[0].rows).toHaveLength(1);
      expect(text(blocks[0].rows[0]![1]!)).toBe('2');
    }
  });

  it('gestisce CRLF e stringhe vuote', () => {
    expect(parseMarkdown('a\r\n\r\nb')).toHaveLength(2);
    expect(parseMarkdown('')).toHaveLength(0);
    expect(parseMarkdown('   \n  ')).toHaveLength(0);
  });

  it('legge una news reale senza perdere pezzi', () => {
    const blocks = parseMarkdown(
      '# Titolo\nIntro.\n\n## 🟢 Stable (v1.1.3)\n' +
        '- **NTSC-U**: correzioni varie.\n- **Mogi Mode**: disponibile.\n\n---\n\n' +
        '*(nota tra parentesi)*'
    );

    expect(blocks.map((block) => block.kind)).toEqual([
      'heading',
      'paragraph',
      'heading',
      'list',
      'rule',
      'paragraph'
    ]);
  });
});

describe('parseInline', () => {
  it('riconosce grassetto, corsivo, barrato e codice', () => {
    const spans = parseInline('normale **grassetto** *corsivo* ~~barrato~~ `codice`');

    expect(spans.find((span) => span.bold)?.text).toBe('grassetto');
    expect(spans.find((span) => span.italic)?.text).toBe('corsivo');
    expect(spans.find((span) => span.strike)?.text).toBe('barrato');
    expect(spans.find((span) => span.code)?.text).toBe('codice');
    expect(text(spans)).toBe('normale grassetto corsivo barrato codice');
  });

  it('riconosce grassetto e corsivo insieme', () => {
    const spans = parseInline('***tutto e due***');
    expect(spans[0]).toMatchObject({ text: 'tutto e due', bold: true, italic: true });
  });

  it('non chiude il corsivo sul grassetto che contiene', () => {
    const spans = parseInline('*prima **dentro** dopo*');

    expect(spans.every((span) => span.italic)).toBe(true);
    expect(spans.find((span) => span.bold)?.text).toBe('dentro');
    expect(text(spans)).toBe('prima dentro dopo');
  });

  it('non confonde il grassetto con due corsivi', () => {
    const spans = parseInline('**doppio**');
    expect(spans).toHaveLength(1);
    expect(spans[0]).toMatchObject({ text: 'doppio', bold: true, italic: false });
  });

  it('non rende corsivo un underscore dentro una parola', () => {
    const spans = parseInline('nome_con_underscore');
    expect(spans.some((span) => span.italic)).toBe(false);
    expect(text(spans)).toBe('nome_con_underscore');
  });

  it('lascia intatto il markup non chiuso', () => {
    expect(text(parseInline('**non chiuso'))).toBe('**non chiuso');
    expect(text(parseInline('`neanche questo'))).toBe('`neanche questo');
  });

  it('rispetta le sequenze di escape', () => {
    const spans = parseInline('\\*non corsivo\\*');
    expect(spans.some((span) => span.italic)).toBe(false);
    expect(text(spans)).toBe('*non corsivo*');
  });

  it('costruisce i link markdown e ne formatta l etichetta', () => {
    const spans = parseInline('vedi [il **sito**](https://esempio.test/pagina) adesso');

    const link = spans.filter((span) => span.href !== null);
    expect(link).toHaveLength(2);
    expect(link[0]?.href).toBe('https://esempio.test/pagina');
    expect(link.find((span) => span.bold)?.text).toBe('sito');
  });

  it('riconosce gli indirizzi scritti nudi, punteggiatura esclusa', () => {
    const spans = parseInline('scrivici su https://discord.gg/abc123, davvero');

    const link = spans.find((span) => span.href !== null);
    expect(link?.href).toBe('https://discord.gg/abc123');
    expect(link?.text).toBe('https://discord.gg/abc123');
    expect(text(spans)).toContain(', davvero');
  });

  it('riconosce gli autolink fra parentesi angolari', () => {
    const spans = parseInline('<https://esempio.test/x>');
    expect(spans[0]?.href).toBe('https://esempio.test/x');
  });

  it('riconosce le immagini', () => {
    const spans = parseInline('![una foto](https://esempio.test/foto.png)');
    expect(spans[0]).toMatchObject({
      image: true,
      href: 'https://esempio.test/foto.png',
      text: 'una foto'
    });
  });

  it('restituisce span vuoti solo per un testo vuoto', () => {
    expect(parseInline('')).toHaveLength(0);
    expect(parseInline('solo testo')).toHaveLength(1);
  });

  it('non produce mai markup HTML', () => {
    // È la garanzia che rende sicuro il rendering: gli span sono dati, non
    // stringhe da interpretare.
    const spans = parseInline('<script>alert(1)</script>');

    expect(spans).toHaveLength(1);
    expect(spans[0]?.text).toBe('<script>alert(1)</script>');
    expect(spans[0]?.href).toBeNull();
  });
});

describe('safeUrl', () => {
  it('accetta solo http e https', () => {
    expect(safeUrl('https://esempio.test/x')).toBe('https://esempio.test/x');
    expect(safeUrl('http://esempio.test/x')).toBe('http://esempio.test/x');
    expect(safeUrl('javascript:alert(1)')).toBeNull();
    expect(safeUrl('data:text/html,<script>')).toBeNull();
    expect(safeUrl('file:///C:/Windows')).toBeNull();
    expect(safeUrl('non un url')).toBeNull();
    expect(safeUrl('   ')).toBeNull();
  });

  it('non costruisce link da uno schema pericoloso scritto nel markdown', () => {
    const spans = parseInline('[clicca](javascript:alert(1))');
    expect(spans.every((span) => span.href === null)).toBe(true);
    expect(text(spans)).toBe('[clicca](javascript:alert(1))');
  });
});
