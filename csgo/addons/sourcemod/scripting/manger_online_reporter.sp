#include <sourcemod>
#include <adt_array>
#include <adt_trie>
#include <ripext>

// Edge Sync Agent Native函数声明
native int EdgeSync_EnqueueOperation(const char[] operation, const char[] target, const char[] targetType, const char[] playerName, const char[] reason, const char[] operatorName, const char[] operatorSteamid, int durationMinutes);
native bool EdgeSync_IsOnline();
native int EdgeSync_GetPendingCount();

#define DEFAULT_API_BASE_URL "http://127.0.0.1:8080/api/plugin"
#define DEFAULT_REPORT_INTERVAL "5.0"
#define DEFAULT_ACCESS_SNAPSHOT_INTERVAL "300.0"
#define DEFAULT_STATUS_REPORT_INTERVAL "30.0"
#define ACCESS_SNAPSHOT_DB "manger_access_snapshot"
#define MAX_REPORT_PAYLOAD 8192
#define MAX_BAN_PAYLOAD 2048
#define MAX_SERVER_CONFIGS 4096
#define MAX_SERVER_TOKEN 256
#define MAX_STATUS_PAYLOAD 1024

ConVar g_ApiBaseUrl;
ConVar g_ReportInterval;
ConVar g_AccessSnapshotInterval;
ConVar g_StatusReportInterval;
Handle g_ReportTimer = null;
Handle g_BanPollTimer = null;
Handle g_AccessSnapshotTimer = null;
Handle g_StatusReportTimer = null;
Database g_AccessSnapshotDb = null;
StringMap g_ServerTokenMap = null;
ArrayList g_ServerPorts = null;
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

public Plugin myinfo =
{
    name = "Manger Online Reporter",
    author = "XBDJ",
    description = "Reports CS:GO online players and server status to the Manger backend.",
    version = "0.5.1",
    url = ""
};

public void OnPluginStart()
{
    g_ApiBaseUrl = CreateConVar("manger_api_base_url", DEFAULT_API_BASE_URL, "Manger plugin API base URL.");
    g_ReportInterval = CreateConVar("manger_report_interval", DEFAULT_REPORT_INTERVAL, "Online player report interval in seconds.", _, true, 5.0);
    g_AccessSnapshotInterval = CreateConVar("manger_access_snapshot_interval", DEFAULT_ACCESS_SNAPSHOT_INTERVAL, "Access snapshot refresh interval in seconds.", _, true, 30.0);
    g_StatusReportInterval = CreateConVar("manger_status_report_interval", DEFAULT_STATUS_REPORT_INTERVAL, "Server status report interval in seconds.", _, true, 10.0);

    HookConVarChange(g_ApiBaseUrl, OnPluginConfigChanged);
    HookConVarChange(g_ReportInterval, OnPluginConfigChanged);
    HookConVarChange(g_AccessSnapshotInterval, OnPluginConfigChanged);
    HookConVarChange(g_StatusReportInterval, OnStatusReportIntervalChanged);

    RegAdminCmd("sm_ban", CommandBan, ADMFLAG_BAN, "sm_ban <#userid|name> <minutes|0> [reason]");
    RegAdminCmd("sm_banip", CommandBanIp, ADMFLAG_BAN, "sm_banip <ip|#userid|name> <minutes|0> [reason]");
    RegAdminCmd("sm_addban", CommandAddBan, ADMFLAG_RCON, "sm_addban <minutes|0> <steamid> [reason]");
    RegAdminCmd("sm_unban", CommandUnban, ADMFLAG_UNBAN, "sm_unban <steamid|ip> [reason]");
    RegServerCmd("manger_server", CommandServerMapping, "Register one reporter server mapping.");
    AddCommandListener(ChatHook, "say");
    AddCommandListener(ChatHook, "say_team");

    if (g_ServerTokenMap == null)
    {
        g_ServerTokenMap = new StringMap();
    }
    if (g_ServerPorts == null)
    {
        g_ServerPorts = new ArrayList();
    }

    g_ServerStartTime = GetTime();
    ResetServerTokenMappings();
    LoadReporterConfig();
    AutoExecConfig(false, "manger_online_reporter");
    InitAccessSnapshotDb();
    StartReportTimer();
    StartBanPollTimer();
    StartAccessSnapshotTimer();
    StartStatusReportTimer();
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
    if (g_ServerPorts != null)
    {
        g_ServerPorts.Clear();
    }
}

void InvalidatePluginConfigCache()
{
    g_CachedReportToken[0] = '\0';
    g_CachedReportPort = -1;
    g_HasCachedReportToken = false;
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
    port = GetConVarInt(FindConVar("hostport"));
    return port > 0;
}

bool ParseConfigValueLine(const char[] line, const char[] key, char[] value, int maxLen)
{
    value[0] = '\0';
    if (StrContains(line, key, false) != 0)
    {
        return false;
    }

    int firstQuote = FindCharInString(line, '"');
    if (firstQuote < 0)
    {
        return false;
    }

    int secondQuote = FindCharInString(line[firstQuote + 1], '"');
    if (secondQuote < 0)
    {
        return false;
    }
    secondQuote += firstQuote + 1;

    int copyLength = secondQuote - firstQuote - 1;
    if (copyLength >= maxLen)
    {
        copyLength = maxLen - 1;
    }

    for (int i = 0; i < copyLength; i++)
    {
        value[i] = line[firstQuote + 1 + i];
    }
    value[copyLength] = '\0';
    return true;
}

void ApplyConfigConVarValue(const char[] name, const char[] value)
{
    if (StrEqual(name, "manger_api_base_url"))
    {
        g_ApiBaseUrl.SetString(value);
        return;
    }

    if (StrEqual(name, "manger_report_interval"))
    {
        g_ReportInterval.SetString(value);
        return;
    }

    if (StrEqual(name, "manger_access_snapshot_interval"))
    {
        g_AccessSnapshotInterval.SetString(value);
    }
}

void LoadReporterConfig()
{
    char path[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, path, sizeof(path), "../../cfg/sourcemod/manger_online_reporter.cfg");

    File file = OpenFile(path, "r");
    if (file == null)
    {
        LogError("Manger reporter config load failed: cannot open %s.", path);
        InvalidatePluginConfigCache();
        ResetPluginConfigLogState();
        return;
    }

    char line[512];
    while (!file.EndOfFile() && file.ReadLine(line, sizeof(line)))
    {
        TrimString(line);
        if (line[0] == '\0' || (line[0] == '/' && line[1] == '/'))
        {
            continue;
        }

        char value[256];
        if (ParseConfigValueLine(line, "manger_api_base_url", value, sizeof(value)))
        {
            ApplyConfigConVarValue("manger_api_base_url", value);
            continue;
        }
        if (ParseConfigValueLine(line, "manger_report_interval", value, sizeof(value)))
        {
            ApplyConfigConVarValue("manger_report_interval", value);
            continue;
        }
        if (ParseConfigValueLine(line, "manger_access_snapshot_interval", value, sizeof(value)))
        {
            ApplyConfigConVarValue("manger_access_snapshot_interval", value);
        }
    }

    delete file;
    // Server token mappings are loaded via manger_server commands in config
    // by AutoExecConfig, no need to parse them here
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

    char existingToken[MAX_SERVER_TOKEN];
    if (g_ServerTokenMap.GetString(portKey, existingToken, sizeof(existingToken)))
    {
        LogMessage("Manger server mapping port %d overridden by later config row.", port);
    }
    else
    {
        g_ServerPorts.Push(port);
    }

    g_ServerTokenMap.SetString(portKey, trimmedToken);
}

public void OnPluginEnd()
{
    g_ReportTimer = null;
    g_BanPollTimer = null;
    g_AccessSnapshotTimer = null;
    g_StatusReportTimer = null;

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
    if (g_ServerPorts != null)
    {
        delete g_ServerPorts;
        g_ServerPorts = null;
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
}

void StartAccessSnapshotTimer()
{
    StopAccessSnapshotTimer();

    float interval = g_AccessSnapshotInterval.FloatValue;
    g_AccessSnapshotTimer = CreateTimer(interval, Timer_RefreshAccessSnapshot, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopAccessSnapshotTimer()
{
    g_AccessSnapshotTimer = null;
}

public Action Timer_RefreshAccessSnapshot(Handle timer)
{
    if (g_AccessSnapshotTimer == timer)
    {
        g_AccessSnapshotTimer = null;
    }
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
    JSONObject payload = new JSONObject();
    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        delete payload;
        return;
    }

    if (!ResolvePluginApiConfig(url, sizeof(url), "/access/snapshot", token, sizeof(token)))
    {
        delete payload;
        return;
    }

    payload.SetString("report_token", token);
    payload.SetInt("port", currentPort);

    HTTPRequest request = new HTTPRequest(url);
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
        return;
    }

    SaveAccessSnapshot(item);
}

void SaveAccessSnapshot(JSONObject item)
{
    if (g_AccessSnapshotDb == null)
    {
        return;
    }

    SQL_FastQuery(g_AccessSnapshotDb, "BEGIN IMMEDIATE TRANSACTION");
    SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM metadata");
    SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM server_rules");
    SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM bans");
    SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM whitelist");
    SQL_FastQuery(g_AccessSnapshotDb, "DELETE FROM access_profiles");

    char version[128];
    char generatedAt[64];
    char expiresAt[64];
    item.GetString("version", version, sizeof(version));
    item.GetString("generated_at", generatedAt, sizeof(generatedAt));
    item.GetString("expires_at", expiresAt, sizeof(expiresAt));

    InsertMetadata("version", version);
    InsertMetadata("generated_at", generatedAt);
    InsertMetadata("expires_at", expiresAt);

    char generatedAtUnix[32];
    char expiresAtUnix[32];
    IntToString(item.GetInt("generated_at_unix"), generatedAtUnix, sizeof(generatedAtUnix));
    IntToString(item.GetInt("expires_at_unix"), expiresAtUnix, sizeof(expiresAtUnix));
    InsertMetadata("generated_at_unix", generatedAtUnix);
    InsertMetadata("expires_at_unix", expiresAtUnix);

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
        SQL_FastQuery(g_AccessSnapshotDb, query);
    }

    JSONArray bans = view_as<JSONArray>(item.Get("bans"));
    SaveSnapshotBans(bans);

    JSONArray whitelist = view_as<JSONArray>(item.Get("whitelist"));
    SaveSnapshotWhitelist(whitelist);

    JSONArray profiles = view_as<JSONArray>(item.Get("access_profiles"));
    SaveSnapshotAccessProfiles(profiles);

    SQL_FastQuery(g_AccessSnapshotDb, "COMMIT");
    LogMessage("Manger access snapshot refreshed: %s", version);
}

void InsertMetadata(const char[] key, const char[] value)
{
    char escapedKey[128];
    char escapedValue[256];
    char query[512];
    SQL_EscapeString(g_AccessSnapshotDb, key, escapedKey, sizeof(escapedKey));
    SQL_EscapeString(g_AccessSnapshotDb, value, escapedValue, sizeof(escapedValue));
    Format(query, sizeof(query), "INSERT INTO metadata (key, value) VALUES ('%s', '%s')", escapedKey, escapedValue);
    SQL_FastQuery(g_AccessSnapshotDb, query);
}

void SaveSnapshotBans(JSONArray bans)
{
    if (bans == null)
    {
        return;
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
        SQL_FastQuery(g_AccessSnapshotDb, query);
    }
}

void SaveSnapshotWhitelist(JSONArray whitelist)
{
    if (whitelist == null)
    {
        return;
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
        SQL_FastQuery(g_AccessSnapshotDb, query);
    }
}

void SaveSnapshotAccessProfiles(JSONArray profiles)
{
    if (profiles == null)
    {
        return;
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
        SQL_FastQuery(g_AccessSnapshotDb, query);
    }
}

void StartReportTimer()
{
    StopReportTimer();

    float interval = g_ReportInterval.FloatValue;
    g_ReportTimer = CreateTimer(interval, Timer_ReportOnlinePlayers, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopReportTimer()
{
    g_ReportTimer = null;
}

void StartBanPollTimer()
{
    StopBanPollTimer();

    float interval = g_ReportInterval.FloatValue;
    g_BanPollTimer = CreateTimer(interval, Timer_PollBans, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopBanPollTimer()
{
    g_BanPollTimer = null;
}

public Action Timer_PollBans(Handle timer)
{
    if (g_BanPollTimer == timer)
    {
        g_BanPollTimer = null;
    }

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

    HTTPRequest request = new HTTPRequest(url);
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
    JSON rawItems = data.Get("items");
    JSONArray items = view_as<JSONArray>(rawItems);

    for (int i = 0; i < items.Length; i++)
    {
        JSON rawItem = items.Get(i);
        JSONObject item = view_as<JSONObject>(rawItem);
        KickMatchingBan(item);
        delete item;
    }

    delete items;
    delete data;
}

void KickMatchingBan(JSONObject item)
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

    for (int client = 1; client <= MaxClients; client++)
    {
        if (!IsClientInGame(client) || IsFakeClient(client))
        {
            continue;
        }

        char clientSteamId64[64];
        char clientIp[64];
        GetClientAuthId(client, AuthId_SteamID64, clientSteamId64, sizeof(clientSteamId64), true);
        GetClientIP(client, clientIp, sizeof(clientIp), true);

        if ((steamId[0] != '\0' && StrEqual(clientSteamId64, steamId)) || (ipAddress[0] != '\0' && StrEqual(clientIp, ipAddress)))
        {
            CompletePolledBanDetails(client);
            KickClient(client, "你已被封禁：%s", reason);
        }
    }
}

void CompletePolledBanDetails(int client)
{
    char token[256];
    char url[512];
    char steamId64[64];
    char ipAddress[64];
    char playerName[128];
    char payload[MAX_BAN_PAYLOAD];
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
    BuildPluginBanCheckPayload(payload, sizeof(payload), token, currentPort, steamId64, ipAddress, playerName);
    PostJsonPayload(url, payload, OnBanCheckResponse);
}

bool GetCurrentReportToken(char[] token, int maxLen)
{
    token[0] = '\0';

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
    if (g_ReportTimer == timer)
    {
        g_ReportTimer = null;
    }

    char reportUrl[512];
    char reportToken[256];
    if (!ResolvePluginApiConfig(reportUrl, sizeof(reportUrl), "/online-players/report", reportToken, sizeof(reportToken)))
    {
        return Plugin_Continue;
    }

    char payload[MAX_REPORT_PAYLOAD];
    BuildReportPayload(payload, sizeof(payload), reportToken);
    PostJsonPayload(reportUrl, payload, OnReportResponse);

    return Plugin_Continue;
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

bool SubmitPluginBan(int client, int target, const char[] banType, const char[] steamId, const char[] ipAddress, const char[] player, int duration, const char[] reason)
{
    char token[256];
    char banUrl[512];
    char adminName[128];
    char adminSteamid[64];

    GetClientName(client, adminName, sizeof(adminName));
    GetClientAuthId(client, AuthId_SteamID64, adminSteamid, sizeof(adminSteamid), true);

    int currentPort = 0;
    if (!GetCurrentServerPort(currentPort))
    {
        return false;
    }

    // 先尝试在线提交
    if (ResolvePluginApiConfig(banUrl, sizeof(banUrl), "/bans", token, sizeof(token)))
    {
        char payload[MAX_BAN_PAYLOAD];
        BuildPluginBanPayload(payload, sizeof(payload), token, currentPort, banType, steamId, ipAddress, player, duration, reason, adminName);
        PostJsonPayload(banUrl, payload, OnPluginBanResponse);

        ReplyToCommand(client, "[Manger] 封禁已提交到网站。");
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
        strcopy(targetId, sizeof(targetId), steamId);
    }
    else
    {
        strcopy(targetId, sizeof(targetId), ipAddress);
    }

    EdgeSync_EnqueueOperation("ban", targetId, banType, player, reason, adminName, adminSteamid, duration);

    if (target > 0 && IsClientInGame(target))
    {
        KickClient(target, "你已被封禁：%s", reason);
    }
    return true;
}

int FindClientBySteamId2(const char[] steamId2)
{
    for (int i = 1; i <= MaxClients; i++)
    {
        if (!IsClientInGame(i) || IsFakeClient(i))
        {
            continue;
        }

        char clientSteamId2[64];
        if (GetClientAuthId(i, AuthId_Steam2, clientSteamId2, sizeof(clientSteamId2), true))
        {
            if (StrEqual(clientSteamId2, steamId2, false))
            {
                return i;
            }
        }
    }
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

    // SteamID2 格式 (STEAM_X:Y:Z)：搜索在线玩家或直接提交封禁
    if (StrContains(targetArg, "STEAM_") == 0)
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
            SubmitPluginBan(client, target, "steam", steamId64, ipAddress, player, duration, reason);
        }
        else
        {
            SubmitPluginBan(client, 0, "steam", targetArg, "", "", duration, reason);
            ReplyToCommand(client, "[Manger] 玩家不在线，封禁记录已提交到网站。");
        }
        return Plugin_Handled;
    }

    // RCON 通过 SteamID64 执行，网站已创建封禁记录
    bool isSteamId64 = (strlen(targetArg) == 17 && targetArg[0] == '7');
    if (client == 0 && isSteamId64)
    {
        ReplyToCommand(client, "[Manger] 封禁记录已创建，玩家将在下次轮询时被踢出。");
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
    char steamId[64];
    GetCmdArg(1, timeArg, sizeof(timeArg));
    GetCmdArg(2, steamId, sizeof(steamId));

    int duration = StringToInt(timeArg);
    if (duration < 0)
    {
        ReplyToCommand(client, "[Manger] 封禁时长不能为负数。");
        return Plugin_Handled;
    }

    char reason[256];
    AppendCommandReason(3, args, reason, sizeof(reason));
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
    char payload[MAX_BAN_PAYLOAD];
    GetCmdArg(1, target, sizeof(target));
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
        BuildPluginUnbanPayload(payload, sizeof(payload), token, currentPort, target, reason, adminName, adminSteamid);
        PostJsonPayload(url, payload, OnPluginUnbanResponse, g_UnbanAdminUserId);
        ReplyToCommand(client, "[Manger] 解封请求已发送到网站。");
    }
    else
    {
        ReplyToCommand(client, "[Manger] API未配置，使用离线队列...");
    }

    // 同时写入离线队列作为备份
    char targetType[16];
    if (strlen(target) == 17 && target[0] == '7')
    {
        strcopy(targetType, sizeof(targetType), "steam");
    }
    else
    {
        strcopy(targetType, sizeof(targetType), "ip");
    }

    EdgeSync_EnqueueOperation("unban", target, targetType, "", reason, adminName, adminSteamid, 0);

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
    if (IsFakeClient(client))
    {
        return;
    }

    // 准入接口已包含封禁检查，无需单独调用封禁校验
    SubmitAccessCheck(client);
}

void SubmitAccessCheck(int client)
{
    char token[256];
    char url[512];
    char steamId64[64];
    char ipAddress[64];
    char playerName[128];
    char payload[MAX_BAN_PAYLOAD];
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
    BuildPluginAccessCheckPayload(payload, sizeof(payload), token, currentPort, steamId64, ipAddress, playerName);
    int userId = GetClientUserId(client);
    PostJsonPayload(url, payload, OnAccessCheckResponse, userId);
}

bool PostJsonPayload(const char[] url, const char[] payloadText, HTTPRequestCallback callback, any value = 0)
{
    JSONObject payload = JSONObject.FromString(payloadText);
    if (payload == null)
    {
        LogError("Manger failed to parse JSON payload for %s.", url);
        return false;
    }

    HTTPRequest request = new HTTPRequest(url);
    request.Post(payload, callback, value);
    delete payload;
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
            PrintToChat(client, "[Manger] 解封成功。");
        }
        return;
    }

    // HTTP 错误
    if (response.Status == HTTPStatus_BadRequest)
    {
        // 解析错误消息
        JSONObject data = view_as<JSONObject>(response.Data);
        if (data != null)
        {
            char message[256];
            if (data.GetString("message", message, sizeof(message)))
            {
                if (client > 0)
                {
                    PrintToChat(client, "[Manger] %s", message);
                }
                LogError("Manger unban failed: %s", message);
            }
            delete data;
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
    JSONObject result = view_as<JSONObject>(data.Get("result"));
    bool allowed = result.GetBool("allowed");
    if (!allowed)
    {
        char message[256];
        result.GetString("message", message, sizeof(message));
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
        KickClient(client, "你已被封禁：%s", reason);
        return;
    }

    if (!OfflineRulesAllowClient(steamId))
    {
        KickClient(client, "你的白名单状态无法确认，请稍后再试。");
        return;
    }
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

void BuildReportPayload(char[] payload, int maxLen, const char[] reportToken)
{
    int currentPort = 0;
    GetCurrentServerPort(currentPort);

    char currentMap[64];
    GetCurrentMap(currentMap, sizeof(currentMap));

    char escapedMap[128];
    EscapeJsonString(currentMap, escapedMap, sizeof(escapedMap));

    Format(payload, maxLen, "{\"port\":%d,\"report_token\":\"%s\",\"current_map\":\"%s\",\"players\":[", currentPort, reportToken, escapedMap);

    bool first = true;
    for (int client = 1; client <= MaxClients; client++)
    {
        if (!IsClientInGame(client) || IsFakeClient(client))
        {
            continue;
        }

        char player[128];
        char steamId64[64];
        char playerIp[64];
        char escapedPlayer[256];
        char escapedIp[128];
        GetClientName(client, player, sizeof(player));
        GetClientAuthId(client, AuthId_SteamID64, steamId64, sizeof(steamId64), true);
        GetClientIP(client, playerIp, sizeof(playerIp), true);
        EscapeJsonString(player, escapedPlayer, sizeof(escapedPlayer));
        EscapeJsonString(playerIp, escapedIp, sizeof(escapedIp));

        char item[512];
        int ping = RoundToNearest(GetClientAvgLatency(client, NetFlow_Both) * 1000.0);
        Format(item, sizeof(item), "%s{\"name\":\"%s\",\"steam_id64\":\"%s\",\"ip\":\"%s\",\"ping\":%d,\"server_port\":%d}", first ? "" : ",", escapedPlayer, steamId64, escapedIp, ping, currentPort);
        StrCat(payload, maxLen, item);
        first = false;
    }

    StrCat(payload, maxLen, "]}");
}

void BuildPluginBanPayload(char[] payload, int maxLen, const char[] token, int port, const char[] banType, const char[] steamId, const char[] ipAddress, const char[] player, int duration, const char[] reason, const char[] operatorName)
{
    char escapedPlayer[256];
    char escapedReason[512];
    char escapedOperator[128];
    EscapeJsonString(player, escapedPlayer, sizeof(escapedPlayer));
    EscapeJsonString(reason, escapedReason, sizeof(escapedReason));
    EscapeJsonString(operatorName, escapedOperator, sizeof(escapedOperator));

    Format(payload, maxLen, "{\"report_token\":\"%s\",\"port\":%d,\"ban_type\":\"%s\",\"steam_id\":\"%s\",\"ip_address\":\"%s\",\"player\":\"%s\",\"duration_minutes\":%d,\"reason\":\"%s\",\"operator_name\":\"%s\"}", token, port, banType, steamId, ipAddress, escapedPlayer, duration, escapedReason, escapedOperator);
}

void BuildPluginUnbanPayload(char[] payload, int maxLen, const char[] token, int port, const char[] target, const char[] reason, const char[] operatorName, const char[] operatorSteamid)
{
    char escapedReason[512];
    char escapedOperator[128];
    char escapedSteamid[64];
    EscapeJsonString(reason, escapedReason, sizeof(escapedReason));
    EscapeJsonString(operatorName, escapedOperator, sizeof(escapedOperator));
    EscapeJsonString(operatorSteamid, escapedSteamid, sizeof(escapedSteamid));

    if (operatorSteamid[0] != '\0')
    {
        Format(payload, maxLen, "{\"report_token\":\"%s\",\"port\":%d,\"target\":\"%s\",\"reason\":\"%s\",\"operator_name\":\"%s\",\"operator_steamid\":\"%s\"}", token, port, target, escapedReason, escapedOperator, escapedSteamid);
    }
    else
    {
        Format(payload, maxLen, "{\"report_token\":\"%s\",\"port\":%d,\"target\":\"%s\",\"reason\":\"%s\",\"operator_name\":\"%s\"}", token, port, target, escapedReason, escapedOperator);
    }
}

void BuildPluginBanCheckPayload(char[] payload, int maxLen, const char[] token, int port, const char[] steamId, const char[] ipAddress, const char[] player)
{
    char escapedPlayer[256];
    char escapedIp[128];
    EscapeJsonString(player, escapedPlayer, sizeof(escapedPlayer));
    EscapeJsonString(ipAddress, escapedIp, sizeof(escapedIp));
    Format(payload, maxLen, "{\"report_token\":\"%s\",\"port\":%d,\"steam_id\":\"%s\",\"ip_address\":\"%s\",\"player\":\"%s\",\"server_port\":%d}", token, port, steamId, escapedIp, escapedPlayer, port);
}

void BuildPluginAccessCheckPayload(char[] payload, int maxLen, const char[] token, int port, const char[] steamId64, const char[] ipAddress, const char[] player)
{
    char escapedPlayer[256];
    char escapedIp[128];
    EscapeJsonString(player, escapedPlayer, sizeof(escapedPlayer));
    EscapeJsonString(ipAddress, escapedIp, sizeof(escapedIp));
    Format(payload, maxLen, "{\"report_token\":\"%s\",\"port\":%d,\"steam_id64\":\"%s\",\"ip_address\":\"%s\",\"player\":\"%s\",\"server_port\":%d}", token, port, steamId64, escapedIp, escapedPlayer, port);
}

void EscapeJsonString(const char[] input, char[] output, int maxLen)
{
    output[0] = '\0';

    for (int i = 0; input[i] != '\0'; i++)
    {
        char chunk[3];
        if (input[i] == '"' || input[i] == '\\')
        {
            Format(chunk, sizeof(chunk), "\\%c", input[i]);
        }
        else
        {
            Format(chunk, sizeof(chunk), "%c", input[i]);
        }

        StrCat(output, maxLen, chunk);
    }
}

void StartStatusReportTimer()
{
    StopStatusReportTimer();

    float interval = g_StatusReportInterval.FloatValue;
    g_StatusReportTimer = CreateTimer(interval, Timer_ReportServerStatus, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopStatusReportTimer()
{
    g_StatusReportTimer = null;
}

public Action Timer_ReportServerStatus(Handle timer)
{
    if (g_StatusReportTimer == timer)
    {
        g_StatusReportTimer = null;
    }

    CollectServerStats();
    ReportServerStatus();
    return Plugin_Continue;
}

void CollectServerStats()
{
    g_LastTickrate = GetTickrate();
    int playersCount = 0;
    for (int client = 1; client <= MaxClients; client++)
    {
        if (IsClientInGame(client) && !IsFakeClient(client))
        {
            playersCount++;
        }
    }
}

void ReportServerStatus()
{
    char token[256];
    char url[512];
    if (!ResolvePluginApiConfig(url, sizeof(url), "/server-status", token, sizeof(token)))
    {
        return;
    }

    char payload[MAX_STATUS_PAYLOAD];
    BuildServerStatusPayload(payload, sizeof(payload), token);
    PostJsonPayload(url, payload, OnServerStatusResponse);
}

void BuildServerStatusPayload(char[] payload, int maxLen, const char[] token)
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
    char escapedMap[256];
    EscapeJsonString(currentMap, escapedMap, sizeof(escapedMap));

    Format(payload, maxLen,
        "{\"report_token\":\"%s\",\"port\":%d,\"fps\":%.2f,\"cpu_usage\":%.2f,\"tickrate\":%.2f,\"uptime_seconds\":%d,\"players_count\":%d,\"max_players\":%d,\"current_map\":\"%s\"}",
        token, currentPort, fps, cpu, tickrate, uptime, playersCount, maxPlayers, escapedMap);
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
    return 0.0;
}

int GetTickrate()
{
    ConVar tickrateCvar = FindConVar("sv_maxrate");
    if (tickrateCvar != null)
    {
        int maxRate = tickrateCvar.IntValue;
        if (maxRate > 0)
        {
            return maxRate / 1000;
        }
    }
    return 64;
}
