import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, tick, unmount } from 'svelte';

import MenuButton from './MenuButton.svelte';

describe('menu fuori dai contenitori della pagina', () => {
  let component: ReturnType<typeof mount>;
  let host: HTMLDivElement;
  let trigger: HTMLButtonElement;
  const select = vi.fn();
  const disconnect = vi.fn();

  beforeEach(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        disconnect = disconnect;
      }
    );
    host = document.createElement('div');
    host.style.overflow = 'hidden';
    host.style.transform = 'translateY(-2px)';
    document.body.append(host);
    component = mount(MenuButton, {
      target: host,
      props: {
        label: 'Mii menu',
        items: [
          { label: 'Edit', onselect: select },
          { label: 'Unavailable', disabled: true, onselect: select },
          { label: 'Export', onselect: select }
        ]
      }
    });
    trigger = host.querySelector('button')!;
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue({
      left: 145,
      right: 173,
      top: 90,
      bottom: 118,
      width: 28,
      height: 28,
      x: 145,
      y: 90,
      toJSON() {}
    });
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(220);
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(120);
  });

  afterEach(async () => {
    await unmount(component);
    host.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  async function openMenu() {
    trigger.click();
    await tick();
    return document.querySelector<HTMLElement>('[role="menu"]')!;
  }

  it('esce dal contenitore che ritaglia e resta dentro il bordo sinistro', async () => {
    const menu = await openMenu();
    expect(menu.parentElement).toBe(document.body);
    expect(menu.style.left).toBe('8px');
    expect(menu.style.top).toBe('124px');
    expect(document.activeElement?.textContent?.trim()).toBe('Edit');
  });

  it('si apre sopra il pulsante quando manca spazio in basso', async () => {
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue({
      left: 145,
      right: 173,
      top: window.innerHeight - 40,
      bottom: window.innerHeight - 12,
      width: 28,
      height: 28,
      x: 145,
      y: window.innerHeight - 40,
      toJSON() {}
    });
    const menu = await openMenu();
    expect(Number.parseFloat(menu.style.top) + 120).toBe(window.innerHeight - 46);
  });

  it('esegue le azioni anche dopo lo spostamento del menu nel body', async () => {
    const menu = await openMenu();
    const item = menu.querySelector('button')!;
    item.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    item.click();
    await tick();
    expect(select).toHaveBeenCalledOnce();
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(trigger);
    expect(disconnect).toHaveBeenCalledOnce();
  });

  it('salta le voci disabilitate e chiude con Escape', async () => {
    const menu = await openMenu();
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    expect(document.activeElement?.textContent?.trim()).toBe('Export');
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await tick();
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('si chiude quando si scorre la pagina ma non quando si scorre il menu', async () => {
    const menu = await openMenu();
    menu.dispatchEvent(new Event('scroll'));
    await tick();
    expect(document.querySelector('[role="menu"]')).toBe(menu);
    host.dispatchEvent(new Event('scroll'));
    await tick();
    expect(document.querySelector('[role="menu"]')).toBeNull();
  });
});
