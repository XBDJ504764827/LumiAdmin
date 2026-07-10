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
  assert.ok(existsSync(resolve(scripting, 'cngokz-recordguard/r2_upload.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-recordguard/global_submit.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global/send_run.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global/r2_upload.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global/legacy_disable.sp')));
  assert.ok(existsSync(resolve(scripting, 'cngokz-global/api.sp')));
});

test('cngokz lumiadmin config files are generated on servers only', () => {
  const core = read(resolve(scripting, 'cngokz-core.sp'));
  const coreConfig = read(resolve(scripting, 'cngokz-core/config.sp'));
  const server = read(resolve(scripting, 'cngokz-server.sp'));
  const sync = read(resolve(scripting, 'cngokz-sync.sp'));
  const recordguard = read(resolve(scripting, 'cngokz-recordguard/config.sp'));
  const gitignore = read(resolve(root, '.gitignore'));

  assert.match(core, /#define CNGOKZ_CFG_FOLDER "sourcemod\/cngokz-lumiadmin"/);
  assert.match(coreConfig, /CNGOKZCore_EnsureConfigDirectory\(\)/);
  assert.match(server, /EnsureCNGOKZConfigDirectory\(\)/);
  assert.match(sync, /EnsureCNGOKZSyncConfigDirectory\(\)/);
  assert.match(recordguard, /RecordGuard_EnsureConfigDirectory\(\)/);
  assert.match(coreConfig, /AutoExecConfig\(true, "cngokz-core", CNGOKZ_CFG_FOLDER\)/);
  assert.match(coreConfig, /cngokz_replay_r2_url/);
  assert.match(coreConfig, /cngokz_replay_r2_key/);
  assert.match(server, /AutoExecConfig\(true, "cngokz-server", "sourcemod\/cngokz-lumiadmin"\)/);
  assert.match(sync, /AutoExecConfig\(true, "cngokz-sync", "sourcemod\/cngokz-lumiadmin"\)/);
  assert.match(recordguard, /AutoExecConfig\(true, "cngokz-recordguard", "sourcemod\/cngokz-lumiadmin"\)/);
  assert.match(gitignore, /csgo\/cfg\/sourcemod\/cngokz-lumiadmin\/\*\.cfg/);
  for (const cfgFile of ['cngokz-core.cfg', 'cngokz-server.cfg', 'cngokz-sync.cfg', 'cngokz-recordguard.cfg']) {
    assert.equal(existsSync(resolve(cfg, cfgFile)), false);
  }
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
  assert.match(global, /ApplySettingsEnforcerForLateLoad\(\)/);
  assert.match(global, /gB_EnforcerOnFreshMap = true/);
  assert.match(global, /GOKZ_InvalidateRun\(client\)/);
  assert.match(sendRun, /CNGOKZ_RecordGuard_ShouldHoldRecord/);
  assert.match(sendRun, /GlobalAPI_CreateRecord/);
  assert.match(legacyDisable, /LEGACY_GLOBAL_PLUGIN "addons\/sourcemod\/plugins\/gokz-global\.smx"/);
  assert.match(legacyDisable, /LEGACY_GLOBAL_DISABLED_PLUGIN "addons\/sourcemod\/plugins\/disabled\/gokz-global\.smx"/);
  assert.match(legacyDisable, /ServerCommand\("sm plugins unload gokz-global"\)/);
  assert.match(legacyDisable, /FindPluginByFile\("gokz-global\.smx"\)/);
  assert.match(legacyDisable, /RenameFile\(LEGACY_GLOBAL_DISABLED_PLUGIN, LEGACY_GLOBAL_PLUGIN\)/);
});

test('recordguard supports an all-maps default threshold with map overrides', () => {
  const detection = read(resolve(scripting, 'cngokz-recordguard/detection.sp'));

  assert.match(detection, /StrEqual\(g_RuleMap\[i\], "\*", false\)/);
  assert.match(detection, /int score = exactMap \? 8 : 0/);
});

test('cngokz-global guards invalid GlobalAPI request handles in callbacks', () => {
  const sources = [
    read(resolve(scripting, 'cngokz-global.sp')),
    read(resolve(scripting, 'cngokz-global/send_run.sp')),
    read(resolve(scripting, 'cngokz-global/maptop_menu.sp')),
    read(resolve(scripting, 'cngokz-global/print_records.sp')),
    read(resolve(scripting, 'cngokz-global/ban_player.sp')),
    read(resolve(scripting, 'cngokz-global/points.sp')),
  ].join('\n');

  assert.match(sources, /bool GlobalAPIRequestFailed\(GlobalAPIRequestData request, const char\[] context\)/);
  assert.match(sources, /bool GlobalAPIResponseInvalid\(JSON_Object response, const char\[] context\)/);
  assert.match(sources, /IsValidHandle\(view_as<Handle>\(request\)\)/);
  assert.match(sources, /IsValidHandle\(view_as<Handle>\(response\)\)/);
  assert.match(sources, /response\.Meta\.GetHandle\("data"\)/);
  assert.match(sources, /GlobalAPI response without readable JSON data/);
  assert.doesNotMatch(sources, /request\.Failure/);
  assert.match(sources, /return false;/);
  assert.match(sources, /GlobalAPIRequestFailed\(request, "GetAuthStatusCallback"\)/);
  assert.match(sources, /GlobalAPIResponseInvalid\(auth_json, "GetAuthStatusCallback"\)/);
  assert.match(sources, /GlobalAPIRequestFailed\(request, "UpdatePointsCallback"\)/);
  assert.match(sources, /GlobalAPIResponseInvalid\(ranks, "UpdatePointsCallback"\)/);
  assert.match(sources, /GlobalAPIResponseInvalid\(mode_json, "GetModeInfoCallback mode"\)/);
  assert.match(sources, /GlobalAPIResponseInvalid\(record_object, "MapTopSubmenuAddItems record"\)/);
});

test('recordguard talks to abnormal record plugin APIs', () => {
  const recordguard = [
    read(resolve(scripting, 'cngokz-recordguard/rules.sp')),
    read(resolve(scripting, 'cngokz-recordguard/pending_records.sp')),
    read(resolve(scripting, 'cngokz-recordguard/r2_upload.sp')),
    read(resolve(scripting, 'cngokz-recordguard/global_submit.sp')),
  ].join('\n');

  assert.match(recordguard, /\/abnormal-record-rules/);
  assert.match(recordguard, /\/abnormal-records/);
  assert.match(recordguard, /\/replay-metadata/);
  assert.match(recordguard, /\/poll-approved/);
  assert.match(recordguard, /\/submit-result/);
  assert.match(recordguard, /SteamWorks_SetHTTPRequestRawPostBodyFromFile/);
  assert.match(recordguard, /X-GOKZ-Mode/);
  assert.match(recordguard, /X-Map/);
  assert.match(recordguard, /X-CNGOKZ-Object-Key/);
  assert.match(recordguard, /X-CNGOKZ-Abnormal-Record-Id/);
  assert.match(recordguard, /X-CNGOKZ-Replay-Category/);
  assert.match(recordguard, /"audit\/%s\/%s\.replay"/);
  assert.doesNotMatch(recordguard, /X-Route/);
  assert.match(recordguard, /CNGOKZCore_GetReplayR2Config/);
  assert.match(recordguard, /Timer_RetryPendingR2Uploads/);
  assert.match(recordguard, /r2_uploaded = 0/);
  assert.match(recordguard, /MarkR2UploadComplete/);
  assert.doesNotMatch(recordguard, /gokz_r2upload_url/);
  assert.doesNotMatch(recordguard, /UploadFile\(/);
});

test('cngokz-global integrates legacy WR replay R2 behavior with shared config', () => {
  const global = read(resolve(scripting, 'cngokz-global.sp'));
  const uploader = read(resolve(scripting, 'cngokz-global/r2_upload.sp'));

  assert.match(global, /CNGOKZReplayR2_OnReplaySaved/);
  assert.match(global, /CNGOKZReplayR2_OnMapStart/);
  assert.match(uploader, /tempReplay/);
  assert.match(uploader, /CNGOKZ_RecordGuard_IsHoldingClient/);
  assert.match(uploader, /CNGOKZCore_GetReplayR2Config/);
  assert.match(uploader, /X-GOKZ-Mode/);
  assert.match(uploader, /X-Map/);
  assert.match(uploader, /X-Route/);
  assert.match(uploader, /CNGOKZReplayR2_BackfillExistingRecords/);
  assert.match(uploader, /CNGOKZReplayR2_UploadWRWithShuffle/);
  assert.doesNotMatch(uploader, /gokz_r2upload_/);
});

test('build script compiles cngokz plugins with host sourcemod compiler first', () => {
  const build = read(resolve(root, 'csgo/build_plugins.sh'));

  assert.match(build, /HOST_SOURCEMOD_ROOT/);
  assert.match(build, /GOKZ_TOP_SOURCE_DIR/);
  assert.match(build, /GOKZ_INCLUDE_DIR/);
  assert.match(build, /compile_plugin "cngokz-core\.sp" "cngokz-core\.smx"/);
  assert.match(build, /compile_plugin "cngokz-recordguard\.sp" "cngokz-recordguard\.smx"/);
  assert.match(build, /compile_plugin "cngokz-global\.sp" "cngokz-global\.smx"/);
});

test('project include directory keeps only cngokz and local plugin headers', () => {
  assert.ok(existsSync(resolve(scripting, 'include/cngokz/recordguard.inc')));
  assert.ok(existsSync(resolve(scripting, 'include/cngokz/core.inc')));
  assert.ok(existsSync(resolve(scripting, 'include/cngokz/session_reasons.inc')));
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
