/** Formattazione dei numeri mostrati nella procedura guidata. */

import { i18n } from '$setup/lib/i18n/store.svelte';

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/**
 * Byte in forma leggibile, con la stessa scala del backend
 * (`vk_core::progress::format_bytes`) e il separatore decimale della lingua
 * scelta: 1,5 GB in italiano, 1.5 GB in inglese.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';

  let size = bytes;
  let unit = 0;
  while (size >= 1024 && unit < UNITS.length - 1) {
    size /= 1024;
    unit += 1;
  }

  const decimals = size >= 100 || unit === 0 ? 0 : 1;
  return `${size.toLocaleString(i18n.tag, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals
  })} ${UNITS[unit]}`;
}

/** Data ISO in forma breve; se non è una data, resta com'è. */
export function formatDate(value: string): string {
  if (!value) return '';
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toLocaleDateString(i18n.tag, {
    day: '2-digit',
    month: 'long',
    year: 'numeric'
  });
}

/** Ultima parte di un percorso, per le etichette strette. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}
