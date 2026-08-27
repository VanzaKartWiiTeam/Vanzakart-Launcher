<script lang="ts">
  /**
   * Rende il markdown prodotto da `$lib/markdown`.
   *
   * Non usa `innerHTML`: ogni blocco e ogni span diventano elementi Svelte, e
   * i link passano dal comando `open_external` invece di navigare la webview.
   */
  import * as api from '$lib/api';
  import { parseMarkdown, type Block, type Span } from '$lib/markdown';
  import { app } from '$lib/stores/app.svelte';

  interface Props {
    source: string;
  }

  const { source }: Props = $props();
  const blocks = $derived(parseMarkdown(source));

  async function open(event: MouseEvent, url: string) {
    event.preventDefault();
    try {
      await api.openExternal(url);
    } catch (error) {
      app.toast('Apertura non riuscita', api.errorMessage(error), 'warning');
    }
  }
</script>

{#snippet leaf(span: Span)}
  {#if span.code}<code>{span.text}</code>{:else if span.bold && span.italic}<strong
      ><em>{span.text}</em></strong
    >{:else if span.bold}<strong>{span.text}</strong>{:else if span.italic}<em>{span.text}</em
    >{:else}{span.text}{/if}
{/snippet}

{#snippet styled(span: Span)}
  {#if span.strike}<s>{@render leaf(span)}</s>{:else}{@render leaf(span)}{/if}
{/snippet}

{#snippet inline(spans: Span[])}
  {#each spans as span, index (index)}
    {#if span.image && span.href}<img
        src={span.href}
        alt={span.text}
        loading="lazy"
      />{:else if span.href}<a href={span.href} onclick={(event) => open(event, span.href!)}
        >{@render styled(span)}</a
      >{:else}{@render styled(span)}{/if}
  {/each}
{/snippet}

{#snippet blockList(items: Block[])}
  {#each items as block, index (index)}
    {#if block.kind === 'heading'}
      {#if block.level === 1}<h3>{@render inline(block.spans)}</h3>{:else if block.level === 2}<h4>
          {@render inline(block.spans)}
        </h4>{:else if block.level === 3}<h5>{@render inline(block.spans)}</h5>{:else}<h6>
          {@render inline(block.spans)}
        </h6>{/if}
    {:else if block.kind === 'paragraph'}
      <p>
        {#each block.lines as line, lineIndex (lineIndex)}{#if lineIndex > 0}<br
            />{/if}{@render inline(line)}{/each}
      </p>
    {:else if block.kind === 'list'}
      {#if block.ordered}
        <ol start={block.start}>
          {#each block.items as item, itemIndex (itemIndex)}
            <li>{@render blockList(item)}</li>
          {/each}
        </ol>
      {:else}
        <ul>
          {#each block.items as item, itemIndex (itemIndex)}
            <li>{@render blockList(item)}</li>
          {/each}
        </ul>
      {/if}
    {:else if block.kind === 'quote'}
      <blockquote>{@render blockList(block.blocks)}</blockquote>
    {:else if block.kind === 'code'}
      <pre><code>{block.text}</code></pre>
    {:else if block.kind === 'rule'}
      <hr />
    {:else if block.kind === 'table'}
      <div class="table-scroll">
        <table>
          <thead>
            <tr>
              {#each block.head as cell, cellIndex (cellIndex)}
                <th style="text-align: {block.align[cellIndex] ?? 'left'}"
                  >{@render inline(cell)}</th
                >
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each block.rows as row, rowIndex (rowIndex)}
              <tr>
                {#each row as cell, cellIndex (cellIndex)}
                  <td style="text-align: {block.align[cellIndex] ?? 'left'}"
                    >{@render inline(cell)}</td
                  >
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/each}
{/snippet}

<div class="markdown">{@render blockList(blocks)}</div>

<style>
  .markdown {
    font-size: var(--vk-fs-small);
    line-height: 1.65;
    color: var(--vk-text-secondary);
    overflow-wrap: anywhere;
  }

  .markdown :global(h3),
  .markdown :global(h4),
  .markdown :global(h5),
  .markdown :global(h6) {
    margin: 16px 0 8px;
    color: var(--vk-text);
    font-weight: 900;
  }

  .markdown :global(h3:first-child),
  .markdown :global(h4:first-child),
  .markdown :global(h5:first-child),
  .markdown :global(h6:first-child) {
    margin-top: 0;
  }

  .markdown :global(h3) {
    font-size: var(--vk-fs-card-title);
  }
  .markdown :global(h4) {
    font-size: 15px;
  }
  .markdown :global(h5),
  .markdown :global(h6) {
    font-size: var(--vk-fs-small);
  }

  .markdown :global(p) {
    margin: 0 0 10px;
  }
  .markdown :global(p:last-child) {
    margin-bottom: 0;
  }

  .markdown :global(ul),
  .markdown :global(ol) {
    margin: 0 0 10px;
    padding-left: 20px;
  }

  .markdown :global(li) {
    margin-bottom: 4px;
  }

  /* Un elemento di elenco contiene blocchi: il paragrafo non deve staccarsi
     dal punto elenco. */
  .markdown :global(li > p) {
    margin-bottom: 0;
  }

  .markdown :global(li > ul),
  .markdown :global(li > ol) {
    margin: 4px 0 0;
  }

  .markdown :global(li::marker) {
    color: var(--vk-cyan-soft);
  }

  .markdown :global(strong) {
    color: var(--vk-text);
    font-weight: 800;
  }

  .markdown :global(s) {
    color: var(--vk-text-faint);
  }

  .markdown :global(a) {
    color: var(--vk-cyan-soft);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }

  .markdown :global(a:hover) {
    color: var(--vk-cyan);
  }

  /* Stesso tetto del media della news: proporzioni intatte, nessun ritaglio. */
  .markdown :global(img) {
    display: block;
    width: auto;
    height: auto;
    max-width: 100%;
    max-height: 360px;
    margin: 10px 0;
    border-radius: var(--vk-radius-input);
    object-fit: contain;
  }

  .markdown :global(code) {
    padding: 1px 5px;
    border-radius: 5px;
    background: var(--vk-input);
    color: var(--vk-cyan-soft);
    font-family: var(--vk-font-mono);
    font-size: 0.92em;
  }

  .markdown :global(pre) {
    margin: 0 0 10px;
    padding: 10px 12px;
    overflow-x: auto;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-input);
    background: var(--vk-input);
  }

  .markdown :global(pre code) {
    padding: 0;
    background: none;
    color: var(--vk-text-secondary);
    white-space: pre;
  }

  .markdown :global(blockquote) {
    margin: 0 0 10px;
    padding: 2px 0 2px 12px;
    border-left: 3px solid var(--vk-stroke);
    color: var(--vk-text-faint);
  }

  .markdown :global(hr) {
    height: 1px;
    margin: 14px 0;
    border: 0;
    background: var(--vk-stroke);
  }

  .markdown :global(.table-scroll) {
    margin-bottom: 10px;
    overflow-x: auto;
  }

  .markdown :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--vk-fs-micro);
  }

  .markdown :global(th),
  .markdown :global(td) {
    padding: 6px 10px;
    border: 1px solid var(--vk-stroke);
  }

  .markdown :global(th) {
    background: var(--vk-input);
    color: var(--vk-text);
    font-weight: 800;
  }
</style>
