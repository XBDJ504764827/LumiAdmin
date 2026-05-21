import test from 'node:test';
import assert from 'node:assert/strict';
import { coerceTheme, getStorage, getStoredTheme, getSystemTheme, getNextTheme, persistTheme } from './themeCore.js';

test('coerceTheme accepts only light or dark', () => {
  assert.equal(coerceTheme('light'), 'light');
  assert.equal(coerceTheme('dark'), 'dark');
  assert.equal(coerceTheme('blue'), null);
  assert.equal(coerceTheme(null), null);
});

test('getStorage returns null when browser storage is unavailable', () => {
  const storage = { getItem: () => null, setItem: () => {} };
  assert.equal(getStorage({ localStorage: storage }), storage);
  assert.equal(getStorage({}), null);
  assert.equal(getStorage({ get localStorage() { throw new Error('blocked'); } }), null);
});

test('getStoredTheme returns persisted theme when valid', () => {
  const storage = {
    getItem(key) {
      assert.equal(key, 'manger_theme');
      return 'dark';
    },
  };

  assert.equal(getStoredTheme(storage), 'dark');
});

test('getStoredTheme ignores invalid values and unavailable storage', () => {
  assert.equal(getStoredTheme({ getItem: () => 'auto' }), null);
  assert.equal(getStoredTheme({ getItem: () => { throw new Error('blocked'); } }), null);
  assert.equal(getStoredTheme(null), null);
});

test('persistTheme stores valid theme and ignores storage failures', () => {
  const writes = [];
  const storage = {
    setItem(key, value) {
      writes.push([key, value]);
    },
  };

  persistTheme(storage, 'light');
  persistTheme(storage, 'blue');
  persistTheme({ setItem: () => { throw new Error('blocked'); } }, 'dark');

  assert.deepEqual(writes, [['manger_theme', 'light']]);
});

test('getSystemTheme returns dark only when media query matches', () => {
  assert.equal(getSystemTheme({ matchMedia: () => ({ matches: true }) }), 'dark');
  assert.equal(getSystemTheme({ matchMedia: () => ({ matches: false }) }), 'light');
  assert.equal(getSystemTheme({}), 'light');
});

test('getNextTheme toggles light and dark', () => {
  assert.equal(getNextTheme('light'), 'dark');
  assert.equal(getNextTheme('dark'), 'light');
});
