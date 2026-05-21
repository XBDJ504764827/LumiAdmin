export const themeStorageKey = 'manger_theme';

export function coerceTheme(value) {
  return value === 'light' || value === 'dark' ? value : null;
}

export function getStorage(source) {
  try {
    return source?.localStorage ?? null;
  } catch {
    return null;
  }
}

export function getStoredTheme(storage) {
  if (!storage || typeof storage.getItem !== 'function') return null;
  try {
    return coerceTheme(storage.getItem(themeStorageKey));
  } catch {
    return null;
  }
}

export function persistTheme(storage, theme) {
  if (!storage || typeof storage.setItem !== 'function' || !coerceTheme(theme)) return;
  try {
    storage.setItem(themeStorageKey, theme);
  } catch {
  }
}

export function getSystemTheme(source) {
  if (!source || typeof source.matchMedia !== 'function') return 'light';
  return source.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function getNextTheme(theme) {
  return theme === 'dark' ? 'light' : 'dark';
}
