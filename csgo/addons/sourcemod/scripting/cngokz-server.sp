#include <sourcemod>
#include <adt_array>
#include <ripext>
#include <manger_shared>
#include <cngokz/core>
#include <cngokz/session_reasons>

// Edge Sync Agent Native函数声明
native int EdgeSync_EnqueueOperation(const char[] operation, const char[] target, const char[] targetType, const char[] playerName, const char[] reason, const char[] operatorName, const char[] operatorSteamid, int durationMinutes);
native bool EdgeSync_IsOnline();
native int EdgeSync_GetPendingCount();

#define DEFAULT_API_BASE_URL "http://127.0.0.1:8080/api/plugin"
#define DEFAULT_REPORT_INTERVAL "5.0"
#define DEFAULT_ACCESS_SNAPSHOT_INTERVAL "300.0"
#define DEFAULT_STATUS_REPORT_INTERVAL "30.0"
#define DEFAULT_DEBUG_LOG "0"
#define DEFAULT_ACCESS_FAIL_OPEN "1"
#define ACCESS_SNAPSHOT_DB "manger_access_snapshot"
#define MAX_SERVER_TOKEN 256

ConVar g_ApiBaseUrl;
ConVar g_ReportInterval;
ConVar g_AccessSnapshotInterval;
ConVar g_StatusReportInterval;
ConVar g_DebugLog;
ConVar g_AccessFailOpen;
ConVar g_CpuUsageCvar;
ConVar g_HostPortCvar = null;
ConVar g_TickrateCvar = null;
Handle g_ReportTimer = null;
Handle g_BanPollTimer = null;
Handle g_AccessSnapshotTimer = null;
Handle g_StatusReportTimer = null;
Database g_AccessSnapshotDb = null;
StringMap g_ServerTokenMap = null;
char g_CachedReportToken[256];
int g_CachedReportPort = -1;
bool g_HasCachedReportToken = false;
int g_LastPluginConfigState = -1;
int g_LastPluginConfigPort = -1;
int g_LastPluginConfigEntryIndex = -1;
int g_BanTarget[MAXPLAYERS + 1];
int g_BanTime[MAXPLAYERS + 1];
bool g_WaitingOwnReason[MAXPLAYERS + 1];
int g_LastTickrate = 64;
int g_ServerStartTime = 0;
int g_UnbanAdminUserId = 0;
bool g_LegacyServerSurfaceDeferred = false;
char g_DisconnectReason[MAXPLAYERS + 1][32];
char g_DisconnectDetail[MAXPLAYERS + 1][256];

// 服务端返回的封禁列表版本签名 etag。每次轮询时回传，命中后端版本检测时
// items 为空，避免后端每次全量序列化最多 10000 条记录。
// 注意：配置变更（端口/token）时需要重置，由 ResolvePluginApiConfig 路径负责。
char g_BanPollEtag[96];

public Plugin myinfo =
{
    name = "CNGOKZ Server",
    author = "XBDJ",
    description = "Reports CS:GO online players and server status to LumiAdmin.",
    version = "0.6.0",
    url = ""
};

#include "cngokz-server/legacy_disable.sp"

public void OnPluginStart()
{
    if (!DisableLegacyServerBinary())
    {
        SetFailState("Failed to disable legacy manger_online_reporter.smx. Move it to plugins/disabled and reload cngokz-server.");
    }

    if (g_LegacyServerSurfaceDeferred)
    {
        ServerCommand("sm plugins reload cngokz-server");
        return;
    }

    PrintToServer("[Manger] Plugin v20260525 loaded - SteamID2 matching with universe-agnostic comparison");
    g_ApiBaseUrl = FindConVar("cngokz_api_base_url");
    if (g_ApiBaseUrl == null)
    {
        g_ApiBaseUrl = CreateConVar("cngokz_api_base_url", DEFAULT_API_BASE_URL, "CNGOKZ LumiAdmin plugin API base URL.");
    }
    g_ReportInterval = CreateConVar("cngokz_report_interval", DEFAULT_REPORT_INTERVAL, "Online player report interval in seconds.", _, true, 5.0);
    g_AccessSnapshotInterval = CreateConVar("cngokz_access_snapshot_interval", DEFAULT_ACCESS_SNAPSHOT_INTERVAL, "Access snapshot refresh interval in seconds.", _, true, 30.0);
    g_StatusReportInterval = CreateConVar("cngokz_status_report_interval", DEFAULT_STATUS_REPORT_INTERVAL, "Server status report interval in seconds.", _, true, 10.0);
    g_DebugLog = CreateConVar("cngokz_server_debug", DEFAULT_DEBUG_LOG, "Enable verbose CNGOKZ server plugin debug logs.", _, true, 0.0, true, 1.0);
    g_AccessFailOpen = CreateConVar("cngokz_access_fail_open", DEFAULT_ACCESS_FAIL_OPEN, "Allow players when the LumiAdmin access API and local access snapshot are unavailable.", _, true, 0.0, true, 1.0);

    // sys_cpu_usage: SourceMod 内置的 CPU 使用率 ConVar（需要 csgo/csgo_8225 或更高游戏版本）
    g_CpuUsageCvar = FindConVar("sys_cpu_usage");
    g_HostPortCvar = FindConVar("hostport");
    g_TickrateCvar = FindConVar("sv_maxupdaterate");

    HookConVarChange(g_ApiBaseUrl, OnPluginConfigChanged);
    HookConVarChange(g_ReportInterval, OnPluginConfigChanged);
    HookConVarChange(g_AccessSnapshotInterval, OnPluginConfigChanged);
    HookConVarChange(g_StatusReportInterval, OnStatusReportIntervalChanged);
    HookConVarChange(g_DebugLog, OnPluginConfigChanged);

    RegAdminCmd("sm_ban", CommandBan, ADMFLAG_BAN, "sm_ban <#userid|name> <minutes|0> [reason]");
    RegAdminCmd("sm_banip", CommandBanIp, ADMFLAG_BAN, "sm_banip <ip|#userid|name> <minutes|0> [reason]");
    RegAdminCmd("sm_addban", CommandAddBan, ADMFLAG_RCON, "sm_addban <minutes|0> <steamid> [reason]");
    RegAdminCmd("sm_unban", CommandUnban, ADMFLAG_UNBAN, "sm_unban <steamid|ip> [reason]");
    RegServerCmd("cngokz_server_local", CommandServerMapping, "Register one CNGOKZ server mapping when cngokz-core is not loaded.");
    AddCommandListener(ChatHook, "say");
    AddCommandListener(ChatHook, "say_team");
    AddCommandListener(CommandKickListener, "sm_kick");

    if (g_ServerTokenMap == null)
    {
        g_ServerTokenMap = new StringMap();
    }

    g_ServerStartTime = GetTime();
    ResetServerTokenMappings();
    EnsureCNGOKZConfigDirectory();
    AutoExecConfig(true, "cngokz-server", "sourcemod/cngokz-lumiadmin");
    InitAccessSnapshotDb();
    StartReportTimer();
    StartBanPollTimer();
    StartAccessSnapshotTimer();
    StartStatusReportTimer();
}

void EnsureCNGOKZConfigDirectory()
{
    char dir[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, dir, sizeof(dir), "../../cfg/sourcemod/cngokz-lumiadmin");
    if (!DirExists(dir))
    {
        CreateDirectory(dir, 511);
    }
}

public APLRes AskPluginLoad2(Handle myself, bool late, char[] error, int errMax)
{
    MarkNativeAsOptional("EdgeSync_EnqueueOperation");
    MarkNativeAsOptional("EdgeSync_IsOnline");
    MarkNativeAsOptional("EdgeSync_GetPendingCount");

    if (IsLegacyServerSurfaceOccupied())
    {
        g_LegacyServerSurfaceDeferred = true;
    }
    else
    {
        CreateNative("MangerReporter_GetApiBaseUrl", Native_GetApiBaseUrl);
        CreateNative("MangerReporter_GetReportToken", Native_GetReportToken);
        CreateNative("MangerReporter_GetServerPort", Native_GetServerPort);
        CreateNative("CNGOKZServer_GetApiBaseUrl", Native_GetApiBaseUrl);
        CreateNative("CNGOKZServer_GetReportToken", Native_GetReportToken);
        CreateNative("CNGOKZServer_GetServerPort", Native_GetServerPort);
        RegPluginLibrary("cngokz-server");
        RegPluginLibrary("manger_online_reporter");
    }
    return APLRes_Success;
}

public int Native_GetApiBaseUrl(Handle plugin, int numParams)
{
    int maxLen = GetNativeCell(2);
    char apiBaseUrl[512];
    if (LibraryExists("cngokz-core") && CNGOKZCore_GetApiBaseUrl(apiBaseUrl, sizeof(apiBaseUrl)))
    {
        SetNativeString(1, apiBaseUrl, maxLen);
        return 1;
    }
    g_ApiBaseUrl.GetString(apiBaseUrl, sizeof(apiBaseUrl));
    TrimString(apiBaseUrl);
    SetNativeString(1, apiBaseUrl, maxLen);
    return apiBaseUrl[0] != '\0' ? 1 : 0;
}

public int Native_GetReportToken(Handle plugin, int numParams)
{
    int maxLen = GetNativeCell(2);
    char token[MAX_SERVER_TOKEN];
    if (!GetCurrentReportToken(token, sizeof(token)))
    {
        SetNativeString(1, "", maxLen);
        return 0;
    }
    SetNativeString(1, token, maxLen);
    return 1;
}

public int Native_GetServerPort(Handle plugin, int numParams)
{
    int port = 0;
    if (!GetCurrentServerPort(port))
    {
        return 0;
    }
    return port;
}

public void OnMapStart()
{
    if (g_AccessSnapshotDb == null)
    {
        InitAccessSnapshotDb();
    }

    StartReportTimer();
    StartBanPollTimer();
    StartAccessSnapshotTimer();
    StartStatusReportTimer();
}

public void OnMapEnd()
{
    StopReportTimer();
    StopBanPollTimer();
    StopAccessSnapshotTimer();
    StopStatusReportTimer();
}

public void OnClientDisconnect(int client)
{
    ReportClientDisconnect(client);
    ClearClientDisconnectReason(client);
    g_WaitingOwnReason[client] = false;
    g_BanTarget[client] = 0;
    g_BanTime[client] = 0;
}

public void OnClientPutInServer(int client)
{
    ClearClientDisconnectReason(client);
}

void ClearClientDisconnectReason(int client)
{
    if (client <= 0 || client > MaxClients)
    {
        return;
    }
    g_DisconnectReason[client][0] = '\0';
    g_DisconnectDetail[client][0] = '\0';
}

void MarkClientDisconnect(int client, const char[] reason, const char[] detail)
{
    if (client <= 0 || client > MaxClients)
    {
        return;
    }
    strcopy(g_DisconnectReason[client], sizeof(g_DisconnectReason[]), reason);
    strcopy(g_DisconnectDetail[client], sizeof(g_DisconnectDetail[]), detail);
}

public Action CommandKickListener(int client, const char[] command, int args)
{
    if (args < 1)
    {
        return Plugin_Continue;
    }

    char targetArg[128];
    GetCmdArg(1, targetArg, sizeof(targetArg));
    int target = FindTarget(client, targetArg, true, false);
    if (target <= 0)
    {
        return Plugin_Continue;
    }

    char reason[256];
    AppendCommandReason(2, args, reason, sizeof(reason));

    char adminName[128];
    if (client == 0)
    {
        strcopy(adminName, sizeof(adminName), "CONSOLE");
    }
    else
    {
        GetClientName(client, adminName, sizeof(adminName));
    }

    char detail[256];
    Format(detail, sizeof(detail), "管理员 %s 执行 sm_kick：%s", adminName, reason);
    MarkClientDisconnect(target, CNGOKZ_SESSION_REASON_ADMIN_KICKED, detail);
    return Plugin_Continue;
}

public void OnPluginConfigChanged(ConVar convar, const char[] oldValue, const char[] newValue)
{
    InvalidatePluginConfigCache();
    ResetPluginConfigLogState();

    if (convar == g_ReportInterval)
    {
        StartReportTimer();
        StartBanPollTimer();
        return;
    }

    if (convar == g_AccessSnapshotInterval)
    {
        StartAccessSnapshotTimer();
    }
}

public void OnStatusReportIntervalChanged(ConVar convar, const char[] oldValue, const char[] newValue)
{
    StartStatusReportTimer();
}

void ResetServerTokenMappings()
{
    if (g_ServerTokenMap != null)
    {
        g_ServerTokenMap.Clear();
    }
}

void InvalidatePluginConfigCache()
{
    g_CachedReportToken[0] = '\0';
    g_CachedReportPort = -1;
    g_HasCachedReportToken = false;
    g_BanPollEtag[0] = '\0';
}

void ResetPluginConfigLogState()
{
    g_LastPluginConfigState = -1;
    g_LastPluginConfigPort = -1;
    g_LastPluginConfigEntryIndex = -1;
}

bool ShouldLogPluginConfigState(int state, int port, int entryIndex = -1)
{
    if (state == g_LastPluginConfigState && port == g_LastPluginConfigPort && entryIndex == g_LastPluginConfigEntryIndex)
    {
        return false;
    }

    g_LastPluginConfigState = state;
    g_LastPluginConfigPort = port;
    g_LastPluginConfigEntryIndex = entryIndex;
    return true;
}

bool GetCurrentServerPort(int &port)
{
    if (LibraryExists("cngokz-core"))
    {
        port = CNGOKZCore_GetServerPort();
        if (port > 0)
        {
            return true;
        }
    }

    if (g_HostPortCvar == null)
    {
        return false;
    }
    port = g_HostPortCvar.IntValue;
    return port > 0;
}

bool QueueEdgeSyncOperation(const char[] operation, const char[] target, const char[] targetType, const char[] playerName, const char[] reason, const char[] operatorName, const char[] operatorSteamid, int durationMinutes)
{
    if (GetFeatureStatus(FeatureType_Native, "EdgeSync_EnqueueOperation") != FeatureStatus_Available)
    {
        LogError("[cngokz-server] EdgeSync_EnqueueOperation is unavailable. Load cngokz-sync before relying on offline queue operations.");
        return false;
    }

    EdgeSync_EnqueueOperation(operation, target, targetType, playerName, reason, operatorName, operatorSteamid, durationMinutes);
    return true;
}

Action CommandServerMapping(int args)
{
    if (args < 2)
    {
        return Plugin_Handled;
    }

    char portText[64];
    char token[MAX_SERVER_TOKEN];
    GetCmdArg(1, portText, sizeof(portText));
    GetCmdArg(2, token, sizeof(token));

    int port = StringToInt(portText);
    if (port <= 0)
    {
        LogError("Manger server mapping ignored: invalid port %s.", portText);
        return Plugin_Handled;
    }

    RegisterServerTokenMapping(port, token);
    InvalidatePluginConfigCache();
    return Plugin_Handled;
}

void RegisterServerTokenMapping(int port, const char[] token)
{
    if (port <= 0)
    {
        LogError("Manger server mapping ignored: invalid port %d.", port);
        return;
    }

    char trimmedToken[MAX_SERVER_TOKEN];
    strcopy(trimmedToken, sizeof(trimmedToken), token);
    TrimString(trimmedToken);
    if (trimmedToken[0] == '\0')
    {
        LogError("Manger server mapping ignored: empty token for port %d.", port);
        return;
    }

    char portKey[16];
    IntToString(port, portKey, sizeof(portKey));

    g_ServerTokenMap.SetString(portKey, trimmedToken);
}

public void OnPluginEnd()
{
    StopReportTimer();
    StopBanPollTimer();
    StopAccessSnapshotTimer();
    StopStatusReportTimer();

    if (g_AccessSnapshotDb != null)
    {
        delete g_AccessSnapshotDb;
        g_AccessSnapshotDb = null;
    }
    if (g_ServerTokenMap != null)
    {
        delete g_ServerTokenMap;
        g_ServerTokenMap = null;
    }
}

void InitAccessSnapshotDb()
{
    char error[256];
    g_AccessSnapshotDb = SQLite_UseDatabase(ACCESS_SNAPSHOT_DB, error, sizeof(error));
    if (g_AccessSnapshotDb == null)
    {
        LogError("Manger access snapshot SQLite open failed: %s", error);
        return;
    }

    SQL_FastQuery(g_AccessSnapshotDb, "CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
    SQL_FastQuery(g_AccessSnapshotDb, "CREATE TABLE IF NOT EXISTS server_rules (id INTEGER PRIMARY KEY CHECK (id = 1), whitelist_mode_enabled INTEGER NOT NULL, access_restriction_enabled INTEGER NOT NULL, min_rating INTEGER NOT NULL, min_steam_level INTEGER NOT NULL)");
    SQL_FastQuery(g_AccessSnapshotDb, "CREATE TABLE IF NOT EXISTS bans (steam_id TEXT, ip_address TEXT, reason TEXT NOT NULL, expires_at INTEGER)");
    SQL_FastQuery(g_AccessSnapshotDb, "CREATE TABLE IF NOT EXISTS whitelist (steam_id TEXT PRIMARY KEY)");
    SQL_FastQuery(g_AccessSnapshotDb, "CREATE TABLE IF NOT EXISTS access_profiles (steam_id TEXT PRIMARY KEY, rating INTEGER NOT NULL, steam_level INTEGER NOT NULL, expires_at INTEGER NOT NULL)");

    SQL_FastQuery(g_AccessSnapshotDb, "CREATE INDEX IF NOT EXISTS idx_bans_steam_id ON bans(steam_id)");
    SQL_FastQuery(g_AccessSnapshotDb, "CREATE INDEX IF NOT EXISTS idx_bans_ip_address ON bans(ip_address)");
    SQL_FastQuery(g_AccessSnapshotDb, "CREATE INDEX IF NOT EXISTS idx_bans_expires_at ON bans(expires_at)");
}

void StartAccessSnapshotTimer()
{
    StopAccessSnapshotTimer();

    float interval = g_AccessSnapshotInterval.FloatValue;
    g_AccessSnapshotTimer = CreateTimer(interval, Timer_RefreshAccessSnapshot, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopAccessSnapshotTimer()
{
    if (g_AccessSnapshotTimer != null)
    {
        delete g_AccessSnapshotTimer;
        g_AccessSnapshotTimer = null;
    }
}

public Action Timer_RefreshAccessSnapshot(Handle timer)
{
    RefreshAccessSnapshot();
    return Plugin_Continue;
}

void RefreshAccessSnapshot()
{
    if (g_AccessSnapshotDb == null)
    {
        return;
    }

    char token[256];
    char url[512];
    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        return;
    }

    if (!ResolvePluginApiConfig(url, sizeof(url), "/access/snapshot", token, sizeof(token)))
    {
        return;
    }

    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", currentPort);

    HTTPRequest request = new HTTPRequest(url);
    request.Timeout = 10;
    request.Post(payload, OnAccessSnapshotResponse);
    delete payload;
}

bool BuildPluginApiUrl(char[] url, int maxLen, const char[] suffix)
{
    g_ApiBaseUrl.GetString(url, maxLen);
    TrimString(url);

    if (url[0] == '\0')
    {
        int currentPort = 0;
        GetCurrentServerPort(currentPort);
        if (ShouldLogPluginConfigState(0, currentPort))
        {
            LogError("Manger online reporter skipped: manger_api_base_url is empty.");
        }
        return false;
    }

    int len = strlen(url);
    if (len > 0 && url[len - 1] == '/')
    {
        url[len - 1] = '\0';
    }

    StrCat(url, maxLen, suffix);
    return true;
}

bool ResolvePluginApiConfig(char[] url, int urlMaxLen, const char[] suffix, char[] token, int tokenMaxLen)
{
    if (!BuildPluginApiUrl(url, urlMaxLen, suffix))
    {
        return false;
    }

    if (!GetCurrentReportToken(token, tokenMaxLen))
    {
        return false;
    }

    return true;
}

public void OnAccessSnapshotResponse(HTTPResponse response, any value, const char[] error)
{
    if (error[0] != '\0')
    {
        LogError("Manger access snapshot refresh failed: %s", error);
        return;
    }

    if (response.Status != HTTPStatus_OK)
    {
        LogError("Manger access snapshot returned HTTP status %d.", response.Status);
        return;
    }

    JSONObject root = view_as<JSONObject>(response.Data);
    if (root == null)
    {
        LogError("Manger access snapshot response was empty.");
        return;
    }

    JSONObject item = view_as<JSONObject>(root.Get("item"));
    if (item == null)
    {
        LogError("Manger access snapshot response missed item.");
        delete root;
        return;
    }

    SaveAccessSnapshot(item);
    delete item;
    delete root;
}

void SaveAccessSnapshot(JSONObject item)
{
    if (g_AccessSnapshotDb == null)
    {
        return;
    }

    if (!SQL_FastQuery(g_AccessSnapshotDb, "BEGIN IMMEDIATE TRANSACTION"))
    {
        LogError("Manger access snapshot: failed to BEGIN transaction.");
        return;
    }

    if (!SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM metadata")
        || !SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM server_rules")
        || !SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM bans")
        || !SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM whitelist")
        || !SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM access_profiles"))
    {
        LogError("Manger access snapshot: cleanup failed, ROLLBACK.");
        SQL_FastQuery(g_AccessSnapshotDb, "ROLLBACK");
        return;
    }

    char version[128];
    char generatedAt[64];
    char expiresAt[64];
    item.GetString("version", version, sizeof(version));
    item.GetString("generated_at", generatedAt, sizeof(generatedAt));
    item.GetString("expires_at", expiresAt, sizeof(expiresAt));

    if (!InsertMetadata("version", version)
        || !InsertMetadata("generated_at", generatedAt)
        || !InsertMetadata("expires_at", expiresAt))
    {
        LogError("Manger access snapshot: metadata insert failed, ROLLBACK.");
        SQL_FastQuery(g_AccessSnapshotDb, "ROLLBACK");
        return;
    }

    char generatedAtUnix[32];
    char expiresAtUnix[32];
    IntToString(item.GetInt("generated_at_unix"), generatedAtUnix, sizeof(generatedAtUnix));
    IntToString(item.GetInt("expires_at_unix"), expiresAtUnix, sizeof(expiresAtUnix));
    if (!InsertMetadata("generated_at_unix", generatedAtUnix)
        || !InsertMetadata("expires_at_unix", expiresAtUnix))
    {
        LogError("Manger access snapshot: metadata unix insert failed, ROLLBACK.");
        SQL_FastQuery(g_AccessSnapshotDb, "ROLLBACK");
        return;
    }

    JSONObject server = view_as<JSONObject>(item.Get("server"));
    if (server != null)
    {
        char query[512];
        Format(query, sizeof(query),
            "INSERT INTO server_rules (id, whitelist_mode_enabled, access_restriction_enabled, min_rating, min_steam_level) VALUES (1, %d, %d, %d, %d)",
            server.GetBool("whitelist_mode_enabled") ? 1 : 0,
            server.GetBool("access_restriction_enabled") ? 1 : 0,
            server.GetInt("min_rating"),
            server.GetInt("min_steam_level"));
        if (!SQL_FastQuery(g_AccessSnapshotDb, query))
        {
            LogError("Manger access snapshot: server_rules insert failed, ROLLBACK.");
            SQL_FastQuery(g_AccessSnapshotDb, "ROLLBACK");
            delete server;
            return;
        }
        delete server;
    }

    JSONArray bans = view_as<JSONArray>(item.Get("bans"));
    bool bansSaved = SaveSnapshotBans(bans);
    if (bans != null)
    {
        delete bans;
    }
    if (!bansSaved)
    {
        SQL_FastQuery(g_AccessSnapshotDb, "ROLLBACK");
        return;
    }

    JSONArray whitelist = view_as<JSONArray>(item.Get("whitelist"));
    bool whitelistSaved = SaveSnapshotWhitelist(whitelist);
    if (whitelist != null)
    {
        delete whitelist;
    }
    if (!whitelistSaved)
    {
        SQL_FastQuery(g_AccessSnapshotDb, "ROLLBACK");
        return;
    }

    JSONArray profiles = view_as<JSONArray>(item.Get("access_profiles"));
    bool profilesSaved = SaveSnapshotAccessProfiles(profiles);
    if (profiles != null)
    {
        delete profiles;
    }
    if (!profilesSaved)
    {
        SQL_FastQuery(g_AccessSnapshotDb, "ROLLBACK");
        return;
    }

    if (!SQL_FastQuery(g_AccessSnapshotDb, "COMMIT"))
    {
        LogError("Manger access snapshot: COMMIT failed, ROLLBACK.");
        SQL_FastQuery(g_AccessSnapshotDb, "ROLLBACK");
    }
}

bool InsertMetadata(const char[] key, const char[] value)
{
    char escapedKey[128];
    char escapedValue[256];
    char query[512];
    SQL_EscapeString(g_AccessSnapshotDb, key, escapedKey, sizeof(escapedKey));
    SQL_EscapeString(g_AccessSnapshotDb, value, escapedValue, sizeof(escapedValue));
    Format(query, sizeof(query), "INSERT INTO metadata (key, value) VALUES ('%s', '%s')", escapedKey, escapedValue);
    if (!SQL_FastQuery(g_AccessSnapshotDb, query))
    {
        LogError("Manger access snapshot: metadata insert failed for key '%s'", key);
        return false;
    }
    return true;
}

bool SaveSnapshotBans(JSONArray bans)
{
    if (bans == null)
    {
        return true;
    }

    for (int i = 0; i < bans.Length; i++)
    {
        JSONObject ban = view_as<JSONObject>(bans.Get(i));
        if (ban == null)
        {
            continue;
        }

        char steamId[64];
        char ipAddress[64];
        char reason[256];
        char expiresAtUnix[32];
        ban.GetString("steam_id", steamId, sizeof(steamId));
        ban.GetString("ip_address", ipAddress, sizeof(ipAddress));
        ban.GetString("reason", reason, sizeof(reason));
        IntToString(ban.GetInt("expires_at_unix"), expiresAtUnix, sizeof(expiresAtUnix));

        char escapedSteamId[128];
        char escapedIpAddress[128];
        char escapedReason[512];
        SQL_EscapeString(g_AccessSnapshotDb, steamId, escapedSteamId, sizeof(escapedSteamId));
        SQL_EscapeString(g_AccessSnapshotDb, ipAddress, escapedIpAddress, sizeof(escapedIpAddress));
        SQL_EscapeString(g_AccessSnapshotDb, reason, escapedReason, sizeof(escapedReason));

        char query[1024];
        Format(query, sizeof(query), "INSERT INTO bans (steam_id, ip_address, reason, expires_at) VALUES ('%s', '%s', '%s', %d)", escapedSteamId, escapedIpAddress, escapedReason, StringToInt(expiresAtUnix));
        if (!SQL_FastQuery(g_AccessSnapshotDb, query))
        {
            LogError("Manger access snapshot: bans insert failed at index %d", i);
            delete ban;
            return false;
        }
        delete ban;
    }

    return true;
}

bool SaveSnapshotWhitelist(JSONArray whitelist)
{
    if (whitelist == null)
    {
        return true;
    }

    for (int i = 0; i < whitelist.Length; i++)
    {
        JSONObject entry = view_as<JSONObject>(whitelist.Get(i));
        if (entry == null)
        {
            continue;
        }

        char steamId[64];
        entry.GetString("steam_id64", steamId, sizeof(steamId));
        char escapedSteamId[128];
        char query[256];
        SQL_EscapeString(g_AccessSnapshotDb, steamId, escapedSteamId, sizeof(escapedSteamId));
        Format(query, sizeof(query), "INSERT OR REPLACE INTO whitelist (steam_id) VALUES ('%s')", escapedSteamId);
        if (!SQL_FastQuery(g_AccessSnapshotDb, query))
        {
            LogError("Manger access snapshot: whitelist insert failed at index %d", i);
            delete entry;
            return false;
        }
        delete entry;
    }

    return true;
}

bool SaveSnapshotAccessProfiles(JSONArray profiles)
{
    if (profiles == null)
    {
        return true;
    }

    for (int i = 0; i < profiles.Length; i++)
    {
        JSONObject profile = view_as<JSONObject>(profiles.Get(i));
        if (profile == null)
        {
            continue;
        }

        char steamId[64];
        profile.GetString("steam_id64", steamId, sizeof(steamId));
        char escapedSteamId[128];
        char query[512];
        SQL_EscapeString(g_AccessSnapshotDb, steamId, escapedSteamId, sizeof(escapedSteamId));
        Format(query, sizeof(query),
            "INSERT OR REPLACE INTO access_profiles (steam_id, rating, steam_level, expires_at) VALUES ('%s', %d, %d, %d)",
            escapedSteamId,
            profile.GetInt("rating"),
            profile.GetInt("steam_level"),
            profile.GetInt("expires_at_unix"));
        if (!SQL_FastQuery(g_AccessSnapshotDb, query))
        {
            LogError("Manger access snapshot: access_profiles insert failed at index %d", i);
            delete profile;
            return false;
        }
        delete profile;
    }

    return true;
}

void StartReportTimer()
{
    StopReportTimer();

    float interval = g_ReportInterval.FloatValue;
    g_ReportTimer = CreateTimer(interval, Timer_ReportOnlinePlayers, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopReportTimer()
{
    if (g_ReportTimer != null)
    {
        delete g_ReportTimer;
        g_ReportTimer = null;
    }
}

void StartBanPollTimer()
{
    StopBanPollTimer();

    float interval = g_ReportInterval.FloatValue;
    g_BanPollTimer = CreateTimer(interval, Timer_PollBans, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopBanPollTimer()
{
    if (g_BanPollTimer != null)
    {
        delete g_BanPollTimer;
        g_BanPollTimer = null;
    }
}

public Action Timer_PollBans(Handle timer)
{
    char token[256];
    char url[512];
    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        return Plugin_Continue;
    }
    if (!ResolvePluginApiConfig(url, sizeof(url), "/bans/poll", token, sizeof(token)))
    {
        return Plugin_Continue;
    }

    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", currentPort);
    // 回传上次版本签名；首次请求或配置变更后为空，后端会返回完整列表
    if (g_BanPollEtag[0] != '\0')
    {
        payload.SetString("etag", g_BanPollEtag);
    }

    HTTPRequest request = new HTTPRequest(url);
    request.Timeout = 10;
    request.Post(payload, OnBanPollResponse);
    delete payload;

    return Plugin_Continue;
}

public void OnBanPollResponse(HTTPResponse response, any value, const char[] error)
{
    if (error[0] != '\0')
    {
        LogError("Manger ban poll failed: %s", error);
        return;
    }

    if (response.Status != HTTPStatus_OK)
    {
        LogError("Manger ban poll returned HTTP status %d.", response.Status);
        return;
    }

    JSONObject data = view_as<JSONObject>(response.Data);
    if (data == null)
    {
        LogError("Manger ban poll response data was null.");
        return;
    }

    // 记录服务端返回的版本签名，供下次轮询回传以启用增量检测
    if (!data.IsNull("etag"))
    {
        data.GetString("etag", g_BanPollEtag, sizeof(g_BanPollEtag));
    }

    JSON rawItems = data.Get("items");
    if (rawItems == null)
    {
        LogError("Manger ban poll response missing items.");
        delete data;
        return;
    }

    JSONArray items = view_as<JSONArray>(rawItems);
    if (items == null)
    {
        delete data;
        return;
    }

    // 预构建在线玩家索引，避免 O(N×M) 遍历
    StringMap steamMap = new StringMap();
    StringMap ipMap = new StringMap();
    for (int client = 1; client <= MaxClients; client++)
    {
        if (!IsClientInGame(client) || IsFakeClient(client))
        {
            continue;
        }
        char clientSteamId64[64];
        char clientIp[64];
        if (GetClientAuthId(client, AuthId_SteamID64, clientSteamId64, sizeof(clientSteamId64), true))
        {
            steamMap.SetValue(clientSteamId64, client);
        }
        if (GetClientIP(client, clientIp, sizeof(clientIp), true))
        {
            ipMap.SetValue(clientIp, client);
        }
    }

    for (int i = 0; i < items.Length; i++)
    {
        JSON rawItem = items.Get(i);
        JSONObject item = view_as<JSONObject>(rawItem);
        KickMatchingBan(item, steamMap, ipMap);
        delete item;
    }

    delete steamMap;
    delete ipMap;
    delete items;
    delete data;
}

void KickMatchingBan(JSONObject item, StringMap steamMap, StringMap ipMap)
{
    char steamId[64];
    char ipAddress[64];
    char reason[256];
    item.GetString("steam_id", steamId, sizeof(steamId));
    if (!item.IsNull("ip_address"))
    {
        item.GetString("ip_address", ipAddress, sizeof(ipAddress));
    }
    else
    {
        ipAddress[0] = '\0';
    }
    item.GetString("reason", reason, sizeof(reason));

    // 用预构建索引 O(1) 查找匹配玩家，避免遍历所有客户端
    int matchedClient = -1;
    if (steamId[0] != '\0' && steamMap.GetValue(steamId, matchedClient))
    {
        // SteamID 匹配
    }
    else if (ipAddress[0] != '\0' && ipMap.GetValue(ipAddress, matchedClient))
    {
        // IP 匹配
    }

    if (matchedClient > 0 && IsClientInGame(matchedClient))
    {
        CompletePolledBanDetails(matchedClient);
        MarkClientDisconnect(matchedClient, CNGOKZ_SESSION_REASON_BANNED_KICKED, reason);
        KickClient(matchedClient, "你已被封禁：%s", reason);
    }
}

void CompletePolledBanDetails(int client)
{
    char token[256];
    char url[512];
    char steamId64[64];
    char ipAddress[64];
    char playerName[128];
    if (!ResolvePluginApiConfig(url, sizeof(url), "/bans/check", token, sizeof(token)))
    {
        return;
    }

    if (!GetClientAuthId(client, AuthId_SteamID64, steamId64, sizeof(steamId64), true))
    {
        return;
    }

    GetClientIP(client, ipAddress, sizeof(ipAddress), true);
    GetClientName(client, playerName, sizeof(playerName));
    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        return;
    }
    JSONObject payload = BuildPluginBanCheckPayload(token, currentPort, steamId64, ipAddress, playerName);
    PostJsonObject(url, payload, OnBanCheckResponse);
    delete payload;
}

bool GetCurrentReportToken(char[] token, int maxLen)
{
    token[0] = '\0';

    if (LibraryExists("cngokz-core") && CNGOKZCore_GetReportToken(token, maxLen))
    {
        return token[0] != '\0';
    }

    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        if (ShouldLogPluginConfigState(7, 0))
        {
            LogError("Manger online reporter skipped: current server port detect failed.");
        }
        return false;
    }

    if (g_HasCachedReportToken && g_CachedReportPort == currentPort)
    {
        strcopy(token, maxLen, g_CachedReportToken);
        return true;
    }

    char portKey[16];
    IntToString(currentPort, portKey, sizeof(portKey));
    if (g_ServerTokenMap == null || !g_ServerTokenMap.GetString(portKey, token, maxLen))
    {
        if (ShouldLogPluginConfigState(6, currentPort))
        {
            LogError("Manger online reporter skipped: no token found for port %d.", currentPort);
        }
        return false;
    }

    TrimString(token);
    if (token[0] == '\0')
    {
        if (ShouldLogPluginConfigState(5, currentPort))
        {
            LogError("Manger online reporter skipped: token for port %d is empty.", currentPort);
        }
        return false;
    }

    strcopy(g_CachedReportToken, sizeof(g_CachedReportToken), token);
    g_CachedReportPort = currentPort;
    g_HasCachedReportToken = true;
    ResetPluginConfigLogState();
    return true;
}


public Action Timer_ReportOnlinePlayers(Handle timer)
{
    char reportUrl[512];
    char reportToken[256];
    if (!ResolvePluginApiConfig(reportUrl, sizeof(reportUrl), "/online-players/report", reportToken, sizeof(reportToken)))
    {
        return Plugin_Continue;
    }

    JSONObject payload = BuildReportPayload(reportToken);
    PostJsonObject(reportUrl, payload, OnReportResponse);
    delete payload;

    return Plugin_Continue;
}

void DebugLog(const char[] format, any ...)
{
    if (g_DebugLog == null || !g_DebugLog.BoolValue)
    {
        return;
    }

    char message[512];
    VFormat(message, sizeof(message), format, 2);
    PrintToServer("[Manger Debug] %s", message);
}

public void OnReportResponse(HTTPResponse response, any value, const char[] error)
{
    if (error[0] != '\0')
    {
        LogError("Manger online reporter failed: %s", error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("Manger online reporter returned HTTP status %d.", response.Status);
    }
}

void ReportClientDisconnect(int client)
{
    if (client <= 0 || client > MaxClients || IsFakeClient(client))
    {
        return;
    }

    char token[256];
    char url[512];
    if (!ResolvePluginApiConfig(url, sizeof(url), "/online-players/disconnect", token, sizeof(token)))
    {
        return;
    }

    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        return;
    }

    char steamId64[64] = "";
    char steamId2[64] = "";
    GetClientAuthId(client, AuthId_SteamID64, steamId64, sizeof(steamId64), true);
    GetClientAuthId(client, AuthId_Steam2, steamId2, sizeof(steamId2), true);
    if (steamId64[0] == '\0' && steamId2[0] == '\0')
    {
        return;
    }

    char reason[32];
    if (g_DisconnectReason[client][0] == '\0')
    {
        strcopy(reason, sizeof(reason), CNGOKZ_SESSION_REASON_PLAYER_QUIT);
    }
    else
    {
        strcopy(reason, sizeof(reason), g_DisconnectReason[client]);
    }

    char detail[256];
    if (g_DisconnectDetail[client][0] == '\0')
    {
        strcopy(detail, sizeof(detail), "玩家断开连接。");
    }
    else
    {
        strcopy(detail, sizeof(detail), g_DisconnectDetail[client]);
    }

    char playerName[128] = "";
    char playerIp[64] = "";
    GetClientName(client, playerName, sizeof(playerName));
    GetClientIP(client, playerIp, sizeof(playerIp), true);

    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", currentPort);
    payload.SetString("steam_id64", steamId64);
    payload.SetString("steam_id", steamId2);
    payload.SetString("player_name", playerName);
    payload.SetString("ip", playerIp);
    payload.SetString("reason", reason);
    payload.SetString("detail", detail);
    PostJsonObject(url, payload, OnDisconnectReportResponse);
    delete payload;
}

public void OnDisconnectReportResponse(HTTPResponse response, any value, const char[] error)
{
    LogHttpPostFailure("disconnect report", response, error);
}

void AppendCommandReason(int startArg, int args, char[] reason, int maxLen)
{
    reason[0] = '\0';
    for (int i = startArg; i <= args; i++)
    {
        char part[192];
        GetCmdArg(i, part, sizeof(part));
        if (reason[0] != '\0')
        {
            StrCat(reason, maxLen, " ");
        }
        StrCat(reason, maxLen, part);
    }

    if (reason[0] == '\0')
    {
        strcopy(reason, maxLen, "未填写");
    }
}

bool IsDecimalString(const char[] value)
{
    int len = strlen(value);
    if (len == 0)
    {
        return false;
    }

    for (int i = 0; i < len; i++)
    {
        if (value[i] < '0' || value[i] > '9')
        {
            return false;
        }
    }
    return true;
}

bool IsSteamId64String(const char[] steamId)
{
    return strlen(steamId) == 17 && StrContains(steamId, "7656119") == 0 && IsDecimalString(steamId);
}

bool IsIpAddressTarget(const char[] target)
{
    int len = strlen(target);
    bool hasDot = false;
    bool hasDigit = false;

    for (int i = 0; i < len; i++)
    {
        if (target[i] >= '0' && target[i] <= '9')
        {
            hasDigit = true;
            continue;
        }

        if (target[i] == '.')
        {
            hasDot = true;
            continue;
        }

        return false;
    }

    return hasDot && hasDigit;
}

bool AppendCharToString(char[] value, int maxLen, int ch)
{
    int len = strlen(value);
    if (len >= maxLen - 1)
    {
        return false;
    }

    value[len] = ch;
    value[len + 1] = '\0';
    return true;
}

void CopyStringRange(const char[] source, int start, int endExclusive, char[] dest, int maxLen)
{
    int destIdx = 0;
    for (int i = start; i < endExclusive && destIdx < maxLen - 1; i++)
    {
        dest[destIdx] = source[i];
        destIdx++;
    }
    dest[destIdx] = '\0';
}

void DecimalAddStrings(const char[] left, const char[] right, char[] output, int maxLen)
{
    char reversed[64];
    int reversedLen = 0;
    int i = strlen(left) - 1;
    int j = strlen(right) - 1;
    int carry = 0;

    while ((i >= 0 || j >= 0 || carry > 0) && reversedLen < sizeof(reversed) - 1)
    {
        int sum = carry;
        if (i >= 0)
        {
            sum += left[i] - '0';
            i--;
        }
        if (j >= 0)
        {
            sum += right[j] - '0';
            j--;
        }

        reversed[reversedLen] = (sum % 10) + '0';
        reversedLen++;
        carry = sum / 10;
    }

    int outIdx = 0;
    for (int k = reversedLen - 1; k >= 0 && outIdx < maxLen - 1; k--)
    {
        output[outIdx] = reversed[k];
        outIdx++;
    }
    output[outIdx] = '\0';
}

void DecimalMultiplyByTwoAndAddSmall(const char[] decimal, int addValue, char[] output, int maxLen)
{
    char reversed[64];
    int reversedLen = 0;
    int carry = addValue;

    for (int i = strlen(decimal) - 1; i >= 0 && reversedLen < sizeof(reversed) - 1; i--)
    {
        int value = (decimal[i] - '0') * 2 + carry;
        reversed[reversedLen] = (value % 10) + '0';
        reversedLen++;
        carry = value / 10;
    }

    while (carry > 0 && reversedLen < sizeof(reversed) - 1)
    {
        reversed[reversedLen] = (carry % 10) + '0';
        reversedLen++;
        carry /= 10;
    }

    int outIdx = 0;
    for (int k = reversedLen - 1; k >= 0 && outIdx < maxLen - 1; k--)
    {
        output[outIdx] = reversed[k];
        outIdx++;
    }
    output[outIdx] = '\0';
}

bool ConvertSteam2ToSteamId64(const char[] steamId2, char[] steamId64, int maxLen)
{
    if (StrContains(steamId2, "STEAM_", false) != 0)
    {
        return false;
    }

    int firstColon = FindCharInString(steamId2, ':');
    if (firstColon == -1)
    {
        return false;
    }

    int secondColon = -1;
    int len = strlen(steamId2);
    for (int i = firstColon + 1; i < len; i++)
    {
        if (steamId2[i] == ':')
        {
            secondColon = i;
            break;
        }
    }
    if (secondColon == -1)
    {
        return false;
    }

    char universe[8];
    char yPart[4];
    char zPart[32];
    CopyStringRange(steamId2, 6, firstColon, universe, sizeof(universe));
    CopyStringRange(steamId2, firstColon + 1, secondColon, yPart, sizeof(yPart));
    CopyStringRange(steamId2, secondColon + 1, len, zPart, sizeof(zPart));

    if (!IsDecimalString(universe) || !IsDecimalString(yPart) || !IsDecimalString(zPart))
    {
        return false;
    }

    int y = StringToInt(yPart);
    if (y < 0 || y > 1)
    {
        return false;
    }

    char accountId[32];
    DecimalMultiplyByTwoAndAddSmall(zPart, y, accountId, sizeof(accountId));
    DecimalAddStrings("76561197960265728", accountId, steamId64, maxLen);
    return IsSteamId64String(steamId64);
}

bool ConvertSteam3ToSteamId64(const char[] steamId3, char[] steamId64, int maxLen)
{
    int len = strlen(steamId3);
    if (len <= 6 || StrContains(steamId3, "[U:1:") != 0 || steamId3[len - 1] != ']')
    {
        return false;
    }

    char accountId[32];
    CopyStringRange(steamId3, 5, len - 1, accountId, sizeof(accountId));
    if (!IsDecimalString(accountId))
    {
        return false;
    }

    DecimalAddStrings("76561197960265728", accountId, steamId64, maxLen);
    return IsSteamId64String(steamId64);
}

bool NormalizePluginSteamId(const char[] input, char[] steamId64, int maxLen)
{
    char value[128];
    strcopy(value, sizeof(value), input);
    TrimString(value);

    if (IsSteamId64String(value))
    {
        strcopy(steamId64, maxLen, value);
        return true;
    }

    if (ConvertSteam2ToSteamId64(value, steamId64, maxLen))
    {
        return true;
    }

    if (ConvertSteam3ToSteamId64(value, steamId64, maxLen))
    {
        return true;
    }

    return false;
}

bool SteamId2SegmentHasValue(int segment, const char[] universe, const char[] yPart, const char[] zPart)
{
    if (segment == 0)
    {
        return universe[0] != '\0';
    }
    if (segment == 1)
    {
        return yPart[0] != '\0';
    }
    return zPart[0] != '\0';
}

bool AppendSteamId2SegmentDigit(int segment, int digit, char[] universe, int universeMaxLen, char[] yPart, int yMaxLen, char[] zPart, int zMaxLen)
{
    if (segment == 0)
    {
        return AppendCharToString(universe, universeMaxLen, digit);
    }
    if (segment == 1)
    {
        return AppendCharToString(yPart, yMaxLen, digit);
    }
    if (segment == 2)
    {
        return AppendCharToString(zPart, zMaxLen, digit);
    }
    return false;
}

bool ReadSteamId2CommandTarget(int firstArg, int args, char[] steamId2, int maxLen, int &nextArg)
{
    char part[64];
    GetCmdArg(firstArg, part, sizeof(part));
    if (StrContains(part, "STEAM_", false) != 0)
    {
        return false;
    }

    char universe[8] = "";
    char yPart[4] = "";
    char zPart[32] = "";
    int segment = 0;

    for (int arg = firstArg; arg <= args; arg++)
    {
        GetCmdArg(arg, part, sizeof(part));
        int start = 0;
        if (arg == firstArg)
        {
            start = 6;
        }
        else if (part[0] != ':' && segment < 2 && SteamId2SegmentHasValue(segment, universe, yPart, zPart))
        {
            segment++;
        }

        int len = strlen(part);
        for (int i = start; i < len; i++)
        {
            if (part[i] == ':')
            {
                if (segment >= 2)
                {
                    return false;
                }
                segment++;
                continue;
            }

            if (part[i] < '0' || part[i] > '9')
            {
                return false;
            }

            if (!AppendSteamId2SegmentDigit(segment, part[i], universe, sizeof(universe), yPart, sizeof(yPart), zPart, sizeof(zPart)))
            {
                return false;
            }
        }

        if (universe[0] != '\0' && yPart[0] != '\0' && zPart[0] != '\0')
        {
            char normalizedSteamId2[64];
            Format(normalizedSteamId2, sizeof(normalizedSteamId2), "STEAM_%s:%s:%s", universe, yPart, zPart);
            char steamId64[64];
            if (!ConvertSteam2ToSteamId64(normalizedSteamId2, steamId64, sizeof(steamId64)))
            {
                return false;
            }

            strcopy(steamId2, maxLen, normalizedSteamId2);
            nextArg = arg + 1;
            return true;
        }
    }

    return false;
}

bool SubmitPluginBan(int client, int target, const char[] banType, const char[] steamId, const char[] ipAddress, const char[] player, int duration, const char[] reason)
{
    char token[256];
    char banUrl[512];
    char adminName[128];
    char adminSteamid[64];
    char normalizedSteamId[64];

    if (StrEqual(banType, "steam"))
    {
        if (!NormalizePluginSteamId(steamId, normalizedSteamId, sizeof(normalizedSteamId)))
        {
            ReplyToCommand(client, "[Manger] SteamID 无效，无法封禁：%s", steamId);
            return false;
        }
    }
    else
    {
        strcopy(normalizedSteamId, sizeof(normalizedSteamId), steamId);
    }

    if (client == 0)
    {
        strcopy(adminName, sizeof(adminName), "CONSOLE");
        adminSteamid[0] = '\0';
    }
    else
    {
        GetClientName(client, adminName, sizeof(adminName));
        GetClientAuthId(client, AuthId_SteamID64, adminSteamid, sizeof(adminSteamid), true);
    }

    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        return false;
    }

    // 先尝试在线提交
    if (ResolvePluginApiConfig(banUrl, sizeof(banUrl), "/bans", token, sizeof(token)))
    {
        JSONObject payload = BuildPluginBanPayload(token, currentPort, banType, normalizedSteamId, ipAddress, player, duration, reason, adminName);
        PostJsonObject(banUrl, payload, OnPluginBanResponse);
        delete payload;
    }
    else
    {
        // API配置无效，尝试离线队列
        ReplyToCommand(client, "[Manger] API未配置，尝试离线队列...");
    }

    // 同时写入离线队列作为备份（幂等键会防止重复应用）
    char targetId[64];
    if (StrEqual(banType, "steam"))
    {
        strcopy(targetId, sizeof(targetId), normalizedSteamId);
    }
    else
    {
        strcopy(targetId, sizeof(targetId), ipAddress);
    }

    if (!QueueEdgeSyncOperation("ban", targetId, banType, player, reason, adminName, adminSteamid, duration))
    {
        ReplyToCommand(client, "[Manger] 离线队列不可用，请确认 cngokz-sync 已加载。");
    }

    if (target > 0 && IsClientInGame(target))
    {
        MarkClientDisconnect(target, CNGOKZ_SESSION_REASON_BANNED_KICKED, reason);
        KickClient(target, "你已被封禁：%s", reason);
    }
    return true;
}

int FindClientBySteamId2(const char[] steamId2)
{
    // 提取输入的 Y:Z 部分，忽略 STEAM_X 中的 universe 差异
    char inputYz[64];
    int firstColon = FindCharInString(steamId2, ':');
    if (firstColon == -1)
    {
        DebugLog("FindClientBySteamId2: input=\"%s\" has no colon, returning -1", steamId2);
        return -1;
    }
    strcopy(inputYz, sizeof(inputYz), steamId2[firstColon + 1]);
    DebugLog("FindClientBySteamId2: input=\"%s\" -> inputYz=\"%s\"", steamId2, inputYz);

    for (int i = 1; i <= MaxClients; i++)
    {
        if (!IsClientInGame(i) || IsFakeClient(i))
        {
            continue;
        }

        char clientSteamId2[64];
        if (GetClientAuthId(i, AuthId_Steam2, clientSteamId2, sizeof(clientSteamId2), true))
        {
            int clientFirstColon = FindCharInString(clientSteamId2, ':');
            if (clientFirstColon != -1)
            {
                DebugLog("FindClientBySteamId2: client[%d] steam=\"%s\" yz=\"%s\" vs inputYz=\"%s\"", i, clientSteamId2, clientSteamId2[clientFirstColon + 1], inputYz);
                if (StrEqual(clientSteamId2[clientFirstColon + 1], inputYz, false))
                {
                    DebugLog("FindClientBySteamId2: MATCH found at client index %d", i);
                    return i;
                }
            }
        }
    }
    DebugLog("FindClientBySteamId2: no match found for input=\"%s\"", steamId2);
    return -1;
}

public Action CommandBan(int client, int args)
{
    if (args == 0 && client > 0)
    {
        DisplayBanTargetMenu(client);
        return Plugin_Handled;
    }

    if (args < 2)
    {
        ReplyToCommand(client, "用法: sm_ban <#userid|name|steamid2> <minutes|0> [reason]");
        return Plugin_Handled;
    }

    // CS:GO 聊天命令可能将 STEAM_X:Y:Z 中的冒号拆分为独立参数。
    // 先按 SteamID2 语义重建目标，再读取时长，避免把 X/Y/Z 误分到时长和理由。
    char targetArg[128] = "";
    char timeArg[32] = "";
    int argIdx = 1;

    // 第一步：提取目标（SteamID 或玩家名/#userid）
    {
        char part[64];
        GetCmdArg(1, part, sizeof(part));
        if (StrContains(part, "STEAM_", false) == 0)
        {
            if (!ReadSteamId2CommandTarget(1, args, targetArg, sizeof(targetArg), argIdx))
            {
                ReplyToCommand(client, "[Manger] SteamID 格式无效，应为 STEAM_X:Y:Z。");
                return Plugin_Handled;
            }
        }
        else
        {
            strcopy(targetArg, sizeof(targetArg), part);
            argIdx = 2;
        }
    }

    // 第二步：提取封禁时长
    if (argIdx > args)
    {
        ReplyToCommand(client, "用法: sm_ban <#userid|name|steamid2> <minutes|0> [reason]");
        return Plugin_Handled;
    }

    if (argIdx <= args)
    {
        GetCmdArg(argIdx, timeArg, sizeof(timeArg));
        argIdx++;
    }

    int duration = StringToInt(timeArg);
    if (duration < 0)
    {
        ReplyToCommand(client, "[Manger] 封禁时长不能为负数。");
        return Plugin_Handled;
    }

    // 第三步：提取封禁理由
    char reason[256];
    AppendCommandReason(argIdx, args, reason, sizeof(reason));

    DebugLog("CommandBan: targetArg=\"%s\" timeArg=\"%s\" duration=%d reason=\"%s\" args=%d", targetArg, timeArg, duration, reason, args);

    // SteamID2 格式 (STEAM_X:Y:Z)：搜索在线玩家或直接提交封禁
    if (StrContains(targetArg, "STEAM_", false) == 0)
    {
        int target = FindClientBySteamId2(targetArg);
        if (target > 0 && IsClientInGame(target))
        {
            char steamId64[64];
            char ipAddress[64];
            char player[128];
            GetClientAuthId(target, AuthId_SteamID64, steamId64, sizeof(steamId64), true);
            GetClientIP(target, ipAddress, sizeof(ipAddress), true);
            GetClientName(target, player, sizeof(player));
            if (SubmitPluginBan(client, target, "steam", steamId64, ipAddress, player, duration, reason))
            {
                ReplyToCommand(client, "[Manger] 封禁已生效。");
            }
        }
        else
        {
            char steamId64[64];
            if (!ConvertSteam2ToSteamId64(targetArg, steamId64, sizeof(steamId64)))
            {
                ReplyToCommand(client, "[Manger] SteamID 格式无效，应为 STEAM_X:Y:Z。");
                return Plugin_Handled;
            }

            if (SubmitPluginBan(client, 0, "steam", steamId64, "", "", duration, reason))
            {
                ReplyToCommand(client, "[Manger] 玩家不在线，封禁记录已上传。");
            }
        }
        return Plugin_Handled;
    }

    // RCON 通过 SteamID64 执行，网站已创建封禁记录
    bool isSteamId64 = IsSteamId64String(targetArg);
    if (client == 0 && isSteamId64)
    {
        ReplyToCommand(client, "[Manger] 封禁记录已创建，玩家将在下次轮询时被踢出。");
        return Plugin_Handled;
    }

    if (isSteamId64)
    {
        if (SubmitPluginBan(client, 0, "steam", targetArg, "", "", duration, reason))
        {
            ReplyToCommand(client, "[Manger] 玩家不在线，封禁记录已上传。");
        }
        return Plugin_Handled;
    }

    // 通过玩家名称或 #userid 查找
    int target = FindTarget(client, targetArg, true);
    if (target <= 0)
    {
        return Plugin_Handled;
    }

    char steamId[64];
    char ipAddress[64];
    char player[128];
    GetClientAuthId(target, AuthId_SteamID64, steamId, sizeof(steamId), true);
    GetClientIP(target, ipAddress, sizeof(ipAddress), true);
    GetClientName(target, player, sizeof(player));

    SubmitPluginBan(client, target, "steam", steamId, ipAddress, player, duration, reason);
    return Plugin_Handled;
}

public Action CommandBanIp(int client, int args)
{
    if (args < 2)
    {
        ReplyToCommand(client, "用法: sm_banip <ip|#userid|name> <minutes|0> [reason]");
        return Plugin_Handled;
    }

    char targetArg[128];
    char timeArg[32];
    GetCmdArg(1, targetArg, sizeof(targetArg));
    GetCmdArg(2, timeArg, sizeof(timeArg));

    int duration = StringToInt(timeArg);
    if (duration < 0)
    {
        ReplyToCommand(client, "[Manger] 封禁时长不能为负数。");
        return Plugin_Handled;
    }

    char reason[256];
    AppendCommandReason(3, args, reason, sizeof(reason));

    char steamId[64] = "";
    char ipAddress[64];
    char player[128] = "";
    int target = FindTarget(client, targetArg, true, false);
    if (target > 0)
    {
        GetClientAuthId(target, AuthId_SteamID64, steamId, sizeof(steamId), true);
        GetClientIP(target, ipAddress, sizeof(ipAddress), true);
        GetClientName(target, player, sizeof(player));
    }
    else
    {
        strcopy(ipAddress, sizeof(ipAddress), targetArg);
    }

    SubmitPluginBan(client, target, "ip", steamId, ipAddress, player, duration, reason);
    return Plugin_Handled;
}

public Action CommandAddBan(int client, int args)
{
    if (args < 2)
    {
        ReplyToCommand(client, "用法: sm_addban <minutes|0> <steamid> [reason]");
        return Plugin_Handled;
    }

    char timeArg[32];
    char steamId[128];
    GetCmdArg(1, timeArg, sizeof(timeArg));

    int reasonStart = 3;
    char part[64];
    GetCmdArg(2, part, sizeof(part));
    if (StrContains(part, "STEAM_", false) == 0)
    {
        if (!ReadSteamId2CommandTarget(2, args, steamId, sizeof(steamId), reasonStart))
        {
            ReplyToCommand(client, "[Manger] SteamID 格式无效，应为 STEAM_X:Y:Z。");
            return Plugin_Handled;
        }
    }
    else
    {
        strcopy(steamId, sizeof(steamId), part);
    }

    int duration = StringToInt(timeArg);
    if (duration < 0)
    {
        ReplyToCommand(client, "[Manger] 封禁时长不能为负数。");
        return Plugin_Handled;
    }

    char reason[256];
    AppendCommandReason(reasonStart, args, reason, sizeof(reason));
    SubmitPluginBan(client, 0, "steam", steamId, "", "", duration, reason);
    return Plugin_Handled;
}

public Action CommandUnban(int client, int args)
{
    if (args < 1)
    {
        ReplyToCommand(client, "用法: sm_unban <steamid|ip> [reason]");
        return Plugin_Handled;
    }

    char target[128];
    char reason[256];
    char token[256];
    char url[512];
    char adminName[128];
    char adminSteamid[64];

    GetCmdArg(1, target, sizeof(target));
    if (!IsSteamId64String(target) && !IsIpAddressTarget(target))
    {
        ReplyToCommand(client, "[Manger] 游戏内解封玩家请使用 SteamID64。");
        return Plugin_Handled;
    }
    AppendCommandReason(2, args, reason, sizeof(reason));

    // 处理服务器控制台（client=0）的情况
    if (client == 0)
    {
        strcopy(adminName, sizeof(adminName), "CONSOLE");
        adminSteamid[0] = '\0';
        g_UnbanAdminUserId = 0;
    }
    else
    {
        GetClientName(client, adminName, sizeof(adminName));
        GetClientAuthId(client, AuthId_SteamID64, adminSteamid, sizeof(adminSteamid), true);
        g_UnbanAdminUserId = GetClientUserId(client);
    }

    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        ReplyToCommand(client, "[Manger] 无法获取服务器端口。");
        return Plugin_Handled;
    }

    // 尝试在线解封
    if (ResolvePluginApiConfig(url, sizeof(url), "/bans/unban", token, sizeof(token)))
    {
        JSONObject payload = BuildPluginUnbanPayload(token, currentPort, target, reason, adminName, adminSteamid);
        PostJsonObject(url, payload, OnPluginUnbanResponse, g_UnbanAdminUserId);
        delete payload;
    }
    else
    {
        ReplyToCommand(client, "[Manger] API未配置，使用离线队列...");
    }

    // 同时写入离线队列作为备份
    char targetType[16];
    if (IsSteamId64String(target))
    {
        strcopy(targetType, sizeof(targetType), "steam");
    }
    else
    {
        strcopy(targetType, sizeof(targetType), "ip");
    }

    if (!QueueEdgeSyncOperation("unban", target, targetType, "", reason, adminName, adminSteamid, 0))
    {
        ReplyToCommand(client, "[Manger] 离线队列不可用，请确认 cngokz-sync 已加载。");
    }

    return Plugin_Handled;
}

public Action ChatHook(int client, const char[] command, int argc)
{
    if (!g_WaitingOwnReason[client])
    {
        return Plugin_Continue;
    }

    char reason[256];
    GetCmdArgString(reason, sizeof(reason));
    StripQuotes(reason);
    g_WaitingOwnReason[client] = false;

    if (StrEqual(reason, "!noreason"))
    {
        PrintToChat(client, "[Manger] 已取消自定义理由。");
        return Plugin_Handled;
    }

    SubmitMenuBan(client, reason);
    return Plugin_Handled;
}

void DisplayBanTargetMenu(int client)
{
    Menu menu = new Menu(MenuHandler_BanTarget);
    menu.SetTitle("选择要封禁的玩家");

    for (int target = 1; target <= MaxClients; target++)
    {
        if (!IsClientInGame(target) || IsFakeClient(target))
        {
            continue;
        }

        char userid[16];
        char name[128];
        IntToString(GetClientUserId(target), userid, sizeof(userid));
        GetClientName(target, name, sizeof(name));
        menu.AddItem(userid, name);
    }

    menu.Display(client, MENU_TIME_FOREVER);
}

public int MenuHandler_BanTarget(Menu menu, MenuAction action, int client, int item)
{
    if (action == MenuAction_End)
    {
        delete menu;
        return 0;
    }

    if (action == MenuAction_Select)
    {
        char userid[16];
        menu.GetItem(item, userid, sizeof(userid));
        int target = GetClientOfUserId(StringToInt(userid));
        if (target > 0)
        {
            g_BanTarget[client] = target;
            DisplayBanTimeMenu(client);
        }
    }

    return 0;
}

void DisplayBanTimeMenu(int client)
{
    Menu menu = new Menu(MenuHandler_BanTime);
    menu.SetTitle("选择封禁时长");
    menu.AddItem("0", "永久");
    menu.AddItem("10", "10 分钟");
    menu.AddItem("30", "30 分钟");
    menu.AddItem("60", "1 小时");
    menu.AddItem("1440", "1 天");
    menu.AddItem("10080", "1 周");
    menu.Display(client, MENU_TIME_FOREVER);
}

public int MenuHandler_BanTime(Menu menu, MenuAction action, int client, int item)
{
    if (action == MenuAction_End)
    {
        delete menu;
        return 0;
    }

    if (action == MenuAction_Select)
    {
        char minutes[16];
        menu.GetItem(item, minutes, sizeof(minutes));
        g_BanTime[client] = StringToInt(minutes);
        DisplayBanReasonMenu(client);
    }

    return 0;
}

void DisplayBanReasonMenu(int client)
{
    Menu menu = new Menu(MenuHandler_BanReason);
    menu.SetTitle("选择封禁理由");
    menu.AddItem("作弊", "作弊");
    menu.AddItem("恶意行为", "恶意行为");
    menu.AddItem("辱骂玩家", "辱骂玩家");
    menu.AddItem("own", "自定义理由");
    menu.Display(client, MENU_TIME_FOREVER);
}

public int MenuHandler_BanReason(Menu menu, MenuAction action, int client, int item)
{
    if (action == MenuAction_End)
    {
        delete menu;
        return 0;
    }

    if (action == MenuAction_Select)
    {
        char reason[256];
        menu.GetItem(item, reason, sizeof(reason));
        if (StrEqual(reason, "own"))
        {
            g_WaitingOwnReason[client] = true;
            PrintToChat(client, "[Manger] 请在聊天输入自定义封禁理由，输入 !noreason 取消。");
            return 0;
        }

        SubmitMenuBan(client, reason);
    }

    return 0;
}

void SubmitMenuBan(int client, const char[] reason)
{
    int target = g_BanTarget[client];
    if (target <= 0 || !IsClientInGame(target))
    {
        PrintToChat(client, "[Manger] 目标玩家已离线。");
        return;
    }

    char steamId[64];
    char ipAddress[64];
    char player[128];
    GetClientAuthId(target, AuthId_SteamID64, steamId, sizeof(steamId), true);
    GetClientIP(target, ipAddress, sizeof(ipAddress), true);
    GetClientName(target, player, sizeof(player));
    SubmitPluginBan(client, target, "steam", steamId, ipAddress, player, g_BanTime[client], reason);
}

public void OnClientAuthorized(int client, const char[] auth)
{
    if (IsFakeClient(client) || !IsClientConnected(client))
    {
        return;
    }

    // 准入接口已包含封禁检查，无需单独调用封禁校验
    SubmitAccessCheck(client);
}

void SubmitAccessCheck(int client)
{
    if (client <= 0 || client > MaxClients || !IsClientConnected(client) || IsFakeClient(client))
    {
        return;
    }

    char token[256];
    char url[512];
    char steamId64[64];
    char ipAddress[64];
    char playerName[128];
    if (!ResolvePluginApiConfig(url, sizeof(url), "/access/check", token, sizeof(token)))
    {
        return;
    }

    if (!GetClientAuthId(client, AuthId_SteamID64, steamId64, sizeof(steamId64), true))
    {
        return;
    }

    GetClientIP(client, ipAddress, sizeof(ipAddress), true);
    GetClientName(client, playerName, sizeof(playerName));
    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        return;
    }
    JSONObject payload = BuildPluginAccessCheckPayload(token, currentPort, steamId64, ipAddress, playerName);
    int userId = GetClientUserId(client);
    PostJsonObject(url, payload, OnAccessCheckResponse, userId);
    delete payload;
}

bool PostJsonObject(const char[] url, JSONObject payload, HTTPRequestCallback callback, any value = 0)
{
    if (payload == null)
    {
        LogError("Manger failed to create JSON payload for %s.", url);
        return false;
    }

    HTTPRequest request = new HTTPRequest(url);
    request.Timeout = 10;
    request.Post(payload, callback, value);
    return true;
}

void LogHttpPostFailure(const char[] label, HTTPResponse response, const char[] error)
{
    if (error[0] != '\0')
    {
        LogError("Manger %s failed: %s", label, error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("Manger %s returned HTTP status %d.", label, response.Status);
    }
}

public void OnPluginBanResponse(HTTPResponse response, any value, const char[] error)
{
    LogHttpPostFailure("ban submit", response, error);
}

public void OnPluginUnbanResponse(HTTPResponse response, any value, const char[] error)
{
    int client = GetClientOfUserId(value);

    if (error[0] != '\0')
    {
        LogError("Manger unban failed: %s", error);
        if (client > 0)
        {
            PrintToChat(client, "[Manger] 解封请求失败：%s", error);
        }
        return;
    }

    if (response.Status >= HTTPStatus_OK && response.Status < HTTPStatus_MultipleChoices)
    {
        // 成功解封
        if (client > 0)
        {
            PrintToChat(client, "[Manger] 成功解封该玩家，后台面板中同步解除封禁。");
        }
        return;
    }

    // HTTP 错误
    if (response.Status == HTTPStatus_BadRequest)
    {
        JSONObject data = view_as<JSONObject>(response.Data);
        if (data != null)
        {
            char message[256];
            // 后端可能使用 "message" 或 "error" 作为错误 key
            if (data.GetString("message", message, sizeof(message)) && message[0] != '\0')
            {
                // nothing more needed, message already filled
            }
            else if (data.GetString("error", message, sizeof(message)) && message[0] != '\0')
            {
                // nothing more needed
            }
            else
            {
                strcopy(message, sizeof(message), "解封失败，请稍后重试");
            }

            if (client > 0)
            {
                PrintToChat(client, "[Manger] %s", message);
            }
            LogError("Manger unban failed: %s", message);
            delete data;
        }
        else
        {
            if (client > 0)
            {
                PrintToChat(client, "[Manger] 解封请求失败，请稍后重试。");
            }
        }
    }
    else
    {
        LogError("Manger unban returned HTTP status %d.", response.Status);
        if (client > 0)
        {
            PrintToChat(client, "[Manger] 解封请求失败，HTTP 状态码 %d。", response.Status);
        }
    }
}

public void OnBanCheckResponse(HTTPResponse response, any value, const char[] error)
{
    LogHttpPostFailure("ban check", response, error);
}

public void OnAccessCheckResponse(HTTPResponse response, any value, const char[] error)
{
    LogHttpPostFailure("access check", response, error);
    if (error[0] != '\0' || response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        OfflineAccessCheck(value);
        return;
    }

    int client = GetClientOfUserId(value);
    if (client <= 0 || !IsClientConnected(client))
    {
        return;
    }

    JSONObject data = view_as<JSONObject>(response.Data);
    if (data == null)
    {
        return;
    }

    JSON rawResult = data.Get("result");
    if (rawResult == null)
    {
        delete data;
        return;
    }

    JSONObject result = view_as<JSONObject>(rawResult);
    if (result == null)
    {
        delete data;
        return;
    }

    bool allowed = result.GetBool("allowed");
    if (!allowed)
    {
        char message[256];
        char failureCode[64];
        char accessMethod[64];
        result.GetString("message", message, sizeof(message));
        result.GetString("failure_code", failureCode, sizeof(failureCode));
        result.GetString("access_method", accessMethod, sizeof(accessMethod));
        if (StrEqual(failureCode, "banned") || StrEqual(failureCode, "linked_ip_banned") || StrEqual(accessMethod, "banned"))
        {
            MarkClientDisconnect(client, CNGOKZ_SESSION_REASON_BANNED_KICKED, message);
        }
        else
        {
            MarkClientDisconnect(client, CNGOKZ_SESSION_REASON_ACCESS_REJECTED, message);
        }
        KickClient(client, "%s", message);
    }

    delete result;
    delete data;
}

void OfflineAccessCheck(any userId)
{
    int client = GetClientOfUserId(userId);
    if (client <= 0 || !IsClientInGame(client))
    {
        return;
    }

    if (g_AccessSnapshotDb == null || !IsAccessSnapshotUsable())
    {
        if (ShouldFailOpenAccessCheck())
        {
            DebugLog("Access check fallback allowed userId %d because local snapshot is unavailable.", userId);
            return;
        }
        MarkClientDisconnect(client, CNGOKZ_SESSION_REASON_ACCESS_REJECTED, "访问控制服务暂时不可用。");
        KickClient(client, "访问控制服务暂时不可用，请稍后再试。");
        return;
    }

    char steamId[64];
    char ipAddress[64];
    GetClientAuthId(client, AuthId_SteamID64, steamId, sizeof(steamId), true);
    GetClientIP(client, ipAddress, sizeof(ipAddress), true);

    char reason[256];
    if (FindOfflineBan(steamId, ipAddress, reason, sizeof(reason)))
    {
        MarkClientDisconnect(client, CNGOKZ_SESSION_REASON_BANNED_KICKED, reason);
        KickClient(client, "你已被封禁：%s", reason);
        return;
    }

    if (!OfflineRulesAllowClient(steamId))
    {
        if (ShouldFailOpenAccessCheck())
        {
            DebugLog("Access check fallback allowed userId %d because offline whitelist/access rules could not confirm eligibility.", userId);
            return;
        }
        MarkClientDisconnect(client, CNGOKZ_SESSION_REASON_ACCESS_REJECTED, "本地访问快照未确认玩家满足进入条件。");
        KickClient(client, "你的白名单状态无法确认，请稍后再试。");
        return;
    }
}

bool ShouldFailOpenAccessCheck()
{
    return g_AccessFailOpen == null || g_AccessFailOpen.BoolValue;
}

bool IsAccessSnapshotUsable()
{
    char expiresAtUnixText[32];
    if (!GetMetadataValue("expires_at_unix", expiresAtUnixText, sizeof(expiresAtUnixText)))
    {
        return false;
    }

    int expiresAtUnix = StringToInt(expiresAtUnixText);
    if (expiresAtUnix <= 0)
    {
        return false;
    }

    return GetTime() < expiresAtUnix;
}

bool GetMetadataValue(const char[] key, char[] value, int maxLen)
{
    char escapedKey[128];
    char query[256];
    SQL_EscapeString(g_AccessSnapshotDb, key, escapedKey, sizeof(escapedKey));
    Format(query, sizeof(query), "SELECT value FROM metadata WHERE key = '%s'", escapedKey);

    DBResultSet results = SQL_Query(g_AccessSnapshotDb, query);
    if (results == null)
    {
        return false;
    }

    bool found = false;
    if (SQL_FetchRow(results))
    {
        SQL_FetchString(results, 0, value, maxLen);
        found = true;
    }
    delete results;
    return found;
}

bool FindOfflineBan(const char[] steamId, const char[] ipAddress, char[] reason, int maxLen)
{
    char escapedSteamId[128];
    char escapedIpAddress[128];
    char query[512];
    SQL_EscapeString(g_AccessSnapshotDb, steamId, escapedSteamId, sizeof(escapedSteamId));
    SQL_EscapeString(g_AccessSnapshotDb, ipAddress, escapedIpAddress, sizeof(escapedIpAddress));
    Format(query, sizeof(query), "SELECT reason FROM bans WHERE (steam_id = '%s' OR ip_address = '%s') AND (expires_at IS NULL OR expires_at = 0 OR expires_at > %d) LIMIT 1", escapedSteamId, escapedIpAddress, GetTime());

    DBResultSet results = SQL_Query(g_AccessSnapshotDb, query);
    if (results == null)
    {
        return false;
    }

    bool found = false;
    if (SQL_FetchRow(results))
    {
        SQL_FetchString(results, 0, reason, maxLen);
        found = true;
    }
    delete results;
    return found;
}

bool OfflineRulesAllowClient(const char[] steamId)
{
    DBResultSet rules = SQL_Query(g_AccessSnapshotDb, "SELECT whitelist_mode_enabled, access_restriction_enabled, min_rating, min_steam_level FROM server_rules WHERE id = 1");
    if (rules == null)
    {
        return false;
    }

    if (!SQL_FetchRow(rules))
    {
        delete rules;
        return false;
    }

    bool whitelistMode = SQL_FetchInt(rules, 0) == 1;
    bool accessRestriction = SQL_FetchInt(rules, 1) == 1;
    int minRating = SQL_FetchInt(rules, 2);
    int minSteamLevel = SQL_FetchInt(rules, 3);
    delete rules;

    if (whitelistMode && !OfflineWhitelistContains(steamId))
    {
        return false;
    }

    if (accessRestriction && !OfflineProfileMeetsRequirement(steamId, minRating, minSteamLevel))
    {
        return false;
    }

    return true;
}

bool OfflineWhitelistContains(const char[] steamId)
{
    char escapedSteamId[128];
    char query[256];
    SQL_EscapeString(g_AccessSnapshotDb, steamId, escapedSteamId, sizeof(escapedSteamId));
    Format(query, sizeof(query), "SELECT 1 FROM whitelist WHERE steam_id = '%s' LIMIT 1", escapedSteamId);

    DBResultSet results = SQL_Query(g_AccessSnapshotDb, query);
    if (results == null)
    {
        return false;
    }
    bool found = SQL_FetchRow(results);
    delete results;
    return found;
}

bool OfflineProfileMeetsRequirement(const char[] steamId, int minRating, int minSteamLevel)
{
    char escapedSteamId[128];
    char query[256];
    SQL_EscapeString(g_AccessSnapshotDb, steamId, escapedSteamId, sizeof(escapedSteamId));
    Format(query, sizeof(query), "SELECT rating, steam_level FROM access_profiles WHERE steam_id = '%s' AND expires_at > %d LIMIT 1", escapedSteamId, GetTime());

    DBResultSet results = SQL_Query(g_AccessSnapshotDb, query);
    if (results == null)
    {
        return false;
    }

    if (!SQL_FetchRow(results))
    {
        delete results;
        return false;
    }

    int rating = SQL_FetchInt(results, 0);
    int steamLevel = SQL_FetchInt(results, 1);
    delete results;
    return rating >= minRating && steamLevel >= minSteamLevel;
}

JSONObject BuildReportPayload(const char[] reportToken)
{
    int currentPort = 0;
    GetCurrentServerPort(currentPort);

    char currentMap[64];
    GetCurrentMap(currentMap, sizeof(currentMap));

    JSONObject root = new JSONObject();
    root.SetInt("port", currentPort);
    root.SetString("report_token", reportToken);
    root.SetString("current_map", currentMap);

    JSONArray players = new JSONArray();
    for (int client = 1; client <= MaxClients; client++)
    {
        if (!IsClientInGame(client) || IsFakeClient(client))
        {
            continue;
        }

        char player[128];
        char steamId64[64];
        char playerIp[64];
        GetClientName(client, player, sizeof(player));
        GetClientAuthId(client, AuthId_SteamID64, steamId64, sizeof(steamId64), true);
        GetClientIP(client, playerIp, sizeof(playerIp), true);

        JSONObject entry = new JSONObject();
        entry.SetString("name", player);
        entry.SetString("steam_id64", steamId64);
        entry.SetString("ip", playerIp);
        entry.SetInt("ping", RoundToNearest(GetClientAvgLatency(client, NetFlow_Both) * 1000.0));
        entry.SetInt("server_port", currentPort);
        entry.SetInt("connected_seconds", RoundToNearest(GetClientTime(client)));
        players.Push(entry);
        delete entry;
    }

    root.Set("players", players);
    delete players;
    return root;
}

JSONObject BuildPluginBanPayload(const char[] token, int port, const char[] banType, const char[] steamId, const char[] ipAddress, const char[] player, int duration, const char[] reason, const char[] operatorName)
{
    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", port);
    payload.SetString("ban_type", banType);
    payload.SetString("steam_id", steamId);
    payload.SetString("ip_address", ipAddress);
    payload.SetString("player", player);
    payload.SetInt("duration_minutes", duration);
    payload.SetString("reason", reason);
    payload.SetString("operator_name", operatorName);
    return payload;
}

JSONObject BuildPluginUnbanPayload(const char[] token, int port, const char[] target, const char[] reason, const char[] operatorName, const char[] operatorSteamid)
{
    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", port);
    payload.SetString("target", target);
    payload.SetString("reason", reason);
    payload.SetString("operator_name", operatorName);
    if (operatorSteamid[0] != '\0')
    {
        payload.SetString("operator_steamid", operatorSteamid);
    }
    return payload;
}

JSONObject BuildPluginBanCheckPayload(const char[] token, int port, const char[] steamId, const char[] ipAddress, const char[] player)
{
    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", port);
    payload.SetInt("server_port", port);
    payload.SetString("steam_id", steamId);
    payload.SetString("ip_address", ipAddress);
    payload.SetString("player", player);
    return payload;
}

JSONObject BuildPluginAccessCheckPayload(const char[] token, int port, const char[] steamId64, const char[] ipAddress, const char[] player)
{
    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", port);
    payload.SetInt("server_port", port);
    payload.SetString("steam_id64", steamId64);
    payload.SetString("ip_address", ipAddress);
    payload.SetString("player", player);
    return payload;
}

void StartStatusReportTimer()
{
    StopStatusReportTimer();

    float interval = g_StatusReportInterval.FloatValue;
    g_StatusReportTimer = CreateTimer(interval, Timer_ReportServerStatus, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopStatusReportTimer()
{
    if (g_StatusReportTimer != null)
    {
        delete g_StatusReportTimer;
        g_StatusReportTimer = null;
    }
}

public Action Timer_ReportServerStatus(Handle timer)
{
    g_LastTickrate = GetTickrate();
    ReportServerStatus();
    return Plugin_Continue;
}


void ReportServerStatus()
{
    char token[256];
    char url[512];
    if (!ResolvePluginApiConfig(url, sizeof(url), "/server-status", token, sizeof(token)))
    {
        return;
    }

    JSONObject payload = BuildServerStatusPayload(token);
    PostJsonObject(url, payload, OnServerStatusResponse);
    delete payload;
}

JSONObject BuildServerStatusPayload(const char[] token)
{
    int currentPort = 0;
    GetCurrentServerPort(currentPort);

    float fps = GetServerFrameRate();
    float cpu = GetServerCpuUsage();
    float tickrate = float(g_LastTickrate);
    int uptime = GetTime() - g_ServerStartTime;
    int playersCount = 0;
    int maxPlayers = GetMaxHumanPlayers();

    for (int client = 1; client <= MaxClients; client++)
    {
        if (IsClientInGame(client) && !IsFakeClient(client))
        {
            playersCount++;
        }
    }

    char currentMap[128];
    GetCurrentMap(currentMap, sizeof(currentMap));

    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", currentPort);
    payload.SetFloat("fps", fps);
    payload.SetFloat("cpu_usage", cpu);
    payload.SetFloat("tickrate", tickrate);
    payload.SetInt("uptime_seconds", uptime);
    payload.SetInt("players_count", playersCount);
    payload.SetInt("max_players", maxPlayers);
    payload.SetString("current_map", currentMap);
    return payload;
}

public void OnServerStatusResponse(HTTPResponse response, any value, const char[] error)
{
    if (error[0] != '\0')
    {
        LogError("Manger server status report failed: %s", error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("Manger server status returned HTTP status %d.", response.Status);
    }
}

float GetServerFrameRate()
{
    return GetTickInterval() > 0.0 ? 1.0 / GetTickInterval() : 0.0;
}

float GetServerCpuUsage()
{
    if (g_CpuUsageCvar != null)
    {
        return g_CpuUsageCvar.FloatValue;
    }
    return 0.0;
}

int GetTickrate()
{
    if (g_TickrateCvar != null)
    {
        int maxUpdateRate = g_TickrateCvar.IntValue;
        if (maxUpdateRate > 0)
        {
            return maxUpdateRate;
        }
    }
    return 64;
}
