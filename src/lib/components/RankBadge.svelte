<script lang="ts" module>
  import type { StaffRole } from '$lib/api/types';
  import type { TranslationKey } from '$lib/stores/i18n.svelte';

  const STAFF_NAMES: Record<StaffRole, TranslationKey> = {
    moderator: 'staff.moderator',
    leader: 'staff.leader',
    staff_ghost: 'staff.staff_ghost',
    developer: 'staff.developer',
    creative_director: 'staff.creative_director',
    translator: 'staff.translator'
  };

  /** Il nome di un ruolo dello staff, nella lingua dell'interfaccia. */
  export function staffKey(role: StaffRole): TranslationKey {
    return STAFF_NAMES[role];
  }
</script>

<script lang="ts">
  /**
   * Stemma dello staff e grado di un giocatore, accanto al suo nome.
   *
   * Come nel gioco: prima lo stemma, poi il grado, e chi ha entrambi li
   * mostra tutti e due (§D-097). Il grado è l'immagine del sito; quando manca
   * resta il numero in un cerchietto, lo stesso ripiego del sito: un
   * giocatore di grado 5 non deve sembrare senza grado solo perché il
   * disegno non c'è (§D-094).
   */
  import type { StaffBadgeView } from '$lib/api/types';
  import { tooltip } from '$lib/attachments/tooltip';
  import { t } from '$lib/stores/i18n.svelte';

  interface Props {
    /** Miniatura del grado come data URI. */
    image?: string | null | undefined;
    /** Grado del gioco; 0 se non ne ha. */
    rank?: number | undefined;
    /** Nome di un rank assegnato dal server; vuoto per i gradi del gioco. */
    label?: string | undefined;
    /** Stemma dello staff, se il giocatore ne fa parte. */
    staff?: StaffBadgeView | null | undefined;
    /** Lato in pixel CSS. */
    size?: number;
  }

  const { image = null, rank = 0, label = '', staff = null, size = 22 }: Props = $props();

  const title = $derived(label || (rank > 0 ? t('board.rank', { rank }) : ''));
  const staffTitle = $derived(staff ? t(staffKey(staff.role)) : '');
</script>

{#if staff}
  <img
    class="rank-badge"
    src={staff.image}
    alt={staffTitle}
    {@attach tooltip(staffTitle)}
    draggable="false"
    style="--size: {size}px"
  />
{/if}
{#if image}
  <img
    class="rank-badge"
    src={image}
    alt={title}
    {@attach tooltip(title)}
    draggable="false"
    style="--size: {size}px"
  />
{:else if rank > 0}
  <span
    class="rank-badge number"
    role="img"
    aria-label={title}
    style="--size: {size}px"
    {@attach tooltip(title)}
  >
    {rank}
  </span>
{/if}

<style>
  .rank-badge {
    flex: none;
    width: var(--size);
    height: var(--size);
    object-fit: contain;
    filter: drop-shadow(0 0 5px rgb(0 0 0 / 0.4));
  }

  .number {
    display: inline-grid;
    place-items: center;
    border: 1px solid rgb(255 255 255 / 0.18);
    border-radius: 50%;
    background: rgb(255 255 255 / 0.08);
    color: var(--vk-cyan-soft);
    font-size: calc(var(--size) * 0.5);
    font-weight: 900;
    line-height: 1;
    filter: none;
  }
</style>
