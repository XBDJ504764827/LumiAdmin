/**
 * Edge Sync Agent - 离线操作队列和同步引擎
 *
 * 功能：
 * 1. 离线操作队列 - 所有操作先写入本地SQLite，再尝试同步
 * 2. 断线检测 - HTTP请求失败时自动切换到离线模式
 * 3. 重连同步 - 定时检查队列，自动重试pending操作
 * 4. 本地审计 - 记录所有操作到audit_log表
 */
#include <sourcemod>
#include <adt_array>
#include <ripext>
#include <manger_shared>
#include <cngokz/core>

native int MangerReporter_GetApiBaseUrl(char[] apiBaseUrl, int maxLen);
native int MangerReporter_GetReportToken(char[] token, int maxLen);
native int MangerReporter_GetServerPort();

#define DEFAULT_API_BASE_URL "http://127.0.0.1:8080/api/plugin"
#define EDGE_SYNC_DB "cngokz_sync"
#define MAX_IDEMPOTENCY_KEY 64
#define MAX_SERVER_TOKEN 256
#define MAX_RETRY_COUNT 10
#define CLEANUP_RETENTION_SECONDS 604800

Database g_EdgeSyncDb = null;
Handle g_SyncTimer = null;
ConVar g_ApiBaseUrl;
ConVar g_SyncInterval;
ConVar g_HostPortCvar = null;
char g_ServerReportToken[MAX_SERVER_TOKEN];
int g_ServerPort = 0;
bool g_IsOnline = true;
bool g_SyncInFlight = false;
int g_PendingCount = 0;
int g_LastSyncTime = 0;
int g_OperationSeq = 0;
bool g_LegacyEdgeSyncSurfaceDeferred = false;

/**
 * 安全格式化整数到 SQL 字符串，负数输出 "0"。
 */
void SafeIntToString(int value, char[] buffer, int maxLen)
{
    if (value < 0) value = 0;
    IntToString(value, buffer, maxLen);
}


public Plugin myinfo =
{
    name = "CNGOKZ Sync",
    author = "XBDJ",
    description = "Offline operation queue and sync engine for LumiAdmin.",
    version = "1.1.0",
    url = ""
};

#include "cngokz-sync/legacy_disable.sp"

public void OnPluginStart()
{
    if (!DisableLegacyEdgeSyncBinary())
    {
        SetFailState("Failed to disable legacy manger_edge_sync.smx. Move it to plugins/disabled and reload cngokz-sync.");
    }

    if (g_LegacyEdgeSyncSurfaceDeferred)
    {
        ServerCommand("sm plugins reload cngokz-sync");
        return;
    }

    g_ApiBaseUrl = FindConVar("cngokz_api_base_url");
    if (g_ApiBaseUrl == null)
    {
        g_ApiBaseUrl = CreateConVar("cngokz_api_base_url", DEFAULT_API_BASE_URL, "CNGOKZ LumiAdmin plugin API base URL.");
    }
    g_SyncInterval = CreateConVar("cngokz_sync_interval", "30.0", "Offline queue sync interval in seconds.", _, true, 10.0);
    g_HostPortCvar = FindConVar("hostport");
    HookConVarChange(g_SyncInterval, OnSyncIntervalChanged);

    EnsureCNGOKZSyncConfigDirectory();
    AutoExecConfig(true, "cngokz-sync", "sourcemod/cngokz-lumiadmin");

    RegServerCmd("cngokz_sync_set_token", CommandSetToken, "Set server report token for CNGOKZ Sync fallback mode.");
    RegAdminCmd("sm_sync_status", CommandSyncStatus, ADMFLAG_RCON, "Show offline sync status");
    RegAdminCmd("sm_force_sync", CommandForceSync, ADMFLAG_RCON, "Force sync offline queue");

    ResolveEdgeSyncConfig();
    InitEdgeSyncDb();
    StartSyncTimer();
}

void EnsureCNGOKZSyncConfigDirectory()
{
    char dir[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, dir, sizeof(dir), "../../cfg/sourcemod/cngokz-lumiadmin");
    if (!DirExists(dir))
    {
        CreateDirectory(dir, 511);
    }
}

public void OnMapStart()
{
    if (g_EdgeSyncDb == null)
    {
        InitEdgeSyncDb();
    }
    ResolveEdgeSyncConfig();
    StartSyncTimer();
}

public void OnMapEnd()
{
    StopSyncTimer();
}

public void OnPluginEnd()
{
    StopSyncTimer();
}

public void OnSyncIntervalChanged(ConVar convar, const char[] oldValue, const char[] newValue)
{
    StartSyncTimer();
}

void StartSyncTimer()
{
    StopSyncTimer();
    float interval = g_SyncInterval.FloatValue;
    g_SyncTimer = CreateTimer(interval, Timer_SyncQueue, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopSyncTimer()
{
    if (g_SyncTimer != null)
    {
        delete g_SyncTimer;
        g_SyncTimer = null;
    }
}

public Action Timer_SyncQueue(Handle timer)
{
    SyncOfflineQueue();
    return Plugin_Continue;
}

bool GetCurrentServerPort(int &port)
{
    if (g_HostPortCvar == null)
    {
        return false;
    }
    port = g_HostPortCvar.IntValue;
    return port > 0;
}


void LoadEdgeSyncConfig()
{
    int currentPort = 0;
    GetCurrentServerPort(currentPort);

    char path[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, path, sizeof(path), "../../cfg/sourcemod/cngokz-lumiadmin/cngokz-core.cfg");

    File file = OpenFile(path, "r");
    if (file == null)
    {
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
        if (ParseConfigValueLine(line, "cngokz_api_base_url", value, sizeof(value)))
        {
            g_ApiBaseUrl.SetString(value);
            continue;
        }

        int port = 0;
        char token[MAX_SERVER_TOKEN];
        if (ParseCNGOKZServerMappingLine(line, port, token, sizeof(token)) && port == currentPort)
        {
            g_ServerPort = port;
            strcopy(g_ServerReportToken, sizeof(g_ServerReportToken), token);
        }
    }

    delete file;
}

bool LoadEdgeSyncConfigFromCore()
{
    if (!LibraryExists("cngokz-core"))
    {
        return false;
    }

    int port = CNGOKZCore_GetServerPort();
    if (port <= 0)
    {
        return false;
    }

    char apiBaseUrl[512];
    if (!CNGOKZCore_GetApiBaseUrl(apiBaseUrl, sizeof(apiBaseUrl)))
    {
        return false;
    }

    char token[MAX_SERVER_TOKEN];
    if (!CNGOKZCore_GetReportToken(token, sizeof(token)))
    {
        return false;
    }

    TrimString(apiBaseUrl);
    TrimString(token);
    if (apiBaseUrl[0] == '\0' || token[0] == '\0')
    {
        return false;
    }

    g_ApiBaseUrl.SetString(apiBaseUrl);
    g_ServerPort = port;
    strcopy(g_ServerReportToken, sizeof(g_ServerReportToken), token);
    return true;
}

void ResolveEdgeSyncConfig()
{
    if (LoadEdgeSyncConfigFromCore())
    {
        return;
    }
    LoadEdgeSyncConfig();
}

void InitEdgeSyncDb()
{
    char error[256];
    g_EdgeSyncDb = SQLite_UseDatabase(EDGE_SYNC_DB, error, sizeof(error));
    if (g_EdgeSyncDb == null)
    {
        LogError("[EdgeSync] SQLite open failed: %s", error);
        return;
    }

    // 离线操作队列表
    SQL_FastQuery(g_EdgeSyncDb, "CREATE TABLE IF NOT EXISTS offline_queue (id INTEGER PRIMARY KEY AUTOINCREMENT, operation TEXT NOT NULL, target TEXT NOT NULL, target_type TEXT NOT NULL, player_name TEXT, reason TEXT, duration_minutes INTEGER, operator_name TEXT NOT NULL, operator_steamid TEXT, server_port INTEGER NOT NULL, created_at INTEGER NOT NULL, status TEXT NOT NULL, synced_at INTEGER, sync_error TEXT, retry_count INTEGER DEFAULT 0, idempotency_key TEXT NOT NULL)");
    SQL_FastQuery(g_EdgeSyncDb, "CREATE UNIQUE INDEX IF NOT EXISTS idx_offline_queue_idempotency_key ON offline_queue(idempotency_key)");

    // 本地审计日志表
    SQL_FastQuery(g_EdgeSyncDb, "CREATE TABLE IF NOT EXISTS audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, operation TEXT NOT NULL, target TEXT NOT NULL, operator_name TEXT NOT NULL, operator_steamid TEXT, server_port INTEGER NOT NULL, success INTEGER NOT NULL, message TEXT, created_at INTEGER NOT NULL)");

    // 更新待同步计数
    UpdatePendingCount();

    // 清理过期历史数据（保留7天）
    CleanupStaleRecords();
}

void UpdatePendingCount()
{
    if (g_EdgeSyncDb == null) return;

    DBResultSet results = SQL_Query(g_EdgeSyncDb, "SELECT COUNT(*) FROM offline_queue WHERE status = 'pending' AND retry_count < %d", MAX_RETRY_COUNT);
    if (results != null)
    {
        if (SQL_FetchRow(results))
        {
            g_PendingCount = SQL_FetchInt(results, 0);
        }
        delete results;
    }
}

void CleanupStaleRecords()
{
    if (g_EdgeSyncDb == null) return;

    int cutoff = GetTime() - CLEANUP_RETENTION_SECONDS;

    char query[256];
    Format(query, sizeof(query), "DELETE FROM offline_queue WHERE status IN ('synced', 'failed') AND created_at < %d", cutoff);
    SQL_FastQuery(g_EdgeSyncDb, query);

    Format(query, sizeof(query), "DELETE FROM audit_log WHERE created_at < %d", cutoff);
    SQL_FastQuery(g_EdgeSyncDb, query);
}

public Action CommandSetToken(int args)
{
    if (args < 2) return Plugin_Handled;

    char portText[16];
    char token[256];
    GetCmdArg(1, portText, sizeof(portText));
    GetCmdArg(2, token, sizeof(token));

    g_ServerPort = StringToInt(portText);
    strcopy(g_ServerReportToken, sizeof(g_ServerReportToken), token);

    LogMessage("[EdgeSync] Configured for port %d", g_ServerPort);
    return Plugin_Handled;
}

bool EscapeSqlString(Database db, const char[] value, char[] escaped, int maxLen)
{
    escaped[0] = '\0';
    return SQL_EscapeString(db, value, escaped, maxLen);
}

bool ExecuteSql(Database db, const char[] query, const char[] context)
{
    if (SQL_FastQuery(db, query))
    {
        return true;
    }
    LogError("[EdgeSync] SQL failed in %s.", context);
    return false;
}

public Action CommandSyncStatus(int client, int args)
{
    char status[128];
    Format(status, sizeof(status), "[EdgeSync] Status: %s | Pending: %d | Last sync: %d seconds ago",
        g_IsOnline ? "Online" : "Offline",
        g_PendingCount,
        GetTime() - g_LastSyncTime);

    ReplyToCommand(client, status);
    return Plugin_Handled;
}

public Action CommandForceSync(int client, int args)
{
    ReplyToCommand(client, "[EdgeSync] Force syncing queue...");
    SyncOfflineQueue();
    return Plugin_Handled;
}

/**
 * 添加操作到离线队列
 * @param operation 操作类型: ban, unban, whitelist_add, whitelist_remove
 * @param target 目标 (SteamID64 或 IP)
 * @param targetType 目标类型: steam 或 ip
 * @param playerName 玩家名称（可选）
 * @param reason 原因（可选）
 * @param durationMinutes 封禁时长（分钟，0=永久）
 * @param operatorName 操作人名称
 * @param operatorSteamid 操作人SteamID（可选）
 * @return 操作ID
 */
int EnqueueOperation(
    const char[] operation,
    const char[] target,
    const char[] targetType,
    const char[] playerName,
    const char[] reason,
    int durationMinutes,
    const char[] operatorName,
    const char[] operatorSteamid
)
{
    if (g_EdgeSyncDb == null) return -1;

    // 生成幂等键
    char idempotencyKey[MAX_IDEMPOTENCY_KEY];
    Format(idempotencyKey, sizeof(idempotencyKey), "%d_%d_%s_%s_%d",
        GetTime(), ++g_OperationSeq, operation, target, g_ServerPort);

    // 校验 durationMinutes 为合法非负整数
    if (durationMinutes < 0)
    {
        LogError("[EdgeSync] EnqueueOperation rejected: negative duration_minutes %d.", durationMinutes);
        return -1;
    }

    // 校验 g_ServerPort 为合法正整数
    if (g_ServerPort <= 0)
    {
        LogError("[EdgeSync] EnqueueOperation rejected: invalid server_port %d.", g_ServerPort);
        return -1;
    }

    // 转义字符串
    char escapedTarget[256];
    char escapedPlayerName[256];
    char escapedReason[512];
    char escapedOperatorName[256];
    char escapedOperatorSteamid[256];
    char escapedIdempotencyKey[MAX_IDEMPOTENCY_KEY];

    EscapeSqlString(g_EdgeSyncDb, target, escapedTarget, sizeof(escapedTarget));
    EscapeSqlString(g_EdgeSyncDb, playerName, escapedPlayerName, sizeof(escapedPlayerName));
    EscapeSqlString(g_EdgeSyncDb, reason, escapedReason, sizeof(escapedReason));
    EscapeSqlString(g_EdgeSyncDb, operatorName, escapedOperatorName, sizeof(escapedOperatorName));
    EscapeSqlString(g_EdgeSyncDb, operatorSteamid, escapedOperatorSteamid, sizeof(escapedOperatorSteamid));
    EscapeSqlString(g_EdgeSyncDb, idempotencyKey, escapedIdempotencyKey, sizeof(escapedIdempotencyKey));

    char query[2048];
    char portBuf[16];
    char timeBuf[16];
    SafeIntToString(g_ServerPort, portBuf, sizeof(portBuf));
    SafeIntToString(GetTime(), timeBuf, sizeof(timeBuf));

    Format(query, sizeof(query), "INSERT INTO offline_queue (operation, target, target_type, player_name, reason, duration_minutes, operator_name, operator_steamid, server_port, created_at, status, idempotency_key) VALUES ('%s', '%s', '%s', '%s', '%s', %d, '%s', '%s', %s, %s, 'pending', '%s')", operation, escapedTarget, targetType, escapedPlayerName, escapedReason, durationMinutes, escapedOperatorName, escapedOperatorSteamid, portBuf, timeBuf, escapedIdempotencyKey);

    if (!ExecuteSql(g_EdgeSyncDb, query, "enqueue operation"))
    {
        return -1;
    }

    // 获取插入的ID
    int opId = 0;
    DBResultSet results = SQL_Query(g_EdgeSyncDb, "SELECT last_insert_rowid()");
    if (results != null)
    {
        if (SQL_FetchRow(results))
        {
            opId = SQL_FetchInt(results, 0);
        }
        delete results;
    }

    // 写入本地审计日志
    WriteLocalAuditLog(operation, target, operatorName, operatorSteamid, true, "Enqueued for offline sync");

    // 更新计数
    g_PendingCount++;

    // 立即尝试同步
    SyncOfflineQueue();

    return opId;
}

void WriteLocalAuditLog(
    const char[] operation,
    const char[] target,
    const char[] operatorName,
    const char[] operatorSteamid,
    bool success,
    const char[] message
)
{
    if (g_EdgeSyncDb == null) return;

    char escapedTarget[256];
    char escapedOperatorName[256];
    char escapedOperatorSteamid[256];
    char escapedMessage[512];

    EscapeSqlString(g_EdgeSyncDb, target, escapedTarget, sizeof(escapedTarget));
    EscapeSqlString(g_EdgeSyncDb, operatorName, escapedOperatorName, sizeof(escapedOperatorName));
    EscapeSqlString(g_EdgeSyncDb, operatorSteamid, escapedOperatorSteamid, sizeof(escapedOperatorSteamid));
    EscapeSqlString(g_EdgeSyncDb, message, escapedMessage, sizeof(escapedMessage));

    char query[1024];
    char portBuf[16];
    char timeBuf[16];
    SafeIntToString(g_ServerPort, portBuf, sizeof(portBuf));
    SafeIntToString(GetTime(), timeBuf, sizeof(timeBuf));
    char successBuf[8];
    SafeIntToString(success ? 1 : 0, successBuf, sizeof(successBuf));

    Format(query, sizeof(query), "INSERT INTO audit_log (operation, target, operator_name, operator_steamid, server_port, success, message, created_at) VALUES ('%s', '%s', '%s', '%s', %s, %s, '%s', %s)", operation, escapedTarget, escapedOperatorName, escapedOperatorSteamid, portBuf, successBuf, escapedMessage, timeBuf);

    ExecuteSql(g_EdgeSyncDb, query, "local audit log");
}

void SyncOfflineQueue()
{
    if (g_EdgeSyncDb == null) return;
    if (g_SyncInFlight) return;
    if (g_ServerPort <= 0 || g_ServerReportToken[0] == '\0')
    {
        ResolveEdgeSyncConfig();
    }
    if (g_ServerPort <= 0 || g_ServerReportToken[0] == '\0') return;

    // 查询待同步的操作
    DBResultSet results = SQL_Query(g_EdgeSyncDb, "SELECT id, operation, target, target_type, player_name, reason, duration_minutes, operator_name, operator_steamid, created_at, idempotency_key FROM offline_queue WHERE status = 'pending' AND retry_count < %d ORDER BY created_at ASC LIMIT 50", MAX_RETRY_COUNT);

    if (results == null) return;

    ArrayList ids = new ArrayList();
    JSONObject jsonPayload = new JSONObject();
    jsonPayload.SetString("report_token", g_ServerReportToken);
    jsonPayload.SetInt("port", g_ServerPort);
    JSONArray opsArray = new JSONArray();

    while (SQL_FetchRow(results))
    {
        int id = SQL_FetchInt(results, 0);
        ids.Push(id);

        char operation[32];
        char target[64];
        char targetType[16];
        char playerName[128];
        char reason[256];
        char operatorName[128];
        char operatorSteamid[64];
        char idempotencyKey[MAX_IDEMPOTENCY_KEY];
        int durationMinutes;
        int createdAt;

        SQL_FetchString(results, 1, operation, sizeof(operation));
        SQL_FetchString(results, 2, target, sizeof(target));
        SQL_FetchString(results, 3, targetType, sizeof(targetType));
        SQL_FetchString(results, 4, playerName, sizeof(playerName));
        SQL_FetchString(results, 5, reason, sizeof(reason));
        durationMinutes = SQL_FetchInt(results, 6);
        SQL_FetchString(results, 7, operatorName, sizeof(operatorName));
        SQL_FetchString(results, 8, operatorSteamid, sizeof(operatorSteamid));
        createdAt = SQL_FetchInt(results, 9);
        SQL_FetchString(results, 10, idempotencyKey, sizeof(idempotencyKey));

        // 构建 JSON 对象（使用 JSON API 避免注入）
        JSONObject entry = new JSONObject();
        entry.SetString("operation", operation);
        entry.SetString("target", target);
        entry.SetString("target_type", targetType);
        entry.SetString("player_name", playerName);
        entry.SetString("reason", reason);
        entry.SetInt("duration_minutes", durationMinutes);
        entry.SetString("operator_name", operatorName);
        entry.SetString("operator_steamid", operatorSteamid);
        entry.SetInt("created_at_unix", createdAt);
        entry.SetString("idempotency_key", idempotencyKey);

        opsArray.Push(entry);
        delete entry;
    }
    delete results;

    if (opsArray.Length == 0)
    {
        delete opsArray;
        delete jsonPayload;
        delete ids;
        return;
    }

    jsonPayload.Set("operations", opsArray);
    delete opsArray;

    // 发送同步请求
    char url[512];
    g_ApiBaseUrl.GetString(url, sizeof(url));
    TrimString(url);
    if (url[0] == '\0')
    {
        delete jsonPayload;
        delete ids;
        return;
    }
    int len = strlen(url);
    if (len > 0 && url[len - 1] == '/')
    {
        url[len - 1] = '\0';
    }
    StrCat(url, sizeof(url), "/offline/sync");

    HTTPRequest request = new HTTPRequest(url);
    request.Timeout = 10;
    g_SyncInFlight = true;
    request.Post(jsonPayload, OnSyncResponse, ids);
    delete jsonPayload;
}

public void OnSyncResponse(HTTPResponse response, any value, const char[] error)
{
    ArrayList ids = view_as<ArrayList>(value);

    if (error[0] != '\0')
    {
        LogError("[EdgeSync] Sync failed: %s", error);
        g_IsOnline = false;
        g_SyncInFlight = false;
        MarkOperationsRetryable(ids, error);
        delete ids;
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("[EdgeSync] Sync returned HTTP %d", response.Status);
        g_IsOnline = false;
        char errorMsg[64];
        Format(errorMsg, sizeof(errorMsg), "HTTP %d", response.Status);
        g_SyncInFlight = false;
        if (response.Status >= HTTPStatus_BadRequest && response.Status < HTTPStatus_InternalServerError)
        {
            MarkOperationsFailed(ids, errorMsg);
        }
        else
        {
            MarkOperationsRetryable(ids, errorMsg);
        }
        delete ids;
        return;
    }

    g_IsOnline = true;
    g_SyncInFlight = false;
    g_LastSyncTime = GetTime();

    // 解析响应
    JSONObject data = view_as<JSONObject>(response.Data);
    if (data == null)
    {
        delete ids;
        return;
    }

    int applied = data.GetInt("applied");
    int skipped = data.GetInt("skipped");

    LogMessage("[EdgeSync] Sync complete: applied=%d, skipped=%d", applied, skipped);

    // 标记所有操作为已同步
    for (int i = 0; i < ids.Length; i++)
    {
        int id = ids.Get(i);
        MarkOperationSynced(id);
    }

    delete data;
    delete ids;

    UpdatePendingCount();
}

void MarkOperationSynced(int id)
{
    if (g_EdgeSyncDb == null) return;
    if (id < 0) return;

    char query[256];
    char timeBuf[16];
    char idBuf[16];
    SafeIntToString(GetTime(), timeBuf, sizeof(timeBuf));
    SafeIntToString(id, idBuf, sizeof(idBuf));
    Format(query, sizeof(query), "UPDATE offline_queue SET status = 'synced', synced_at = %s WHERE id = %s", timeBuf, idBuf);
    ExecuteSql(g_EdgeSyncDb, query, "mark operation synced");
}

void MarkOperationsFailed(ArrayList ids, const char[] error)
{
    if (g_EdgeSyncDb == null) return;

    char escapedError[256];
    EscapeSqlString(g_EdgeSyncDb, error, escapedError, sizeof(escapedError));

    for (int i = 0; i < ids.Length; i++)
    {
        int id = ids.Get(i);
        if (id < 0) continue;

        char query[512];
        char idBuf[16];
        SafeIntToString(id, idBuf, sizeof(idBuf));
        Format(query, sizeof(query), "UPDATE offline_queue SET status = 'failed', sync_error = '%s' WHERE id = %s", escapedError, idBuf);
        ExecuteSql(g_EdgeSyncDb, query, "mark operations failed");
    }

    UpdatePendingCount();
}

void MarkOperationsRetryable(ArrayList ids, const char[] error)
{
    if (g_EdgeSyncDb == null) return;

    char escapedError[256];
    EscapeSqlString(g_EdgeSyncDb, error, escapedError, sizeof(escapedError));

    for (int i = 0; i < ids.Length; i++)
    {
        int id = ids.Get(i);
        if (id < 0) continue;

        char query[512];
        char idBuf[16];
        SafeIntToString(id, idBuf, sizeof(idBuf));
        Format(query, sizeof(query), "UPDATE offline_queue SET status = 'pending', sync_error = '%s', retry_count = retry_count + 1 WHERE id = %s", escapedError, idBuf);
        ExecuteSql(g_EdgeSyncDb, query, "mark operations retryable");
    }

    UpdatePendingCount();
}

// 导出函数供其他插件调用
public int Native_EnqueueOperation(Handle plugin, int numParams)
{
    char operation[32];
    char target[64];
    char targetType[16];
    char playerName[128];
    char reason[256];
    char operatorName[128];
    char operatorSteamid[64];

    GetNativeString(1, operation, sizeof(operation));
    GetNativeString(2, target, sizeof(target));
    GetNativeString(3, targetType, sizeof(targetType));
    GetNativeString(4, playerName, sizeof(playerName));
    GetNativeString(5, reason, sizeof(reason));
    GetNativeString(6, operatorName, sizeof(operatorName));
    GetNativeString(7, operatorSteamid, sizeof(operatorSteamid));
    int durationMinutes = GetNativeCell(8);

    return EnqueueOperation(operation, target, targetType, playerName, reason, durationMinutes, operatorName, operatorSteamid);
}

public int Native_IsOnline(Handle plugin, int numParams)
{
    return g_IsOnline ? 1 : 0;
}

public int Native_GetPendingCount(Handle plugin, int numParams)
{
    return g_PendingCount;
}

public APLRes AskPluginLoad2(Handle myself, bool late, char[] error, int err_max)
{
    MarkNativeAsOptional("MangerReporter_GetApiBaseUrl");
    MarkNativeAsOptional("MangerReporter_GetReportToken");
    MarkNativeAsOptional("MangerReporter_GetServerPort");
    MarkNativeAsOptional("CNGOKZCore_GetApiBaseUrl");
    MarkNativeAsOptional("CNGOKZCore_GetReportToken");
    MarkNativeAsOptional("CNGOKZCore_GetServerPort");
    if (IsLegacyEdgeSyncSurfaceOccupied())
    {
        g_LegacyEdgeSyncSurfaceDeferred = true;
    }
    else
    {
        CreateNative("EdgeSync_EnqueueOperation", Native_EnqueueOperation);
        CreateNative("EdgeSync_IsOnline", Native_IsOnline);
        CreateNative("EdgeSync_GetPendingCount", Native_GetPendingCount);
        RegPluginLibrary("cngokz-sync");
    }
    return APLRes_Success;
}
