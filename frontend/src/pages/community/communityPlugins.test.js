import assert from 'node:assert/strict';
import test from 'node:test';
import {
  BUILT_IN_RELOAD_PLUGINS,
  DEFAULT_RELOAD_PLUGINS,
  DETECTED_RELOAD_PLUGINS_STORAGE_KEY,
  SAVED_RELOAD_PLUGINS_STORAGE_KEY,
  addPluginOption,
  buildPluginInfoCommand,
  buildPluginReloadCommand,
  buildPluginReloadCommands,
  normalizePluginList,
  parseSourceModPluginInfo,
  parseSourceModPluginList,
  pluginIdentityKey,
  readDetectedReloadPlugins,
  readSavedReloadPluginOptions,
  validatePluginName,
  writeDetectedReloadPlugins,
  writeSavedReloadPluginOptions,
} from './communityPlugins.js';

test('reload plugins are available but not selected by default', () => {
  assert.deepEqual(DEFAULT_RELOAD_PLUGINS, []);
  assert.deepEqual(BUILT_IN_RELOAD_PLUGINS, ['manger_edge_sync', 'manger_online_reporter']);
});

test('normalizePluginList trims and deduplicates plugin names', () => {
  assert.deepEqual(
    normalizePluginList([' manger_edge_sync ', 'MANGER_EDGE_SYNC.smx', '', 'custom_plugin.smx']),
    ['manger_edge_sync', 'custom_plugin.smx'],
  );
});

test('pluginIdentityKey normalizes .smx suffix for selection matching', () => {
  assert.equal(pluginIdentityKey('Manger_Edge_Sync.smx'), 'manger_edge_sync');
});

test('validatePluginName accepts SourceMod-style plugin identifiers', () => {
  assert.equal(validatePluginName('disabled/custom-plugin.smx'), '');
  assert.equal(validatePluginName('custom_plugin.v2'), '');
});

test('normalizePluginList strips SourceMod plugin directory prefixes from detected files', () => {
  assert.deepEqual(
    normalizePluginList(['addons/sourcemod/plugins/custom_plugin.smx', 'plugins/disabled/other.smx']),
    ['custom_plugin.smx', 'disabled/other.smx'],
  );
});

test('validatePluginName rejects empty and unsafe names', () => {
  assert.equal(validatePluginName(''), '插件名称不能为空。');
  assert.equal(validatePluginName('../secret'), '插件名称不能包含无效路径。');
  assert.equal(validatePluginName('plugin;quit'), '插件名称只能包含字母、数字、下划线、短横线、点和斜杠。');
  assert.equal(validatePluginName('plugin name'), '插件名称只能包含字母、数字、下划线、短横线、点和斜杠。');
});

test('addPluginOption validates and preserves existing options', () => {
  assert.deepEqual(addPluginOption(['manger_edge_sync'], ' custom '), ['manger_edge_sync', 'custom']);
  assert.throws(() => addPluginOption([], 'bad name'), /插件名称只能包含/);
});

test('saved reload plugin options persist normalized custom plugins', () => {
  const store = new Map();
  const storage = {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, value),
  };

  const saved = writeSavedReloadPluginOptions([' custom_plugin ', 'CUSTOM_PLUGIN.smx', 'disabled/other.smx'], storage);

  assert.deepEqual(saved, ['custom_plugin', 'disabled/other.smx']);
  assert.equal(store.get(SAVED_RELOAD_PLUGINS_STORAGE_KEY), '["custom_plugin","disabled/other.smx"]');
  assert.deepEqual(readSavedReloadPluginOptions(storage), ['custom_plugin', 'disabled/other.smx']);
});

test('detected reload plugins persist until next detection overwrites them', () => {
  const store = new Map();
  const storage = {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, value),
  };

  const first = writeDetectedReloadPlugins([' custom_a.smx ', 'CUSTOM_A', 'disabled/other.smx'], storage);
  assert.deepEqual(first, ['custom_a.smx', 'disabled/other.smx']);
  assert.equal(store.get(DETECTED_RELOAD_PLUGINS_STORAGE_KEY), '["custom_a.smx","disabled/other.smx"]');
  assert.deepEqual(readDetectedReloadPlugins(storage), ['custom_a.smx', 'disabled/other.smx']);

  const next = writeDetectedReloadPlugins(['new_plugin.smx'], storage);
  assert.deepEqual(next, ['new_plugin.smx']);
  assert.deepEqual(readDetectedReloadPlugins(storage), ['new_plugin.smx']);
});

test('buildPluginReloadCommands builds commands for selected plugins', () => {
  assert.deepEqual(
    buildPluginReloadCommands(['manger_edge_sync', 'custom_plugin']),
    ['sm plugins reload manger_edge_sync', 'sm plugins reload custom_plugin'],
  );
  assert.equal(buildPluginReloadCommand('custom_plugin.smx'), 'sm plugins reload custom_plugin.smx');
  assert.throws(() => buildPluginReloadCommands([]), /请至少选择一个插件/);
});

test('buildPluginInfoCommand validates SourceMod plugin ids', () => {
  assert.equal(buildPluginInfoCommand(12), 'sm plugins info 12');
  assert.throws(() => buildPluginInfoCommand(0), /插件编号无效/);
  assert.throws(() => buildPluginInfoCommand('bad'), /插件编号无效/);
});

test('parseSourceModPluginList extracts ids and filenames from common list output', () => {
  const parsed = parseSourceModPluginList(`
    [01] SourceMod (1.11.0.6968) by AlliedModders LLC
    [02] Manger Edge Sync Agent (0.1.0) by LumiAdmin
    03 "Custom Plugin" (1.0) by Someone
    [04] disabled/other_plugin.smx
  `);

  assert.deepEqual(parsed.ids, [1, 2, 3, 4]);
  assert.deepEqual(parsed.filenames, ['disabled/other_plugin.smx']);
});

test('parseSourceModPluginInfo prefers Filename fields', () => {
  assert.equal(parseSourceModPluginInfo(`
    Filename: manger_edge_sync.smx
    Title: Manger Edge Sync Agent
  `), 'manger_edge_sync.smx');
  assert.equal(parseSourceModPluginInfo('File: disabled/custom_plugin.smx'), 'disabled/custom_plugin.smx');
});

test('parseSourceModPluginInfo falls back to smx mentions', () => {
  assert.equal(parseSourceModPluginInfo('Loaded plugin addons/sourcemod/plugins/custom_plugin.smx'), 'custom_plugin.smx');
  assert.equal(parseSourceModPluginInfo('No filename here'), '');
});
