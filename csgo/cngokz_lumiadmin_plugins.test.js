import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../');
const scripting = resolve(root, 'csgo/addons/sourcemod/scripting');
const plugins = resolve(root, 'csgo/addons/sourcemod/plugins');
const cfg = resolve(root, 'csgo/cfg/sourcemod/cngokz-lumiadmin');
const expectCompiledPlugins = process.env.CNGOKZ_EXPECT_COMPILED_PLUGINS === '1';

function read(path) {
  return readFileSync(path, 'utf8');
}

test('cngokz lumiadmin plugins use block-style SourcePawn structure', () => {
  assert.ok(existsSync(resolve(scripting, 'cngokz-core.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-core/config.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-core/natives.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-server/legacy_disable.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-sync/legacy_disable.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-recordguard.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-recordguard/rules.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-recordguard/detection.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-recordguard/pending_records.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-recordguard/replay_capture.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-recordguard/global_submit.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global/send_run.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global/legacy_disable.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global/api.sp')));
});

test('cngokz lumiadmin config files are under cngokz-lumiadmin', () => {
  const core = read(resolve(scripting, 'cngokz-core.sp'));
  const coreConfig = read(resolve(scripting, 'cngokz-core/config.sp'));
  const server = read(resolve(scripting, 'cngokz-server.sp'));
  const recordguard = read(resolve(scripting, 'cngokz-recordguard/config.sp'));

  assert.match(core, /#define CNGOKZ_CFG_FOLDER "sourcemod\/cngokz-lumiadmin"/);
  assert.match(coreConfig, /CNGOKZCore_EnsureConfigDirectory\(\)/);
  assert.match(server, /EnsureCNGOKZConfigDirectory\(\)/);
  assert.match(recordguard, /RecordGuard_EnsureConfigDirectory\(\)/);
  assert.match(recordguard, /AutoExecConfig\(true, "cngokz-recordguard", "sourcemod\/cngokz-lumiadmin"\)/);
  assert.ok(existsSync(resolve(cfg, 'cngokz-core.cfg')));
  assert.ok(existsSync(resolve(cfg, 'cngokz-recordguard.cfg')));
});

test('cngokz server and sync disable legacy manger plugins before registering compatibility natives', () => {
  const server = read(resolve(scripting, 'cngokz-server.sp'));
  const serverLegacy = read(resolve(scripting, 'cngokz-server/legacy_disable.sp'));
  const sync = read(resolve(scripting, 'cngokz-sync.sp'));
  const syncLegacy = read(resolve(scripting, 'cngokz-sync/legacy_disable.sp'));

  assert.match(server, /IsLegacyServerSurfaceOccupied\(\)/);
  assert.match(server, /g_LegacyServerSurfaceDeferred = true/);
  assert.match(server, /DisableLegacyServerBinary\(\)/);
  assert.match(server, /ServerCommand\("sm plugins reload cngokz-server"\)/);
  assert.match(server, /MarkNativeAsOptional\("EdgeSync_EnqueueOperation"\)/);
  assert.match(server, /bool QueueEdgeSyncOperation\(/);
  assert.match(serverLegacy, /manger_online_reporter\.smx/);
  assert.match(serverLegacy, /MangerReporter_GetApiBaseUrl/);
  assert.match(serverLegacy, /RenameFile\(LEGACY_SERVER_DISABLED_PLUGIN, LEGACY_SERVER_PLUGIN\)/);

  assert.match(sync, /IsLegacyEdgeSyncSurfaceOccupied\(\)/);
  assert.match(sync, /g_LegacyEdgeSyncSurfaceDeferred = true/);
  assert.match(sync, /DisableLegacyEdgeSyncBinary\(\)/);
  assert.match(sync, /ServerCommand\("sm plugins reload cngokz-sync"\)/);
  assert.match(syncLegacy, /manger_edge_sync\.smx/);
  assert.match(syncLegacy, /EdgeSync_EnqueueOperation/);
  assert.match(syncLegacy, /RenameFile\(LEGACY_EDGE_SYNC_DISABLED_PLUGIN, LEGACY_EDGE_SYNC_PLUGIN\)/);
});

test('cngokz-global replaces gokz-global and intercepts abnormal records directly', () => {
  const include = read(resolve(scripting, 'include/cngokz/recordguard.inc'));
  const global = read(resolve(scripting, 'cngokz-global.sp'));
  const sendRun = read(resolve(scripting, 'cngokz-global/send_run.sp'));
  const legacyDisable = read(resolve(scripting, 'cngokz-global/legacy_disable.sp'));

  assert.match(include, /CNGOKZ_RecordGuard_ShouldHoldRecord/);
  assert.match(global, /name = "CNGOKZ Global"/);
  assert.match(global, /#undef REQUIRE_PLUGIN\s+#include <gokz\/global>\s+#define REQUIRE_PLUGIN/);
  assert.match(global, /#include "cngokz-global\/legacy_disable\.sp"/);
  assert.match(global, /#include <cngokz\/recordguard>/);
  assert.match(global, /IsLegacyGlobalSurfaceOccupied\(\)/);
  assert.match(global, /gB_LegacyGlobalSurfaceDeferred = true/);
  assert.match(global, /DisableLegacyGlobalBinary\(\)/);
  assert.match(global, /ServerCommand\("sm plugins reload cngokz-global"\)/);
  assert.match(global, /RegPluginLibrary\("cngokz-global"\)/);
  assert.match(global, /RegPluginLibrary\("gokz-global"\)/);
  assert.match(sendRun, /CNGOKZ_RecordGuard_ShouldHoldRecord/);
  assert.match(sendRun, /GlobalAPI_CreateRecord/);
  assert.match(legacyDisable, /LEGACY_GLOBAL_PLUGIN "addons\/sourcemod\/plugins\/gokz-global\.smx"/);
  assert.match(legacyDisable, /LEGACY_GLOBAL_DISABLED_PLUGIN "addons\/sourcemod\/plugins\/disabled\/gokz-global\.smx"/);
  assert.match(legacyDisable, /ServerCommand\("sm plugins unload gokz-global"\)/);
  assert.match(legacyDisable, /FindPluginByFile\("gokz-global\.smx"\)/);
  assert.match(legacyDisable, /RenameFile\(LEGACY_GLOBAL_DISABLED_PLUGIN, LEGACY_GLOBAL_PLUGIN\)/);
});

test('recordguard talks to abnormal record plugin APIs', () => {
  const recordguard = [
    read(resolve(scripting, 'cngokz-recordguard/rules.sp')),
    read(resolve(scripting, 'cngokz-recordguard/pending_records.sp')),
    read(resolve(scripting, 'cngokz-recordguard/global_submit.sp')),
  ].join('\n');

  assert.match(recordguard, /\/abnormal-record-rules/);
  assert.match(recordguard, /\/abnormal-records/);
  assert.match(recordguard, /\/replay/);
  assert.match(recordguard, /\/poll-approved/);
  assert.match(recordguard, /\/submit-result/);
});

test('build script compiles cngokz plugins with host sourcemod compiler first', () => {
  const build = read(resolve(root, 'csgo/build_plugins.sh'));

  assert.match(build, /HOST_SOURCEMOD_ROOT/);
  assert.match(build, /GOKZ_INCLUDE_DIR/);
  assert.match(build, /compile_plugin "cngokz-core\.sp" "cngokz-core\.smx"/);
  assert.match(build, /compile_plugin "cngokz-recordguard\.sp" "cngokz-recordguard\.smx"/);
  assert.match(build, /compile_plugin "cngokz-global\.sp" "cngokz-global\.smx"/);
});

test('project include directory keeps only cngokz and local plugin headers', () => {
  assert.ok(existsSync(resolve(scripting, 'include/cngokz/recordguard.inc')));
  assert.ok(existsSync(resolve(scripting, 'include/cngokz/core.inc')));
  assert.ok(existsSync(resolve(scripting, 'include/manger_shared.inc')));
  assert.ok(existsSync(resolve(scripting, 'include/ripext.inc')));
  assert.equal(existsSync(resolve(scripting, 'include/gokz')), false);
  assert.equal(existsSync(resolve(scripting, 'include/gokz.inc')), false);
  assert.equal(existsSync(resolve(scripting, 'include/GlobalAPI')), false);
  assert.equal(existsSync(resolve(scripting, 'include/GlobalAPI.inc')), false);
});

test('compiled cngokz plugin artifacts exist', { skip: !expectCompiledPlugins }, () => {
  const core = statSync(resolve(plugins, 'cngokz-core.smx'));
  const recordguard = statSync(resolve(plugins, 'cngokz-recordguard.smx'));
  const global = statSync(resolve(plugins, 'cngokz-global.smx'));

  assert.ok(core.size > 0);
  assert.ok(recordguard.size > 0);
  assert.ok(global.size > 0);
});
