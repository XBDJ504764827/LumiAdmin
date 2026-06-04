import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../../../../');
const onlineSourcePath = resolve(root, 'csgo/addons/sourcemod/scripting/manger_online_reporter.sp');
const edgeSourcePath = resolve(root, 'csgo/addons/sourcemod/scripting/manger_edge_sync.sp');
const onlinePluginPath = resolve(root, 'csgo/addons/sourcemod/plugins/manger_online_reporter.smx');
const edgePluginPath = resolve(root, 'csgo/addons/sourcemod/plugins/manger_edge_sync.smx');
const configPath = resolve(root, 'csgo/cfg/sourcemod/manger_online_reporter.cfg');

function read(path) {
  return readFileSync(path, 'utf8');
}

test('online reporter uses row-based port token mappings from cfg', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /RegServerCmd\("manger_server",\s*CommandServerMapping/);
  assert.match(source, /BuildPath\(Path_SM, path, sizeof\(path\), "\.\.\/\.\.\/cfg\/sourcemod\/manger_online_reporter\.cfg"\)/);
  assert.match(source, /IntToString\(currentPort, portKey, sizeof\(portKey\)\)/);
  assert.match(source, /g_ServerTokenMap\.GetString\(portKey, token, maxLen\)/);
  assert.doesNotMatch(source, /CreateConVar\("manger_report_identity"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_report_port"/);
});

test('online reporter parses all cfg-backed intervals and debug flag', () => {
  const source = read(onlineSourcePath);
  const config = read(configPath);

  assert.match(source, /CreateConVar\("manger_status_report_interval"/);
  assert.match(source, /CreateConVar\("manger_debug_log"/);
  assert.match(source, /ParseConfigValueLine\(line, "manger_status_report_interval"/);
  assert.match(source, /ParseConfigValueLine\(line, "manger_debug_log"/);
  assert.match(config, /manger_status_report_interval\s+"30\.0"/);
  assert.match(config, /manger_debug_log\s+"0"/);
  assert.match(config, /manger_server\s+"10001"\s+"/);
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

test('online reporter uses unified plugin API endpoints', () => {
  const source = read(onlineSourcePath);

  assert.match(source, /bool\s+BuildPluginApiUrl\(/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/access\/snapshot", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/bans\/poll", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(banUrl, sizeof\(banUrl\), "\/bans", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/access\/check", token, sizeof\(token\)\)/);
  assert.doesNotMatch(source, /manger_report_url/);
  assert.doesNotMatch(source, /manger_ban_api_url/);
});

test('edge sync loads API and token from reporter cfg', () => {
  const source = read(edgeSourcePath);

  assert.match(source, /g_ApiBaseUrl = FindConVar\("manger_api_base_url"\)/);
  assert.match(source, /CreateConVar\("manger_api_base_url"/);
  assert.match(source, /void LoadEdgeSyncConfig\(\)/);
  assert.match(source, /BuildPath\(Path_SM, path, sizeof\(path\), "\.\.\/\.\.\/cfg\/sourcemod\/manger_online_reporter\.cfg"\)/);
  assert.match(source, /ParseServerMappingLine\(line, port, token, sizeof\(token\)\)/);
  assert.match(source, /port == currentPort/);
  assert.match(source, /strcopy\(g_ServerReportToken, sizeof\(g_ServerReportToken\), token\)/);
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

test('compiled SourceMod plugins are present', () => {
  assert.ok(existsSync(onlinePluginPath), 'online reporter smx should exist');
  assert.ok(statSync(onlinePluginPath).size > 0, 'online reporter smx should not be empty');
  assert.ok(existsSync(edgePluginPath), 'edge sync smx should exist');
  assert.ok(statSync(edgePluginPath).size > 0, 'edge sync smx should not be empty');
});
