<script lang="ts">
  /**
   * Friends.
   *
   * Porta la lista amici di `rksys.dat`: il proprio friend code da condividere,
   * e per ogni licenza gli amici salvati, con aggiunta e rimozione.
   *
   * Le scritture passano dal backend, che copia e verifica il salvataggio
   * prima di toccarlo e rifiuta di scrivere mentre Dolphin è aperto.
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import MiiAvatar from '$lib/components/MiiAvatar.svelte';
  import { app } from '$lib/stores/app.svelte';
  import type { FriendView, LicenseView } from '$lib/api/types';

  let licenses = $state<LicenseView[]>([]);
  let friends = $state<FriendView[]>([]);
  let selected = $state<LicenseView | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let copied = $state('');
  let newCode = $state('');
  let error = $state('');
  let pendingRemoval = $state<FriendView | null>(null);

  const withCode = $derived(licenses.filter((license) => !license.isEmpty && license.friendCode));
  const canWrite = $derived(app.status?.saveWritesEnabled ?? false);
  const full = $derived(friends.length >= 30);

  $effect(() => {
    void load();
  });

  async function load() {
    loading = true;
    try {
      licenses = await api.listLicenses();
      const first = licenses.find((license) => !license.isEmpty);
      await select(first ?? null);
    } catch (err) {
      app.toast('Licenze non leggibili', api.errorMessage(err), 'warning');
      licenses = [];
    } finally {
      loading = false;
    }
  }

  async function select(license: LicenseView | null) {
    selected = license;
    friends = [];
    error = '';
    if (!license) return;

    try {
      friends = await api.listFriends(license.saveIndex, license.slot);
    } catch (err) {
      error = api.errorMessage(err);
    }
  }

  async function copy(code: string) {
    try {
      await navigator.clipboard.writeText(code);
      copied = code;
    } catch {
      copied = '';
    }
  }

  async function addFriend() {
    if (!selected || !newCode.trim()) return;

    busy = true;
    error = '';
    try {
      friends = await api.addFriend(selected.saveIndex, selected.slot, newCode.trim());
      newCode = '';
      app.toast(
        'Amico aggiunto',
        'Il salvataggio è stato copiato prima di modificarlo.',
        'success'
      );
      licenses = await api.listLicenses();
    } catch (err) {
      error = api.errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function confirmRemoval() {
    const target = pendingRemoval;
    pendingRemoval = null;
    if (!selected || !target) return;

    busy = true;
    error = '';
    try {
      friends = await api.removeFriend(selected.saveIndex, selected.slot, target.slot);
      app.toast('Amico rimosso', `${target.friendCode} non è più nella lista.`, 'success');
      licenses = await api.listLicenses();
    } catch (err) {
      error = api.errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="page">
  <section class="vk-card intro vk-rainbow-top">
    <div>
      <p class="vk-eyebrow">Il tuo friend code</p>
      <p class="vk-subtitle">
        Condividi questo codice per farti aggiungere. Chi aggiungi da qui resta una richiesta finché
        non vi incontrate online: è il gioco a confermarla.
      </p>
    </div>
    <button class="vk-btn" onclick={load} disabled={loading || busy}>
      <Icon name="refresh" size={14} />
      Ricarica
    </button>
  </section>

  {#if loading}
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
  {:else if withCode.length === 0}
    <div class="vk-card vk-empty">
      <Icon name="friends" size={28} />
      <p>Nessuna licenza con friend code.</p>
      <p class="vk-faint">
        Il friend code compare dopo la prima connessione online dentro Mario Kart Wii.
      </p>
      <button class="vk-btn" onclick={() => app.navigate('licenses')}>
        Vai a Mii &amp; Licenses
      </button>
    </div>
  {:else}
    <div class="codes">
      {#each withCode as license (`${license.saveIndex}-${license.slot}`)}
        <article
          class="vk-card code-card"
          class:selected={selected?.saveIndex === license.saveIndex &&
            selected?.slot === license.slot}
          style="--accent: {license.accentColor}"
        >
          <MiiAvatar
            studioData={license.studioData}
            initial={license.avatarInitial}
            accent={license.accentColor}
            name={license.miiName || license.name}
            size={48}
          />
          <div>
            <p class="owner">{license.name}</p>
            <p class="vk-faint region">
              {license.region} · Slot {license.slot + 1} · {license.friendCount} amici
            </p>
          </div>
          <span class="vk-spacer"></span>

          {#if withCode.length > 1}
            <button
              class="vk-btn"
              onclick={() => select(license)}
              disabled={busy ||
                (selected?.saveIndex === license.saveIndex && selected?.slot === license.slot)}
            >
              Apri
            </button>
          {/if}

          <button class="code" onclick={() => copy(license.friendCode)}>
            <span class="vk-mono">{license.friendCode}</span>
            <span class="vk-faint hint">
              {copied === license.friendCode ? 'copiato' : 'copia'}
            </span>
          </button>
        </article>
      {/each}
    </div>
  {/if}

  {#if selected}
    <section class="vk-card">
      <div class="section-head">
        <div>
          <p class="vk-eyebrow">Amici di {selected.name}</p>
          <p class="vk-subtitle">
            {friends.length} di 30 posizioni occupate.
            {#if !canWrite}
              Questa build è in sola lettura: la lista si può consultare, non modificare.
            {/if}
          </p>
        </div>
      </div>

      {#if canWrite}
        <div class="add">
          <input
            class="vk-input"
            placeholder="0000-0000-0000"
            maxlength="14"
            bind:value={newCode}
            disabled={busy || full}
          />
          <button
            class="vk-btn vk-btn--primary"
            onclick={addFriend}
            disabled={busy || full || newCode.trim().length === 0}
          >
            Aggiungi
          </button>
        </div>
        {#if full}
          <p class="vk-faint limit">
            La lista è piena: rimuovi un amico prima di aggiungerne un altro.
          </p>
        {/if}
      {/if}

      {#if error}
        <p class="vk-error inline">{error}</p>
      {/if}

      {#if friends.length === 0}
        <p class="vk-faint empty-list">Nessun amico salvato in questa licenza.</p>
      {:else}
        <ul class="friends">
          {#each friends as friend (friend.slot)}
            <li class="friend" style="--accent: {friend.accentColor}">
              <MiiAvatar
                studioData={friend.studioData}
                initial={friend.avatarInitial}
                accent={friend.accentColor}
                name={friend.miiName}
                size={38}
              />
              <div class="friend-id">
                <p class="friend-name">{friend.miiName}</p>
                <p class="vk-mono friend-code">{friend.friendCode}</p>
              </div>
              <div class="friend-stats vk-faint">
                <span>VR {friend.raceRating}</span>
                <span>BR {friend.battleRating}</span>
                <span>{friend.wins}V / {friend.losses}S</span>
              </div>
              {#if friend.isPending}
                <span class="vk-badge vk-badge--warning">In attesa</span>
              {/if}
              {#if canWrite}
                <button
                  class="vk-btn vk-btn--danger"
                  onclick={() => (pendingRemoval = friend)}
                  disabled={busy}
                >
                  Rimuovi
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}

  <section class="vk-card note">
    <p class="vk-eyebrow">Come vengono protetti i salvataggi</p>
    <p class="vk-subtitle">
      Prima di ogni modifica <code>rksys.dat</code> viene copiato nei backup e la copia è verificata per
      hash: se non coincide, il file originale non viene toccato. La scrittura è rifiutata mentre Dolphin
      è aperto, perché all'uscita riscriverebbe il salvataggio dalla propria memoria.
    </p>
  </section>
</div>

<Modal
  open={pendingRemoval !== null}
  title="Rimuovere questo amico?"
  confirmLabel="Rimuovi"
  cancelLabel="Annulla"
  danger
  {busy}
  onconfirm={confirmRemoval}
  oncancel={() => (pendingRemoval = null)}
>
  <p>
    <strong>{pendingRemoval?.miiName}</strong> ({pendingRemoval?.friendCode}) verrà tolto dalla
    lista di questa licenza. Il salvataggio viene copiato prima della modifica.
  </p>
</Modal>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 860px;
    margin: 0 auto;
    padding-bottom: 12px;
  }

  .intro {
    position: relative;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 20px;
    overflow: hidden;
  }

  .codes {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .code-card {
    display: flex;
    align-items: center;
    gap: 14px;
    border-color: color-mix(in srgb, var(--accent) 30%, var(--vk-stroke));
  }

  .code-card.selected {
    border-color: var(--vk-cyan);
    box-shadow: 0 0 18px rgb(0 242 255 / 0.18);
  }

  .owner {
    margin: 0;
    font-size: var(--vk-fs-body);
    font-weight: 800;
  }

  .region {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
  }

  .code {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-input);
    font-size: var(--vk-fs-body);
  }

  .code:hover {
    border-color: var(--vk-cyan);
  }

  .hint {
    font-size: var(--vk-fs-eyebrow);
    text-transform: uppercase;
  }

  .section-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  .add {
    display: flex;
    gap: 10px;
    margin-bottom: 10px;
  }

  .add .vk-input {
    max-width: 220px;
    font-family: var(--vk-font-mono);
  }

  .limit,
  .empty-list {
    font-size: var(--vk-fs-micro);
  }

  .inline {
    padding: 10px 12px;
    margin: 0 0 12px;
    font-size: var(--vk-fs-micro);
  }

  .friends {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .friend {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
  }

  .friend-id {
    min-width: 0;
  }

  .friend-name {
    margin: 0;
    font-weight: 800;
  }

  .friend-code {
    margin: 2px 0 0;
    color: var(--vk-text-secondary);
  }

  .friend-stats {
    display: flex;
    gap: 12px;
    margin-left: auto;
    font-size: var(--vk-fs-micro);
  }

  .friend .vk-btn {
    padding: 7px 11px;
    font-size: var(--vk-fs-micro);
  }

  .note code {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--vk-input);
    color: var(--vk-cyan-soft);
    font-family: var(--vk-font-mono);
    font-size: 0.92em;
  }

  .skeleton {
    height: 90px;
  }

  @media (max-width: 760px) {
    .friend-stats {
      display: none;
    }
  }
</style>
