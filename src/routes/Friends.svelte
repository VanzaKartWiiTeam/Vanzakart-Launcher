<script lang="ts">
  /**
   * Friends.
   *
   * Porta la lista amici di `rksys.dat`: il proprio friend code da condividere,
   * e per ogni licenza gli amici salvati, con aggiunta e rimozione.
   *
   * Le scritture passano dal backend, che copia e verifica il salvataggio
   * prima di toccarlo e rifiuta di scrivere mentre Dolphin è aperto.
   *
   * La pagina è tutta qui dentro: una barra con la licenza attiva e il suo
   * friend code, e sotto la lista. Le licenze in più sono linguette dentro la
   * barra, non card: chi ne ha una sola — quasi tutti — non sceglie niente
   * (§D-059, §D-062).
   *
   * I numeri accanto a ogni amico vengono dal server, non dal salvataggio:
   * quelli nel salvataggio sono fermi all'ultima volta che vi siete incontrati
   * online (§D-064).
   */
  import * as api from '$lib/api';
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import MiiAvatar from '$lib/components/MiiAvatar.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { formatNumber, t } from '$lib/stores/i18n.svelte';
  import type { FriendView, LicenseView } from '$lib/api/types';

  /** Posti disponibili nella lista amici di una licenza. */
  const SLOTS = 30;

  let licenses = $state<LicenseView[]>([]);
  let friends = $state<FriendView[]>([]);
  let selected = $state<LicenseView | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  /** Ultimo friend code copiato: la spunta sta sul pulsante che si è premuto. */
  let copiedCode = $state('');
  let newCode = $state('');
  let error = $state('');
  let pendingRemoval = $state<FriendView | null>(null);

  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  const withCode = $derived(licenses.filter((license) => !license.isEmpty && license.friendCode));
  const canWrite = $derived(app.status?.saveWritesEnabled ?? false);
  const full = $derived(friends.length >= SLOTS);

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
      app.toast(t('friends.licensesUnreadable'), api.errorMessage(err), 'warning');
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

  function isSelected(license: LicenseView): boolean {
    return selected?.saveIndex === license.saveIndex && selected?.slot === license.slot;
  }

  async function copy(code: string) {
    try {
      await navigator.clipboard.writeText(code);
      copiedCode = code;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copiedCode = ''), 1600);
    } catch {
      copiedCode = '';
    }
  }

  async function addFriend() {
    if (!selected || !newCode.trim()) return;

    busy = true;
    error = '';
    try {
      friends = await api.addFriend(selected.saveIndex, selected.slot, newCode.trim());
      newCode = '';
      app.toast(t('friends.added'), t('friends.addedBody'), 'success');
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
      app.toast(
        t('friends.removed'),
        t('friends.removedBody', { code: target.friendCode }),
        'success'
      );
      licenses = await api.listLicenses();
    } catch (err) {
      error = api.errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="page">
  {#if loading}
    <div class="vk-card"><div class="vk-skeleton skeleton"></div></div>
  {:else if withCode.length === 0}
    <div class="vk-card vk-empty">
      <Icon name="friends" size={28} />
      <p>{t('friends.emptyTitle')}</p>
      <p class="vk-faint">{t('friends.emptyHint')}</p>
      <button class="vk-btn" onclick={() => app.navigate('licenses')}>
        {t('friends.goToLicenses')}
      </button>
    </div>
  {:else if selected}
    {@const license = selected}
    <!-- Barra della licenza attiva: chi sei, il tuo codice, e nient'altro. -->
    <section class="vk-card bar vk-rainbow-top" style="--accent: {license.accentColor}">
      <MiiAvatar
        studioData={license.studioData}
        initial={license.avatarInitial}
        accent={license.accentColor}
        name={license.miiName || license.name}
        size={44}
      />

      <div class="who">
        <p class="owner">{license.name}</p>
        <p class="vk-faint region">
          {license.region} · {t('friends.slot', { number: license.slot + 1 })}
        </p>
      </div>

      <button class="code" onclick={() => copy(license.friendCode)} title={t('friends.copyMine')}>
        <span class="vk-mono">{license.friendCode}</span>
        <Icon name={copiedCode === license.friendCode ? 'check' : 'copy'} size={14} />
        <span class="vk-visually-hidden">
          {copiedCode === license.friendCode ? t('common.copied') : t('common.copy')}
        </span>
      </button>

      <button class="icon-btn" onclick={load} disabled={busy} title={t('friends.reload')}>
        <Icon name="refresh" size={15} />
        <span class="vk-visually-hidden">{t('common.refresh')}</span>
      </button>

      {#if withCode.length > 1}
        <!--
          Con più licenze la scelta è l'azione principale della barra: pastiglie
          da 11 px non lo dicevano. Ogni licenza è una linguetta con la sua
          faccia e i suoi amici, e la attiva si vede (§D-062).
        -->
        <nav class="switch" aria-label={t('friends.licensesAria')}>
          <span class="vk-eyebrow switch-label">{t('friends.showFriendsOf')}</span>

          {#each withCode as option (`${option.saveIndex}-${option.slot}`)}
            <button
              class="lic-tab"
              class:active={isSelected(option)}
              aria-current={isSelected(option) ? 'true' : undefined}
              onclick={() => !isSelected(option) && select(option)}
              disabled={busy}
              style="--accent: {option.accentColor}"
            >
              <MiiAvatar
                studioData={option.studioData}
                initial={option.avatarInitial}
                accent={option.accentColor}
                name={option.miiName || option.name}
                size={24}
              />
              <span class="lic-tab-name">{option.name}</span>
              <span class="lic-tab-count">{option.friendCount}</span>
            </button>
          {/each}
        </nav>
      {/if}
    </section>

    <section class="vk-card list-card">
      <header class="list-head">
        <div class="count">
          <span class="value">{friends.length}<span class="of">/{SLOTS}</span></span>
          <span class="vk-eyebrow">{t('friends.saved')}</span>
        </div>

        {#if canWrite}
          <div class="add">
            <input
              class="vk-input"
              placeholder="0000-0000-0000"
              maxlength="14"
              bind:value={newCode}
              disabled={busy || full}
              onkeydown={(event) => event.key === 'Enter' && addFriend()}
            />
            <button
              class="vk-btn vk-btn--primary"
              onclick={addFriend}
              disabled={busy || full || newCode.trim().length === 0}
            >
              <Icon name="plus" size={14} />
              {t('friends.add')}
            </button>
          </div>
        {:else}
          <span class="vk-badge">{t('friends.readOnly')}</span>
        {/if}
      </header>

      {#if full && canWrite}
        <p class="vk-faint hint">{t('friends.full')}</p>
      {/if}

      {#if error}
        <p class="vk-error inline">{error}</p>
      {/if}

      {#if friends.length === 0}
        <p class="vk-faint empty-list">{t('friends.none')}</p>
      {:else}
        <ul class="friends">
          {#each friends as friend (friend.slot)}
            <li class="friend" style="--accent: {friend.accentColor}">
              <MiiAvatar
                studioData={friend.studioData}
                initial={friend.avatarInitial}
                accent={friend.accentColor}
                name={friend.miiName}
                size={36}
              />
              <div class="friend-id">
                <!--
                  Niente badge "in attesa": nel salvataggio quasi ogni amico
                  risulta tale finché non ci si incontra online, quindi il
                  badge marcava come anomalo lo stato normale (§D-060).
                -->
                <p class="friend-name">
                  {#if friend.stats?.rankImage}
                    <img
                      class="rank-mini"
                      src={friend.stats.rankImage}
                      alt={t('friends.rank', { rank: friend.stats.prestigeRank })}
                      title={t('friends.rank', { rank: friend.stats.prestigeRank })}
                    />
                  {/if}
                  {friend.miiName}
                </p>
                <p class="vk-mono friend-code">{friend.friendCode}</p>
              </div>

              <!--
                Del server qui interessa solo il VR: vittorie e percentuale
                allungavano la riga e si leggono per intero in Leaderboard.
              -->
              {#if friend.stats}
                <span class="stat-vr">
                  {t('friends.vr', { points: formatNumber(friend.stats.points) })}
                </span>
              {:else}
                <span class="vk-faint friend-unranked">{t('friends.unranked')}</span>
              {/if}

              <div class="friend-actions">
                <button
                  class="icon-btn"
                  onclick={() => copy(friend.friendCode)}
                  title={t('friends.copyCodeOf', { name: friend.miiName })}
                >
                  <Icon name={copiedCode === friend.friendCode ? 'check' : 'copy'} size={15} />
                  <span class="vk-visually-hidden">{t('friends.copyCode')}</span>
                </button>

                {#if canWrite}
                  <button
                    class="icon-btn danger"
                    onclick={() => (pendingRemoval = friend)}
                    disabled={busy}
                    title={t('friends.removeOf', { name: friend.miiName })}
                  >
                    <Icon name="trash" size={15} />
                    <span class="vk-visually-hidden">{t('common.remove')}</span>
                  </button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<Modal
  open={pendingRemoval !== null}
  title={t('friends.confirmTitle')}
  confirmLabel={t('common.remove')}
  cancelLabel={t('common.cancel')}
  danger
  {busy}
  onconfirm={confirmRemoval}
  oncancel={() => (pendingRemoval = null)}
>
  <p>
    {t('friends.confirmBody', {
      name: pendingRemoval?.miiName ?? '',
      code: pendingRemoval?.friendCode ?? ''
    })}
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

  /* --- Barra della licenza --- */

  .bar {
    position: relative;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    overflow: hidden;
    border-color: color-mix(in srgb, var(--accent) 26%, var(--vk-stroke));
  }

  .who {
    min-width: 0;
    margin-right: auto;
  }

  .owner {
    margin: 0;
    font-size: var(--vk-fs-body);
    font-weight: 800;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .region {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
  }

  /* Il friend code è l'unica cosa che si copia da questa pagina. */
  .code {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 14px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: var(--vk-input);
    color: inherit;
    font-size: var(--vk-fs-body);
    font-weight: 700;
    letter-spacing: 0.02em;
    transition:
      border-color var(--vk-dur-fast) var(--vk-ease),
      color var(--vk-dur-fast) var(--vk-ease);
  }

  .code:hover {
    border-color: var(--vk-cyan);
    color: var(--vk-cyan-soft);
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    flex: none;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-badge);
    background: transparent;
    color: var(--vk-text-secondary);
    transition:
      border-color var(--vk-dur-fast) var(--vk-ease),
      color var(--vk-dur-fast) var(--vk-ease);
  }

  .icon-btn:hover:not(:disabled) {
    border-color: var(--vk-cyan);
    color: var(--vk-cyan-soft);
  }

  .icon-btn.danger:hover:not(:disabled) {
    border-color: var(--vk-danger);
    color: var(--vk-danger);
  }

  .icon-btn:disabled {
    opacity: 0.45;
  }

  /* Le licenze in più stanno su una riga sola, sotto la barra. */
  .switch {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    flex-basis: 100%;
    order: 1;
    padding-top: 12px;
    border-top: 1px solid var(--vk-stroke);
  }

  .switch-label {
    margin-right: 2px;
  }

  .lic-tab {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px 6px 8px;
    border: 1px solid var(--vk-stroke);
    border-radius: var(--vk-radius-pill);
    background: var(--vk-input);
    color: var(--vk-text-secondary);
    font-size: var(--vk-fs-small);
    font-weight: 700;
    max-width: 230px;
    transition:
      border-color var(--vk-dur-fast) var(--vk-ease),
      background var(--vk-dur-fast) var(--vk-ease),
      color var(--vk-dur-fast) var(--vk-ease);
  }

  .lic-tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Il numero di amici: dice cosa cambia premendo. */
  .lic-tab-count {
    padding: 1px 7px;
    border-radius: var(--vk-radius-pill);
    background: rgb(255 255 255 / 0.07);
    font-size: var(--vk-fs-eyebrow);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .lic-tab:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--vk-stroke));
    color: var(--vk-text);
  }

  .lic-tab.active {
    background: var(--vk-tab-active);
    border-color: var(--vk-cyan);
    color: var(--vk-text);
    box-shadow: 0 0 14px rgb(0 242 255 / 0.16);
    cursor: default;
  }

  .lic-tab.active .lic-tab-count {
    background: rgb(0 242 255 / 0.18);
    color: var(--vk-cyan-soft);
  }

  .lic-tab:disabled {
    opacity: 0.55;
  }

  .lic-tab.active:disabled {
    opacity: 1;
  }

  /* --- Lista --- */

  .list-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 12px;
  }

  .count {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .count .value {
    font-size: 22px;
    font-weight: 900;
    line-height: 1;
  }

  .count .of {
    font-size: 14px;
    color: var(--vk-text-faint);
  }

  .add {
    display: flex;
    gap: 8px;
  }

  .add .vk-input {
    width: 170px;
    font-family: var(--vk-font-mono);
  }

  .hint,
  .empty-list {
    margin: 0 0 10px;
    font-size: var(--vk-fs-micro);
  }

  .empty-list {
    margin: 6px 0 0;
  }

  .inline {
    padding: 10px 12px;
    margin: 0 0 12px;
    font-size: var(--vk-fs-micro);
  }

  .friends {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .friend {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border: 1px solid transparent;
    border-radius: var(--vk-radius-badge);
    background: var(--vk-panel-soft);
    transition: border-color var(--vk-dur-fast) var(--vk-ease);
  }

  .friend:hover {
    border-color: color-mix(in srgb, var(--accent) 40%, var(--vk-stroke));
  }

  .friend-id {
    min-width: 0;
  }

  .friend-name {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-weight: 800;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rank-mini {
    width: 18px;
    height: 18px;
    flex: none;
    object-fit: contain;
  }

  .friend-code {
    margin: 2px 0 0;
    font-size: var(--vk-fs-micro);
    color: var(--vk-text-secondary);
  }

  .stat-vr {
    margin-left: auto;
    font-size: var(--vk-fs-micro);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    color: var(--vk-cyan-soft);
    white-space: nowrap;
  }

  .friend-unranked {
    margin-left: auto;
    font-size: var(--vk-fs-micro);
    white-space: nowrap;
  }

  /* Copia e cestino, in quest'ordine: si preme molto più spesso la prima. */
  .friend-actions {
    display: flex;
    gap: 6px;
    flex: none;
  }

  .friend-actions .icon-btn {
    width: 30px;
    height: 30px;
  }

  .skeleton {
    height: 90px;
  }

  @media (max-width: 760px) {
    .bar {
      flex-wrap: wrap;
    }

    .code {
      order: 2;
      width: 100%;
      justify-content: space-between;
    }

    .switch {
      order: 3;
    }

    .list-head {
      flex-direction: column;
      align-items: stretch;
    }

    .add .vk-input {
      flex: 1;
      width: auto;
    }

    .friend-unranked {
      display: none;
    }
  }
</style>
