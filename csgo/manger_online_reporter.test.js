import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../');
const onlineSourcePath = resolve(root, 'csgo/addons/sourcemod/scripting/cngokz-server.sp');
const primeSourcePath = resolve(root, 'csgo/addons/sourcemod/scripting/cngokz-prime.sp');
const edgeSourcePath = resolve(root, 'csgo/addons/sourcemod/scripting/cngokz-sync.sp');
const sharedIncludePath = resolve(root, 'csgo/addons/sourcemod/scripting/include/manger_shared.inc');
const onlinePluginPath = resolve(root, 'csgo/addons/sourcemod/plugins/cngokz-server.smx');
const edgePluginPath = resolve(root, 'csgo/addons/sourcemod/plugins/cngokz-sync.smx');
const buildScriptPath = resolve(root, 'csgo/build_plugins.sh');
const deployWorkflowPath = resolve(root, '.github/workflows/deploy.yml');
const expectCompiledPlugins = process.env.CNGOKZ_EXPECT_COMPILED_PLUGINS === '1';

function read(path) {
  return readFileSync(path, 'utf8');
}

test('online reporter uses row-based port token mappings from cfg', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /RegServerCmd\("cngokz_server_local",\s*CommandServerMapping/);
  assert.match(source, /IntToString\(currentPort, portKey, sizeof\(portKey\)\)/);
  assert.match(source, /g_ServerTokenMap\.GetString\(portKey, token, maxLen\)/);
  assert.doesNotMatch(source, /CreateConVar\("manger_report_identity"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_report_port"/);
});

test('online reporter parses all cfg-backed intervals and debug flag', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /CreateConVar\("cngokz_status_report_interval"/);
  assert.match(source, /CreateConVar\("cngokz_server_debug"/);
  assert.match(source, /CreateConVar\("cngokz_access_fail_open"/);
  // ParseConfigValueLine is now in shared include
  assert.match(source, /#include <manger_shared>/);
  assert.match(source, /#define DEFAULT_STATUS_REPORT_INTERVAL "30\.0"/);
  assert.match(source, /#define DEFAULT_DEBUG_LOG "0"/);
  assert.match(source, /#define DEFAULT_ACCESS_FAIL_OPEN "1"/);
  assert.match(source, /AutoExecConfig\(true, "cngokz-server", "sourcemod\/cngokz-lumiadmin"\)/);
});

test('online reporter keeps repeat timer handles killable', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /void StopReportTimer\(\)[\s\S]*?delete g_ReportTimer;[\s\S]*?g_ReportTimer = null;/);
  assert.match(source, /void StopBanPollTimer\(\)[\s\S]*?delete g_BanPollTimer;[\s\S]*?g_BanPollTimer = null;/);
  assert.match(source, /void StopAccessSnapshotTimer\(\)[\s\S]*?delete g_AccessSnapshotTimer;[\s\S]*?g_AccessSnapshotTimer = null;/);
  assert.match(source, /void StopStatusReportTimer\(\)[\s\S]*?delete g_StatusReportTimer;[\s\S]*?g_StatusReportTimer = null;/);
  assert.doesNotMatch(source, /public Action Timer_ReportOnlinePlayers\(Handle timer\)[\s\S]{0,160}g_ReportTimer = null;/);
  assert.doesNotMatch(source, /public Action Timer_PollBans\(Handle timer\)[\s\S]{0,160}g_BanPollTimer = null;/);
  assert.doesNotMatch(source, /public Action Timer_RefreshAccessSnapshot\(Handle timer\)[\s\S]{0,160}g_AccessSnapshotTimer = null;/);
  assert.doesNotMatch(source, /public Action Timer_ReportServerStatus\(Handle timer\)[\s\S]{0,160}g_StatusReportTimer = null;/);
});

test('online reporter gates verbose ban diagnostics behind debug logging', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /void DebugLog\(const char\[] format, any \.\.\.\)/);
  assert.match(source, /g_DebugLog\.BoolValue/);
  assert.doesNotMatch(source, /Manger Ban Debug/);
  assert.match(source, /DebugLog\("FindClientBySteamId2:/);
  assert.match(source, /DebugLog\("CommandBan:/);
});

test('online reporter reconstructs split SteamID2 ban targets before reading duration', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /bool ReadSteamId2CommandTarget\(int firstArg, int args, char\[] steamId2, int maxLen, int &nextArg\)/);
  assert.match(source, /Format\(normalizedSteamId2, sizeof\(normalizedSteamId2\), "STEAM_%s:%s:%s", universe, yPart, zPart\)/);
  assert.match(source, /if \(!ReadSteamId2CommandTarget\(1, args, targetArg, sizeof\(targetArg\), argIdx\)\)/);
  assert.match(source, /if \(argIdx > args\)[\s\S]*?用法: sm_ban <#userid\|name\|steamid2> <minutes\|0> \[reason\]/);
  assert.match(source, /AppendCommandReason\(argIdx, args, reason, sizeof\(reason\)\)/);
  assert.match(source, /if \(!ReadSteamId2CommandTarget\(2, args, steamId, sizeof\(steamId\), reasonStart\)\)/);
});

test('online reporter normalizes plugin steam bans to SteamID64 before sync', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /bool ConvertSteam2ToSteamId64\(const char\[] steamId2, char\[] steamId64, int maxLen\)/);
  assert.match(source, /DecimalMultiplyByTwoAndAddSmall\(zPart, y, accountId, sizeof\(accountId\)\)/);
  assert.match(source, /DecimalAddStrings\("76561197960265728", accountId, steamId64, maxLen\)/);
  assert.match(source, /bool NormalizePluginSteamId\(const char\[] input, char\[] steamId64, int maxLen\)/);
  assert.match(source, /if \(!NormalizePluginSteamId\(steamId, normalizedSteamId, sizeof\(normalizedSteamId\)\)\)/);
  // BuildPluginBanPayload now returns JSONObject directly
  assert.match(source, /JSONObject BuildPluginBanPayload\(const char\[] token/);
  assert.match(source, /strcopy\(targetId, sizeof\(targetId\), normalizedSteamId\)/);
  assert.match(source, /if \(!ConvertSteam2ToSteamId64\(targetArg, steamId64, sizeof\(steamId64\)\)\)/);
});

test('online reporter requires SteamID64 for game unban steam targets', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /bool IsIpAddressTarget\(const char\[] target\)/);
  assert.match(source, /GetCmdArg\(1, target, sizeof\(target\)\)/);
  assert.match(source, /if \(!IsSteamId64String\(target\) && !IsIpAddressTarget\(target\)\)/);
  assert.match(source, /游戏内解封玩家请使用 SteamID64/);
  assert.doesNotMatch(source, /ReadSteamId2CommandTarget\(1, args, target, sizeof\(target\)/);
  assert.doesNotMatch(source, /NormalizePluginSteamId\(target, target/);
});

test('online reporter uses unified plugin API endpoints', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /bool\s+BuildPluginApiUrl\(/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/access\/snapshot", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/bans\/poll", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(banUrl, sizeof\(banUrl\), "\/bans", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/access\/check", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/online-players\/disconnect", token, sizeof\(token\)\)/);
  assert.doesNotMatch(source, /manger_report_url/);
  assert.doesNotMatch(source, /manger_ban_api_url/);
});

test('online reporter uses JSONObject API for payload construction', () => {
  const source = read(onlineSourcePath);

  // All BuildXxxPayload functions now return JSONObject
  assert.match(source, /JSONObject BuildReportPayload/);
  assert.match(source, /JSONObject BuildPluginBanPayload/);
  assert.match(source, /JSONObject BuildPluginUnbanPayload/);
  assert.match(source, /JSONObject BuildPluginBanCheckPayload/);
  assert.match(source, /JSONObject BuildPluginAccessCheckPayload/);
  assert.match(source, /JSONObject BuildServerStatusPayload/);
  // PostJsonObject replaces PostJsonPayload (no string round-trip)
  assert.match(source, /bool PostJsonObject\(const char\[] url, JSONObject payload/);
  assert.doesNotMatch(source, /JSONObject\.FromString/);
});

test('online reporter caches hostport ConVar handle', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /ConVar g_HostPortCvar/);
  assert.match(source, /g_HostPortCvar = FindConVar\("hostport"\)/);
  assert.match(source, /g_HostPortCvar\.IntValue/);
});

test('online reporter caches tickrate ConVar handle', () => {
  const source = read(onlineSourcePath);
  const tickrateLookups = source.match(/FindConVar\("sv_maxupdaterate"\)/g) ?? [];

  assert.match(source, /ConVar g_TickrateCvar = null/);
  assert.equal(tickrateLookups.length, 1);
  assert.match(source, /g_TickrateCvar = FindConVar\("sv_maxupdaterate"\)/);
  assert.match(source, /int GetTickrate\(\)[\s\S]*?g_TickrateCvar\.IntValue/);
});

test('csgo plugins do not keep deprecated or unused globals/macros', () => {
  const online = read(onlineSourcePath);
  const edge = read(edgeSourcePath);
  const combined = `${online}\n${edge}`;

  assert.doesNotMatch(combined, /adt_trie/);
  assert.doesNotMatch(combined, /g_ServerPorts/);
  assert.doesNotMatch(edge, /MAX_SYNC_PAYLOAD/);
  assert.doesNotMatch(edge, /SAFE_INT_CHECK/);
});

test('online reporter cleans up on client disconnect', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /public void OnClientDisconnect\(int client\)/);
  assert.match(source, /ReportClientDisconnect\(client\)/);
  assert.match(source, /ClearClientDisconnectReason\(client\)/);
  assert.match(source, /g_WaitingOwnReason\[client\] = false/);
  assert.match(source, /g_BanTarget\[client\] = 0/);
  assert.match(source, /g_BanTime\[client\] = 0/);
});

test('online reporter records detailed disconnect reasons', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /char g_DisconnectReason\[MAXPLAYERS \+ 1\]\[32\]/);
  assert.match(source, /#include <cngokz\/session_reasons>/);
  assert.match(source, /AddCommandListener\(CommandKickListener, "sm_kick"\)/);
  assert.match(source, /MarkClientDisconnect\(target, CNGOKZ_SESSION_REASON_ADMIN_KICKED, detail\)/);
  assert.match(source, /MarkClientDisconnect\(matchedClient, CNGOKZ_SESSION_REASON_BANNED_KICKED, reason\)/);
  assert.match(source, /MarkClientDisconnect\(client, CNGOKZ_SESSION_REASON_ACCESS_REJECTED, message\)/);
  assert.match(source, /payload\.SetString\("reason", reason\)/);
  assert.match(source, /payload\.SetString\("detail", detail\)/);
});

test('online reporter resets ban poll etag on config change', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /g_BanPollEtag\[0\] = '\\0'/);
});

test('online reporter bans table has indexes', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /CREATE INDEX IF NOT EXISTS idx_bans_steam_id ON bans\(steam_id\)/);
  assert.match(source, /CREATE INDEX IF NOT EXISTS idx_bans_ip_address ON bans\(ip_address\)/);
  assert.match(source, /CREATE INDEX IF NOT EXISTS idx_bans_expires_at ON bans\(expires_at\)/);
});

test('online reporter uses pre-built player index for ban poll matching', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /StringMap steamMap = new StringMap\(\)/);
  assert.match(source, /StringMap ipMap = new StringMap\(\)/);
  assert.match(source, /steamMap\.SetValue\(clientSteamId64, client\)/);
  assert.match(source, /ipMap\.SetValue\(clientIp, client\)/);
  assert.match(source, /KickMatchingBan\(item, steamMap, ipMap\)/);
});

test('online reporter rolls back access snapshot transaction on insert failure', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /if \(!SQL_FastQuery\(g_AccessSnapshotDb, "DELETE FROM metadata"\)[\s\S]*?ROLLBACK/);
  assert.match(source, /bool SaveSnapshotBans\(JSONArray bans\)/);
  assert.match(source, /bool SaveSnapshotWhitelist\(JSONArray whitelist\)/);
  assert.match(source, /bool SaveSnapshotAccessProfiles\(JSONArray profiles\)/);
  assert.match(source, /if \(!SQL_FastQuery\(g_AccessSnapshotDb, query\)\)[\s\S]*?return false;/);
  assert.match(source, /if \(!SQL_FastQuery\(g_AccessSnapshotDb, "COMMIT"\)\)[\s\S]*?ROLLBACK/);
});

test('online reporter access checks guard disconnected clients and send server_port', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /void SubmitAccessCheck\(int client\)[\s\S]*?!IsClientConnected\(client\)/);
  assert.match(source, /JSONObject BuildPluginBanCheckPayload[\s\S]*?payload\.SetInt\("server_port", port\)/);
  assert.match(source, /JSONObject BuildPluginAccessCheckPayload[\s\S]*?payload\.SetInt\("server_port", port\)/);
});

test('online reporter includes connected seconds in online snapshots', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /entry\.SetInt\("connected_seconds", RoundToNearest\(GetClientTime\(client\)\)\)/);
});

test('shared include file exists and contains common functions', () => {
  const shared = read(sharedIncludePath);

  assert.match(shared, /stock bool ParseConfigValueLine/);
  assert.match(shared, /stock bool CopyQuotedValue/);
  assert.match(shared, /stock bool ParseServerMappingLine/);
  assert.match(shared, /stock bool ParseCNGOKZServerMappingLine/);
});

test('edge sync loads API and token from reporter cfg', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /g_ApiBaseUrl = FindConVar\("cngokz_api_base_url"\)/);
  assert.match(source, /CreateConVar\("cngokz_api_base_url"/);
  assert.match(source, /void LoadEdgeSyncConfig\(\)/);
  assert.match(source, /BuildPath\(Path_SM, path, sizeof\(path\), "\.\.\/\.\.\/cfg\/sourcemod\/cngokz-lumiadmin\/cngokz-core\.cfg"\)/);
  // ParseServerMappingLine is now in shared include
  assert.match(source, /#include <manger_shared>/);
  assert.match(source, /port == currentPort/);
  assert.match(source, /strcopy\(g_ServerReportToken, sizeof\(g_ServerReportToken\), token\)/);
});

test('online reporter exposes config natives for edge sync', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /CreateNative\("MangerReporter_GetApiBaseUrl", Native_GetApiBaseUrl\)/);
  assert.match(source, /CreateNative\("MangerReporter_GetReportToken", Native_GetReportToken\)/);
  assert.match(source, /CreateNative\("MangerReporter_GetServerPort", Native_GetServerPort\)/);
  assert.match(source, /CreateNative\("CNGOKZServer_GetApiBaseUrl", Native_GetApiBaseUrl\)/);
  assert.match(source, /CreateNative\("CNGOKZServer_GetReportToken", Native_GetReportToken\)/);
  assert.match(source, /CreateNative\("CNGOKZServer_GetServerPort", Native_GetServerPort\)/);
  assert.match(source, /RegPluginLibrary\("cngokz-server"\)/);
  assert.match(source, /RegPluginLibrary\("manger_online_reporter"\)/);
});

test('edge sync prefers cngokz-core config natives and falls back to cfg', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /#include <cngokz\/core>/);
  assert.match(source, /LibraryExists\("cngokz-core"\)/);
  assert.match(source, /MarkNativeAsOptional\("CNGOKZCore_GetApiBaseUrl"\)/);
  assert.match(source, /bool LoadEdgeSyncConfigFromCore\(\)/);
  assert.match(source, /void ResolveEdgeSyncConfig\(\)/);
  assert.match(source, /if \(LoadEdgeSyncConfigFromCore\(\)\)[\s\S]*?return;[\s\S]*?LoadEdgeSyncConfig\(\);/);
});

test('edge sync enforces unique idempotency keys', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /CREATE UNIQUE INDEX IF NOT EXISTS idx_offline_queue_idempotency_key ON offline_queue\(idempotency_key\)/);
});

test('edge sync centralizes SQL escaping and execution helpers', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /bool EscapeSqlString\(Database db, const char\[] value, char\[] escaped, int maxLen\)/);
  assert.match(source, /bool ExecuteSql\(Database db, const char\[] query, const char\[] context\)/);
  assert.match(source, /ExecuteSql\(g_EdgeSyncDb, query, "enqueue operation"\)/);
  assert.doesNotMatch(source, /Failed to enqueue operation: %s", query/);
});

test('plugin build script compiles both SourceMod plugins', () => {
  const script = read(buildScriptPath);

  assert.match(script, /SOURCEMOD_VERSION="\$\{SOURCEMOD_VERSION:-1\.11\.0-git6970\}"/);
  assert.match(script, /sm\.alliedmods\.net\/smdrop/);
  assert.match(script, /curl -fsSL "\$SOURCEMOD_DOWNLOAD_URL" -o "\$archive_path"/);
  assert.match(script, /spcomp64/);
  assert.match(script, /addons\/sourcemod\/plugins/);
  assert.match(script, /GOKZ_TOP_SOURCE_DIR/);
  assert.match(script, /GOKZ_INCLUDE_DIR/);
  assert.match(script, /cngokz-core\.sp/);
  assert.match(script, /cngokz-prime\.sp/);
  assert.match(script, /cngokz-server\.sp/);
  assert.match(script, /cngokz-sync\.sp/);
  assert.match(script, /cngokz-recordguard\.sp/);
  assert.match(script, /cngokz-global\.sp/);
});

test('prime access waits for authentication and has a CS:GO fallback signal', () => {
  const server = read(onlineSourcePath);
  const prime = read(primeSourcePath);

  assert.match(prime, /SteamWorks_OnValidateClient/);
  assert.match(prime, /SteamWorks_HasLicenseForApp\(client, CNGOKZ_CS_PRIME_APPID\)/);
  assert.match(prime, /HasEntProp\(playerResource, Prop_Send, "m_bHasPrime"\)/);
  assert.match(prime, /RegAdminCmd\("sm_cngokz_prime"/);
  assert.match(server, /PRIME_CHECK_MAX_ATTEMPTS/);
  assert.match(server, /Timer_RetryAccessCheck/);
  assert.match(server, /cngokz-prime\.smx is not loaded/);
});

test('deploy workflow rebuilds and tests SourceMod plugins', () => {
  const workflow = read(deployWorkflowPath);

  assert.match(workflow, /workflow_dispatch:[\s\S]*?deploy_target:/);
  assert.match(workflow, /workflow_dispatch:[\s\S]*?publish_plugin_release:/);
  assert.match(workflow, /publish_plugin_release:[\s\S]*?type: boolean/);
  assert.match(workflow, /publish_plugin_release:[\s\S]*?default: false/);
  assert.match(workflow, /default: 'ilian'/);
  assert.match(workflow, /options:[\s\S]*?- ilian[\s\S]*?- cngokz[\s\S]*?- all/);
  assert.match(workflow, /plugin:[\s\S]*?'csgo\/\*\*'[\s\S]*?'\.github\/workflows\/deploy\.yml'/);
  assert.match(workflow, /deploy-frontend-cngokz:[\s\S]*?inputs\.deploy_target == 'cngokz'[\s\S]*?inputs\.deploy_target == 'all'/);
  assert.match(workflow, /deploy-backend-cngokz:[\s\S]*?inputs\.deploy_target == 'cngokz'[\s\S]*?inputs\.deploy_target == 'all'/);
  assert.match(workflow, /deploy-plugin-cngokz:[\s\S]*?inputs\.deploy_target == 'cngokz'[\s\S]*?inputs\.deploy_target == 'all'/);
  assert.match(workflow, /deploy-frontend-ilian:[\s\S]*?inputs\.deploy_target == 'ilian'[\s\S]*?inputs\.deploy_target == 'all'/);
  assert.match(workflow, /deploy-backend-ilian:[\s\S]*?inputs\.deploy_target == 'ilian'[\s\S]*?inputs\.deploy_target == 'all'/);
  assert.match(workflow, /deploy-plugin-ilian:[\s\S]*?inputs\.deploy_target == 'ilian'[\s\S]*?inputs\.deploy_target == 'all'/);
  assert.match(workflow, /deploy-frontend-cngokz:[\s\S]*?github\.event_name == 'workflow_dispatch'/);
  assert.match(workflow, /deploy-backend-cngokz:[\s\S]*?github\.event_name == 'workflow_dispatch'/);
  assert.match(workflow, /deploy-plugin-cngokz:[\s\S]*?github\.event_name == 'workflow_dispatch'/);
  assert.match(workflow, /deploy-frontend-ilian:[\s\S]*?github\.event_name == 'workflow_dispatch'/);
  assert.match(workflow, /deploy-backend-ilian:[\s\S]*?github\.event_name == 'workflow_dispatch'/);
  assert.match(workflow, /deploy-plugin-ilian:[\s\S]*?github\.event_name == 'workflow_dispatch'/);
  assert.match(workflow, /name: Cache SourceMod Compiler[\s\S]*?path: csgo\/\.build/);
  assert.match(workflow, /name: Checkout GOKZ Includes[\s\S]*?kzcharm\/gokz-top-plugins\.git/);
  assert.match(workflow, /name: Build SourceMod Plugins[\s\S]*?bash csgo\/build_plugins\.sh/);
  assert.match(workflow, /GOKZ_INCLUDE_DIR="\$PWD\/csgo\/\.build\/gokz-top-plugins\/addons\/sourcemod\/scripting\/include"/);
  assert.match(workflow, /name: Test Plugin Sources[\s\S]*?node --test csgo\/\*\.test\.js/);
  assert.match(workflow, /name: Stage Plugin Output[\s\S]*?mkdir -p csgo\/plugin-output\/plugins/);
  assert.match(workflow, /name: plugin-output[\s\S]*?path: csgo\/plugin-output/);
  assert.match(workflow, /name: Stage Game Server Release Package[\s\S]*?mkdir -p csgo\/plugin-release\/csgo\/addons\/sourcemod\/plugins/);
  assert.match(workflow, /name: Stage Game Server Release Package[\s\S]*?inputs\.publish_plugin_release/);
  assert.match(workflow, /mkdir -p csgo\/plugin-release\/csgo\/cfg\/sourcemod\/cngokz-lumiadmin/);
  assert.match(workflow, /tar -czf "csgo\/plugin-release-assets\/\$package_name" -C csgo\/plugin-release \./);
  assert.match(workflow, /name: plugin-release-package[\s\S]*?path: csgo\/plugin-release-assets/);
  assert.match(workflow, /name: Upload Game Server Package Artifact[\s\S]*?inputs\.publish_plugin_release/);
  assert.match(workflow, /release-plugin:[\s\S]*?softprops\/action-gh-release@v2/);
  assert.match(workflow, /release-plugin:[\s\S]*?inputs\.publish_plugin_release/);
  assert.match(workflow, /tag_name: cngokz-plugins-\$\{\{ github\.run_number \}\}/);
  assert.match(workflow, /plugin-release-package\/\*\.tar\.gz/);
  assert.match(workflow, /plugin-release-package\/SHA256SUMS/);
  assert.doesNotMatch(workflow, /plugin-output\/cfg/);
  assert.doesNotMatch(workflow, /cp csgo\/cfg\/sourcemod\/cngokz-lumiadmin/);
  assert.doesNotMatch(workflow, /rsync[\s\S]*plugin-output\/cfg/);
  assert.match(workflow, /mkdir -p "\$PLUGIN_DIR\/disabled"/);
  assert.match(workflow, /mv "\$PLUGIN_DIR\/gokz-global\.smx" "\$PLUGIN_DIR\/disabled\/gokz-global\.smx"/);
  assert.match(workflow, /mv "\$PLUGIN_DIR\/gokz-r2upload\.smx" "\$PLUGIN_DIR\/disabled\/gokz-r2upload\.smx"/);
  assert.match(workflow, /mv "\$PLUGIN_DIR\/manger_online_reporter\.smx" "\$PLUGIN_DIR\/disabled\/manger_online_reporter\.smx"/);
  assert.match(workflow, /mv "\$PLUGIN_DIR\/manger_edge_sync\.smx" "\$PLUGIN_DIR\/disabled\/manger_edge_sync\.smx"/);
  assert.match(workflow, /包含自定义 gokz-replays\.smx/);
  assert.match(workflow, /请完整重启游戏服务器/);
  assert.doesNotMatch(workflow, /sm plugins (?:unload|reload|load) /);
  for (const plugin of [
    'gokz-replays.smx',
    'cngokz-core.smx',
    'cngokz-prime.smx',
    'cngokz-server.smx',
    'cngokz-sync.smx',
    'cngokz-recordguard.smx',
    'cngokz-global.smx',
  ]) {
    const escapedPlugin = plugin.replace('.', '\\.');
    assert.match(workflow, new RegExp(`csgo/addons/sourcemod/plugins/${escapedPlugin}`));
    assert.match(workflow, new RegExp(`plugin-output/plugins/${escapedPlugin}`));
    assert.match(workflow, new RegExp(`plugin-release/csgo/addons/sourcemod/plugins/${escapedPlugin}`));
  }
  assert.doesNotMatch(workflow, /plugins\/manger_/);
  assert.doesNotMatch(workflow, /sm plugins reload manger_/);
});

test('edge sync retries transient failures and avoids concurrent syncs', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /bool g_SyncInFlight = false/);
  assert.match(source, /if \(g_SyncInFlight\) return;/);
  assert.match(source, /g_SyncInFlight = true;/);
  assert.match(source, /void MarkOperationsRetryable\(ArrayList ids, const char\[] error\)/);
  assert.match(source, /UPDATE offline_queue SET status = 'pending'/);
  assert.match(source, /response\.Status >= HTTPStatus_BadRequest && response\.Status < HTTPStatus_InternalServerError/);
  assert.match(source, /MarkOperationsFailed\(ids, errorMsg\)/);
  assert.match(source, /MarkOperationsRetryable\(ids, errorMsg\)/);
});

test('plugin HTTP requests use explicit timeouts', () => {
  const online = read(onlineSourcePath);
  const edge = read(edgeSourcePath);
  const combined = `${online}\n${edge}`;
  const requestCount = combined.match(/HTTPRequest request = new HTTPRequest\(url\)/g)?.length ?? 0;
  const timeoutCount = combined.match(/request\.Timeout = 10/g)?.length ?? 0;

  assert.ok(requestCount > 0);
  assert.equal(timeoutCount, requestCount);
});

test('online reporter fail-opens uncertain access checks but still keeps offline ban checks', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /bool ShouldFailOpenAccessCheck\(\)/);
  assert.match(source, /g_AccessFailOpen == null \|\| g_AccessFailOpen\.BoolValue/);
  assert.match(source, /if \(g_AccessSnapshotDb == null \|\| !IsAccessSnapshotUsable\(\)\)[\s\S]*?if \(ShouldFailOpenAccessCheck\(\)\)[\s\S]*?return;[\s\S]*?KickClient\(client, "访问控制服务暂时不可用，请稍后再试。"\)/);
  assert.match(source, /if \(FindOfflineBan\(steamId, ipAddress, reason, sizeof\(reason\)\)\)[\s\S]*?KickClient\(client, "你已被封禁：%s", reason\);[\s\S]*?if \(!OfflineRulesAllowClient\(steamId\)\)[\s\S]*?if \(ShouldFailOpenAccessCheck\(\)\)[\s\S]*?return;[\s\S]*?KickClient\(client, "你的白名单状态无法确认，请稍后再试。"\)/);
});

test('edge sync failed operations do not increment retry count', () => {
  const source = read(edgeSourcePath);
  const failedFunction = source.match(/void MarkOperationsFailed\(ArrayList ids, const char\[] error\)[\s\S]*?\n}\n\nvoid MarkOperationsRetryable/)?.[0] ?? '';

  assert.ok(failedFunction.length > 0);
  assert.match(source, /UPDATE offline_queue SET status = 'failed', sync_error = '%s' WHERE id = %s/);
  assert.doesNotMatch(failedFunction, /retry_count = retry_count \+ 1/);
});

test('edge sync has retry limit and cleanup', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /#define MAX_RETRY_COUNT/);
  assert.match(source, /#define CLEANUP_RETENTION_SECONDS/);
  assert.match(source, /retry_count < %d.*MAX_RETRY_COUNT/);
  assert.match(source, /void CleanupStaleRecords\(\)/);
  assert.match(source, /DELETE FROM offline_queue WHERE status IN \('synced', 'failed'\)/);
  assert.match(source, /DELETE FROM audit_log WHERE created_at/);
});

test('edge sync uses sequence counter in idempotency key', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /int g_OperationSeq/);
  assert.match(source, /\+\+g_OperationSeq/);
});

test('edge sync caches hostport ConVar handle', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /ConVar g_HostPortCvar/);
  assert.match(source, /g_HostPortCvar = FindConVar\("hostport"\)/);
  assert.match(source, /g_HostPortCvar\.IntValue/);
});

test('ban poll sends and stores server etag to skip unchanged payloads', () => {
  const source = read(onlineSourcePath);

  // 全局变量保存最近一次服务端返回的版本签名
  assert.match(source, /char g_BanPollEtag\[96\];/);
  // 请求体在上次有 etag 时回传
  assert.match(source, /if \(g_BanPollEtag\[0\] != '\\0'\)[\s\S]*?payload\.SetString\("etag", g_BanPollEtag\);/);
  // 响应体中取出服务端回传的 etag 并缓存
  assert.match(source, /if \(!data\.IsNull\("etag"\)\)[\s\S]*?data\.GetString\("etag", g_BanPollEtag, sizeof\(g_BanPollEtag\)\);/);
});

test('compiled SourceMod plugins are present', { skip: !expectCompiledPlugins }, () => {
  assert.ok(existsSync(onlinePluginPath), 'online reporter smx should exist');
  assert.ok(statSync(onlinePluginPath).size > 0, 'online reporter smx should not be empty');
  assert.ok(existsSync(edgePluginPath), 'edge sync smx should exist');
  assert.ok(statSync(edgePluginPath).size > 0, 'edge sync smx should not be empty');
});
