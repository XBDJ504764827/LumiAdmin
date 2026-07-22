export const DEFAULT_RELOAD_PLUGINS = [];

export const BUILT_IN_RELOAD_PLUGINS = [
  'manger_edge_sync',
  'manger_online_reporter',
];

export const SOURCE_MOD_PLUGIN_LIST_COMMAND = 'sm plugins list';
export const MAX_PLUGIN_INFO_PROBES_PER_SERVER = 80;
export const SAVED_RELOAD_PLUGINS_STORAGE_KEY = 'lumiadmin_reload_plugin_options';
export const DETECTED_RELOAD_PLUGINS_STORAGE_KEY = 'lumiadmin_detected_reload_plugins';

const PLUGIN_NAME_PATTERN = /^[A-Za-z0-9_./-]+$/;

export function normalizePluginName(value) {
  const name = String(value ?? '').trim().replaceAll('\\', '/');
  const pluginsIndex = name.toLowerCase().lastIndexOf('plugins/');
  return pluginsIndex >= 0 ? name.slice(pluginsIndex + 'plugins/'.length) : name;
}

export function pluginIdentityKey(value) {
  return normalizePluginName(value).toLowerCase().replace(/\.smx$/, '');
}

export function validatePluginName(value) {
  const name = normalizePluginName(value);
  if (!name) return '插件名称不能为空。';
  if (name.length > 96) return '插件名称过长。';
  if (!PLUGIN_NAME_PATTERN.test(name)) return '插件名称只能包含字母、数字、下划线、短横线、点和斜杠。';
  if (name.startsWith('/') || name.endsWith('/') || name.includes('..')) return '插件名称不能包含无效路径。';
  return '';
}

export function normalizePluginList(values) {
  const seen = new Set();
  const result = [];

  for (const value of values ?? []) {
    const name = normalizePluginName(value);
    if (!name) continue;

    const key = pluginIdentityKey(name);
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(name);
  }

  return result;
}

function browserLocalStorage() {
  try {
    return typeof window !== 'undefined' ? window.localStorage : null;
  } catch {
    return null;
  }
}

export function readSavedReloadPluginOptions(storage = browserLocalStorage()) {
  if (!storage) return [];
  try {
    const parsed = JSON.parse(storage.getItem(SAVED_RELOAD_PLUGINS_STORAGE_KEY) || '[]');
    return Array.isArray(parsed) ? normalizePluginList(parsed) : [];
  } catch {
    return [];
  }
}

export function writeSavedReloadPluginOptions(values, storage = browserLocalStorage()) {
  const plugins = normalizePluginList(values);
  if (!storage) return plugins;
  try {
    storage.setItem(SAVED_RELOAD_PLUGINS_STORAGE_KEY, JSON.stringify(plugins));
  } catch {
    // Storage may be unavailable in private browsing or restricted contexts.
  }
  return plugins;
}

export function readDetectedReloadPlugins(storage = browserLocalStorage()) {
  if (!storage) return [];
  try {
    const parsed = JSON.parse(storage.getItem(DETECTED_RELOAD_PLUGINS_STORAGE_KEY) || '[]');
    return Array.isArray(parsed) ? normalizePluginList(parsed) : [];
  } catch {
    return [];
  }
}

export function writeDetectedReloadPlugins(values, storage = browserLocalStorage()) {
  const plugins = normalizePluginList(values);
  if (!storage) return plugins;
  try {
    storage.setItem(DETECTED_RELOAD_PLUGINS_STORAGE_KEY, JSON.stringify(plugins));
  } catch {
    // Storage may be unavailable in private browsing or restricted contexts.
  }
  return plugins;
}

export function addPluginOption(options, pluginName) {
  const error = validatePluginName(pluginName);
  if (error) throw new Error(error);
  return normalizePluginList([...(options ?? []), pluginName]);
}

export function buildPluginReloadCommand(pluginName) {
  const error = validatePluginName(pluginName);
  if (error) throw new Error(error);
  return `sm plugins reload ${normalizePluginName(pluginName)}`;
}

export function buildPluginReloadCommands(pluginNames) {
  const plugins = normalizePluginList(pluginNames);
  if (plugins.length === 0) throw new Error('请至少选择一个插件。');
  return plugins.map(buildPluginReloadCommand);
}

export function buildPluginInfoCommand(pluginId) {
  const id = Number(pluginId);
  if (!Number.isInteger(id) || id <= 0 || id > 999) throw new Error('插件编号无效。');
  return `sm plugins info ${id}`;
}

function extractPluginFilenameCandidates(text) {
  const names = [];
  const matches = String(text ?? '').matchAll(/[A-Za-z0-9_./-]+\.smx\b/gi);

  for (const match of matches) {
    const name = normalizePluginName(match[0]);
    if (!validatePluginName(name)) names.push(name);
  }

  return names;
}

export function parseSourceModPluginList(responseText) {
  const ids = [];
  const filenames = [];
  const seenIds = new Set();

  for (const line of String(responseText ?? '').split(/\r?\n/)) {
    filenames.push(...extractPluginFilenameCandidates(line));

    const idMatch = line.match(/^\s*(?:\[(\d{1,3})\]|(\d{1,3}))\s+(?:"|<|[A-Za-z0-9])/);
    if (!idMatch) continue;

    const id = Number(idMatch[1] ?? idMatch[2]);
    if (Number.isInteger(id) && id > 0 && !seenIds.has(id)) {
      seenIds.add(id);
      ids.push(id);
    }
  }

  return {
    ids,
    filenames: normalizePluginList(filenames),
  };
}

export function parseSourceModPluginInfo(responseText) {
  const text = String(responseText ?? '');
  const filenameMatch = text.match(/^\s*(?:File(?:name)?):\s*"?([^"\r\n]+?)"?\s*$/im);
  if (filenameMatch) {
    const filename = normalizePluginName(filenameMatch[1]);
    if (!validatePluginName(filename)) return filename;
  }

  return normalizePluginList(extractPluginFilenameCandidates(text))[0] ?? '';
}
