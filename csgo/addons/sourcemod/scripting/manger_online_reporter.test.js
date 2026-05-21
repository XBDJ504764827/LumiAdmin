import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../../../../');
const sourcePath = resolve(root, 'csgo/addons/sourcemod/scripting/manger_online_reporter.sp');
const pluginPath = resolve(root, 'csgo/addons/sourcemod/plugins/manger_online_reporter.smx');
const configPath = resolve(root, 'csgo/cfg/sourcemod/manger_online_reporter.cfg');

test('SourceMod plugin uses AutoExecConfig to manage manger_online_reporter cfg', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.doesNotMatch(source, /void\s+EnsureReporterConfigExists\(\)/);
  assert.match(source, /BuildPath\(Path_SM, path, sizeof\(path\), "\.\.\/\.\.\/cfg\/sourcemod\/manger_online_reporter\.cfg"\)/);
  assert.match(source, /AutoExecConfig\(true, "manger_online_reporter"\)/);
});

test('SourceMod plugin loads cfg from file without manual creation or rewrite', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.doesNotMatch(source, /void\s+EnsureReporterConfigExists\(\)/);
  assert.doesNotMatch(source, /OpenFile\(path, "w"\)/);
  assert.match(source, /AutoExecConfig\(true, "manger_online_reporter"\)/);
});

test('SourceMod plugin targets cfg path under game cfg directory instead of addons path', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /BuildPath\(Path_SM, path, sizeof\(path\), "\.\.\/\.\.\/cfg\/sourcemod\/manger_online_reporter\.cfg"\)/);
  assert.doesNotMatch(source, /BuildPath\(Path_SM, path, sizeof\(path\), "cfg\/sourcemod\/manger_online_reporter\.cfg"\)/);
});

test('SourceMod plugin clears timer handles on callback and avoids killing stale timers on unload', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /public Action Timer_ReportOnlinePlayers\(Handle timer\)[\s\S]*?if \(g_ReportTimer == timer\)[\s\S]*?g_ReportTimer = null;/);
  assert.match(source, /public Action Timer_PollBans\(Handle timer\)[\s\S]*?if \(g_BanPollTimer == timer\)[\s\S]*?g_BanPollTimer = null;/);
  assert.match(source, /public Action Timer_RefreshAccessSnapshot\(Handle timer\)[\s\S]*?if \(g_AccessSnapshotTimer == timer\)[\s\S]*?g_AccessSnapshotTimer = null;/);
  assert.doesNotMatch(source, /KillTimer\(g_ReportTimer\)/);
  assert.doesNotMatch(source, /KillTimer\(g_BanPollTimer\)/);
  assert.doesNotMatch(source, /KillTimer\(g_AccessSnapshotTimer\)/);
});

test('SourceMod plugin keeps access snapshot database initialization after cfg refactor', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /void\s+InitAccessSnapshotDb\(\)/);
  assert.match(source, /SQLite_UseDatabase\(ACCESS_SNAPSHOT_DB/);
  assert.match(source, /CREATE TABLE IF NOT EXISTS metadata/);
  assert.match(source, /CREATE TABLE IF NOT EXISTS access_profiles/);
});

test('SourceMod plugin registers manger_server as a real server command for exec support', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /RegServerCmd\("manger_server",\s*CommandServerMapping/);
  assert.match(source, /Action\s+CommandServerMapping\(int args\)/);
  assert.match(source, /GetCmdArg\(1,\s*portText/);
  assert.match(source, /GetCmdArg\(2,\s*token/);
});

test('SourceMod plugin relies on AutoExecConfig instead of exec command hook', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.doesNotMatch(source, /RegServerCmd\("exec",\s*CommandExecConfig/);
  assert.doesNotMatch(source, /Action\s+CommandExecConfig\(int args\)/);
  assert.match(source, /AutoExecConfig\(true, "manger_online_reporter"\)/);
});

test('SourceMod plugin reload path manually parses identity and base url from cfg', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /void\s+LoadReporterConfig\(\)/);
  assert.match(source, /ApplyConfigConVarValue\(/);
  assert.match(source, /manger_api_base_url/);
  assert.match(source, /manger_report_identity/);
  assert.match(source, /manger_access_snapshot_interval/);
  assert.match(source, /manger_report_interval/);
});

test('SourceMod online reporter source defines identity ip config without report port cvar', () => {
  assert.ok(existsSync(sourcePath), '插件源码文件应存在');
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /CreateConVar\("manger_api_base_url"/);
  assert.match(source, /CreateConVar\("manger_report_identity"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_report_port"/);
});

test('SourceMod plugin detects current server port at runtime', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /bool\s+GetCurrentServerPort\(int &port\)/);
  assert.match(source, /FindConVar\("hostport"\)/);
  assert.match(source, /GetConVarInt\(FindConVar\("hostport"\)\)/);
  assert.match(source, /GetCurrentServerPort\(currentPort\)/);
});

test('SourceMod plugin builds runtime identity from public ip and detected port', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /bool\s+BuildCurrentServerIdentity\(/);
  assert.match(source, /g_ReportIdentity\.GetString\(reportIdentity/);
  assert.match(source, /Format\(identity, maxLen, "%s:%d", reportIdentity, currentPort\)/);
});

test('SourceMod plugin relies on AutoExecConfig to create cfg template on first load', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.doesNotMatch(source, /void\s+EnsureReporterConfigExists\(\)/);
  assert.match(source, /AutoExecConfig\(true, "manger_online_reporter"\)/);
});

test('SourceMod plugin posts all HTTP payloads through ripext requests', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.doesNotMatch(source, /HTTPClientPostJson/);
  assert.doesNotMatch(source, /MarkNativeAsOptional/);
  assert.match(source, /PostJsonPayload\(/);
  assert.match(source, /JSONObject\.FromString\(/);
  assert.match(source, /request\.Post\(payload,/);
});

test('SourceMod plugin removes legacy URL and token cvars', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.doesNotMatch(source, /CreateConVar\("manger_report_url"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_report_token"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_server_configs"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_server_tokens"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_ban_api_url"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_ban_check_api_url"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_ban_poll_api_url"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_unban_api_url"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_access_check_api_url"/);
  assert.doesNotMatch(source, /CreateConVar\("manger_access_snapshot_api_url"/);
});

test('SourceMod plugin defines endpoint helpers from a single base URL', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /bool\s+BuildPluginApiUrl\(/);
  assert.match(source, /g_ApiBaseUrl\.GetString\(url, maxLen\)/);
  assert.match(source, /TrimString\(url\)/);
  assert.match(source, /StrCat\(url, maxLen, suffix\)/);
  assert.match(source, /bool\s+ResolvePluginApiConfig\(char\[] url, int urlMaxLen, const char\[] suffix, char\[] token, int tokenMaxLen\)/);
  assert.match(source, /if \(!BuildPluginApiUrl\(url, urlMaxLen, suffix\)\)/);
  assert.match(source, /if \(!GetCurrentReportToken\(token, tokenMaxLen\)\)/);
});

test('SourceMod plugin registers SourceBans style ban commands', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /RegAdminCmd\("sm_ban",\s*CommandBan,\s*ADMFLAG_BAN/);
  assert.match(source, /RegAdminCmd\("sm_banip",\s*CommandBanIp,\s*ADMFLAG_BAN/);
  assert.match(source, /RegAdminCmd\("sm_addban",\s*CommandAddBan,\s*ADMFLAG_RCON/);
  assert.match(source, /RegAdminCmd\("sm_unban",\s*CommandUnban,\s*ADMFLAG_UNBAN/);
});

test('SourceMod plugin defines ban menu and plugin API configuration', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /CreateConVar\("manger_api_base_url"/);
  assert.match(source, /CreateConVar\("manger_report_identity"/);
  assert.match(source, /DisplayBanTargetMenu/);
  assert.match(source, /DisplayBanTimeMenu/);
  assert.match(source, /DisplayBanReasonMenu/);
  assert.match(source, /OnClientAuthorized/);
  assert.match(source, /BuildPluginBanPayload/);
  assert.match(source, /BuildPluginBanCheckPayload/);
});

test('SourceMod plugin sends player identity with ban and access checks', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /GetClientName\(client,\s*playerName/);
  assert.match(source, /BuildPluginBanCheckPayload\([^\n]+playerName/);
  assert.match(source, /BuildPluginAccessCheckPayload\([^\n]+playerName/);
  assert.match(source, /\\"player\\":\\"%s\\"/);
  assert.match(source, /\\"ip_address\\":\\"%s\\"/);
  assert.match(source, /\\"server_port\\":%d/);
});

test('SourceMod plugin completes polled bans before kicking online players', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /CompletePolledBanDetails\(client\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/bans\/check", token, sizeof\(token\)\)/);
  assert.match(source, /GetClientName\(client,\s*playerName/);
  assert.match(source, /BuildPluginBanCheckPayload\([^\n]+steamId64[^\n]+ipAddress[^\n]+playerName/);
  assert.match(source, /PostJsonPayload\(url,\s*payload,\s*OnBanCheckResponse\)/);
  assert.match(source, /CompletePolledBanDetails\(client\);\s*\n\s*KickClient/);
});

test('SourceMod plugin kicks players rejected by access check', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /int userId = GetClientUserId\(client\)/);
  assert.match(source, /PostJsonPayload\(url,\s*payload,\s*OnAccessCheckResponse,\s*userId\)/);
  assert.match(source, /int client = GetClientOfUserId\(value\)/);
  assert.match(source, /JSONObject result = view_as<JSONObject>\(data\.Get\("result"\)\)/);
  assert.match(source, /result\.GetBool\("allowed"\)/);
  assert.match(source, /result\.GetString\("message",\s*message/);
  assert.match(source, /KickClient\(client,\s*"%s",\s*message\)/);
});

test('SourceMod plugin defines row-based server mapping storage and config loader', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /#include <adt_array>/);
  assert.match(source, /#include <adt_trie>/);
  assert.match(source, /StringMap\s+g_ServerTokenMap\s*=\s*null/);
  assert.match(source, /ArrayList\s+g_ServerPorts\s*=\s*null/);
  assert.match(source, /void\s+ResetServerTokenMappings\(\)/);
  assert.match(source, /void\s+LoadServerTokenMappings\(\)/);
  assert.match(source, /bool\s+ParseServerMappingLine\(/);
  assert.doesNotMatch(source, /manger_servers_clear/);
  assert.match(source, /manger_server/);
});

test('SourceMod plugin matches token by detected current port instead of configured report port', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /int currentPort = 0/);
  assert.match(source, /if \(!GetCurrentServerPort\(currentPort\)\)/);
  assert.match(source, /IntToString\(currentPort, portKey, sizeof\(portKey\)\)/);
  assert.match(source, /g_ServerTokenMap\.GetString\(portKey, token, maxLen\)/);
  assert.doesNotMatch(source, /CreateConVar\("manger_report_port"/);
  assert.doesNotMatch(source, /JSONArray entries = JSONArray\.FromString\(configs\)/);
});

test('SourceMod plugin uses the unified endpoint builder everywhere', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.doesNotMatch(source, /g_ReportUrl\.GetString\(/);
  assert.doesNotMatch(source, /g_BanApiUrl\.GetString\(/);
  assert.doesNotMatch(source, /g_UnbanApiUrl\.GetString\(/);
  assert.doesNotMatch(source, /g_BanCheckApiUrl\.GetString\(/);
  assert.doesNotMatch(source, /g_BanPollApiUrl\.GetString\(/);
  assert.doesNotMatch(source, /g_AccessCheckApiUrl\.GetString\(/);
  assert.doesNotMatch(source, /g_AccessSnapshotApiUrl\.GetString\(/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/access\/snapshot", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/bans\/poll", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/bans\/check", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(reportUrl, sizeof\(reportUrl\), "\/online-players\/report", reportToken, sizeof\(reportToken\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(banUrl, sizeof\(banUrl\), "\/bans", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/bans\/unban", token, sizeof\(token\)\)/);
  assert.match(source, /ResolvePluginApiConfig\(url, sizeof\(url\), "\/access\/check", token, sizeof\(token\)\)/);
});

test('SourceMod plugin kicks connected players before they enter game', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /if \(client <= 0 \|\| !IsClientConnected\(client\)\)/);
});

test('SourceMod plugin logs explicit errors for identity, invalid rows and missing mapped token', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /manger_api_base_url is empty/);
  assert.match(source, /current server port detect failed/);
  assert.match(source, /report identity ip is empty/);
  assert.match(source, /server mapping ignored: invalid row/);
  assert.match(source, /server mapping ignored: invalid port/);
  assert.match(source, /server mapping ignored: empty token/);
  assert.match(source, /server mapping port %d overridden by later config row/);
  assert.match(source, /no token found for port %d/);
  assert.match(source, /manger_api_base_url、manger_report_identity 或 manger_server 映射未配置/);
  assert.doesNotMatch(source, /manger_server_tokens is not valid JSON/);
});

test('SourceMod plugin caches resolved token per port and invalidates on config changes', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /char\s+g_CachedReportToken\[256\]/);
  assert.match(source, /int\s+g_CachedReportPort\s*=\s*-1/);
  assert.match(source, /bool\s+g_HasCachedReportToken\s*=\s*false/);
  assert.match(source, /HookConVarChange\(g_ApiBaseUrl,\s*OnPluginConfigChanged\)/);
  assert.match(source, /HookConVarChange\(g_ReportIdentity,\s*OnPluginConfigChanged\)/);
  assert.match(source, /void\s+InvalidatePluginConfigCache\(\)/);
  assert.match(source, /if \(g_HasCachedReportToken && g_CachedReportPort == currentPort\)/);
  assert.match(source, /strcopy\(token,\s*maxLen,\s*g_CachedReportToken\)/);
});

test('SourceMod plugin only logs config failures when the state changes', () => {
  const source = readFileSync(sourcePath, 'utf8');

  assert.match(source, /int\s+g_LastPluginConfigState\s*=\s*-1/);
  assert.match(source, /int\s+g_LastPluginConfigPort\s*=\s*-1/);
  assert.match(source, /int\s+g_LastPluginConfigEntryIndex\s*=\s*-1/);
  assert.match(source, /bool\s+ShouldLogPluginConfigState\(int state, int port, int entryIndex = -1\)/);
  assert.match(source, /if \(state == g_LastPluginConfigState && port == g_LastPluginConfigPort && entryIndex == g_LastPluginConfigEntryIndex\)/);
  assert.match(source, /void\s+ResetPluginConfigLogState\(\)/);
});

test('SourceMod config template is managed by AutoExecConfig and keeps row server mappings', () => {
  const config = readFileSync(configPath, 'utf8');

  assert.match(config, /manger_report_identity/);
  assert.match(config, /manger_server\s+"10001"\s+"/);
  assert.doesNotMatch(config, /manger_report_port/);
  assert.doesNotMatch(config, /manger_servers_clear/);
});

test('compiled SourceMod online reporter plugin is present', () => {
  assert.ok(existsSync(pluginPath), '编译后的插件文件应存在');
  assert.ok(statSync(pluginPath).size > 0, '编译后的插件文件不能为空');
});


