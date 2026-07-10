char g_CurrentMapName[128];

void RecordGuard_OnPluginStart()
{
    g_RGEnabled = CreateConVar("cngokz_recordguard_enabled", "1", "Enable abnormal record guard.", _, true, 0.0, true, 1.0);
    g_RGRuleSyncInterval = CreateConVar("cngokz_recordguard_rule_sync_interval", "60.0", "Rule sync interval in seconds.", _, true, 15.0);
    g_RGPollInterval = CreateConVar("cngokz_recordguard_poll_interval", "10.0", "Approved abnormal record poll interval in seconds.", _, true, 5.0);
    g_RGRequestTimeout = CreateConVar("cngokz_recordguard_request_timeout", "15", "HTTP request timeout in seconds.", _, true, 3.0, true, 60.0);
    g_RGDebugLog = CreateConVar("cngokz_recordguard_debug", "0", "Enable verbose recordguard debug logs.", _, true, 0.0, true, 1.0);
    g_RGR2UploadEnabled = CreateConVar("cngokz_recordguard_r2upload_enabled", "1", "Upload abnormal replays directly to R2 from the game server.", _, true, 0.0, true, 1.0);
    g_RGTickrate = FindConVar("sv_maxupdaterate");

    RecordGuard_EnsureConfigDirectory();
    AutoExecConfig(true, "cngokz-recordguard", "sourcemod/cngokz-lumiadmin");
    RecordGuard_InitR2Upload();
    GetCurrentMapDisplayName(g_CurrentMapName, sizeof(g_CurrentMapName));
    InitRecordGuardDb();
    EnsureReplayCacheDir();
    SyncRules();
    StartRuleSyncTimer();
    StartApprovedPollTimer();
    StartR2RetryTimer();
}

void RecordGuard_EnsureConfigDirectory()
{
    char dir[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, dir, sizeof(dir), "../../cfg/sourcemod/cngokz-lumiadmin");
    if (!DirExists(dir))
    {
        CreateDirectory(dir, 511);
    }
}

void StartRuleSyncTimer()
{
    StopRuleSyncTimer();
    g_RGRuleTimer = CreateTimer(g_RGRuleSyncInterval.FloatValue, Timer_SyncRules, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopRuleSyncTimer()
{
    if (g_RGRuleTimer != null)
    {
        delete g_RGRuleTimer;
        g_RGRuleTimer = null;
    }
}

public Action Timer_SyncRules(Handle timer)
{
    SyncRules();
    return Plugin_Continue;
}

void StartApprovedPollTimer()
{
    StopApprovedPollTimer();
    g_RGPollTimer = CreateTimer(g_RGPollInterval.FloatValue, Timer_PollApproved, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

void StopApprovedPollTimer()
{
    if (g_RGPollTimer != null)
    {
        delete g_RGPollTimer;
        g_RGPollTimer = null;
    }
}

public Action Timer_PollApproved(Handle timer)
{
    PollApprovedRecords();
    return Plugin_Continue;
}

void DebugLog(const char[] format, any ...)
{
    if (g_RGDebugLog == null || !g_RGDebugLog.BoolValue)
    {
        return;
    }

    char message[512];
    VFormat(message, sizeof(message), format, 2);
    LogMessage("[cngokz-recordguard] %s", message);
}

void RecordGuardConsole(const char[] format, any ...)
{
    // Routine success diagnostics are intentionally silent. Failures still use LogError.
    if (format[0] == '\0')
    {
        return;
    }
}

bool GetApiConfig(char[] apiBaseUrl, int apiMaxLen, char[] token, int tokenMaxLen, int &port)
{
    apiBaseUrl[0] = '\0';
    token[0] = '\0';
    port = 0;

    if (LibraryExists("cngokz-core"))
    {
        CNGOKZCore_GetApiBaseUrl(apiBaseUrl, apiMaxLen);
        CNGOKZCore_GetReportToken(token, tokenMaxLen);
        port = CNGOKZCore_GetServerPort();
    }

    if ((apiBaseUrl[0] == '\0' || token[0] == '\0' || port <= 0) && LibraryExists("manger_online_reporter"))
    {
        MangerReporter_GetApiBaseUrl(apiBaseUrl, apiMaxLen);
        MangerReporter_GetReportToken(token, tokenMaxLen);
        port = MangerReporter_GetServerPort();
    }

    TrimString(apiBaseUrl);
    TrimString(token);
    return apiBaseUrl[0] != '\0' && token[0] != '\0' && port > 0;
}

bool BuildPluginApiUrl(const char[] suffix, char[] url, int maxLen)
{
    char baseUrl[CNGOKZ_MAX_URL_LENGTH];
    char token[CNGOKZ_MAX_TOKEN_LENGTH];
    int port = 0;
    if (!GetApiConfig(baseUrl, sizeof(baseUrl), token, sizeof(token), port))
    {
        return false;
    }

    int len = strlen(baseUrl);
    if (len > 0 && baseUrl[len - 1] == '/')
    {
        baseUrl[len - 1] = '\0';
    }
    Format(url, maxLen, "%s%s", baseUrl, suffix);
    return true;
}

HTTPRequest CreateJsonRequest(const char[] suffix)
{
    char url[CNGOKZ_MAX_URL_LENGTH];
    if (!BuildPluginApiUrl(suffix, url, sizeof(url)))
    {
        LogError("[cngokz-recordguard] Missing API base URL, token, or server port.");
        return null;
    }

    HTTPRequest request = new HTTPRequest(url);
    request.Timeout = g_RGRequestTimeout.IntValue;
    request.SetHeader("Accept", "application/json");
    return request;
}

void ApplyServerHeaders(HTTPRequest request)
{
    char apiBaseUrl[CNGOKZ_MAX_URL_LENGTH];
    char token[CNGOKZ_MAX_TOKEN_LENGTH];
    int port = 0;
    if (!GetApiConfig(apiBaseUrl, sizeof(apiBaseUrl), token, sizeof(token), port))
    {
        return;
    }

    request.SetHeader("x-cngokz-report-token", "%s", token);
    request.SetHeader("x-cngokz-server-port", "%d", port);
}

int GetTickrate()
{
    if (g_RGTickrate != null)
    {
        return g_RGTickrate.IntValue;
    }
    return RoundToNearest(1.0 / GetTickInterval());
}

void InitRecordGuardDb()
{
    if (g_RGDb != null)
    {
        return;
    }

    char error[256];
    g_RGDb = SQLite_UseDatabase(CNGOKZ_RECORDGUARD_DB, error, sizeof(error));
    if (g_RGDb == null)
    {
        LogError("[cngokz-recordguard] SQLite open failed: %s", error);
        return;
    }

    SQL_FastQuery(g_RGDb, "CREATE TABLE IF NOT EXISTS abnormal_replay_cache (record_id TEXT PRIMARY KEY, idempotency_key TEXT NOT NULL, replay_path TEXT NOT NULL, created_at INTEGER NOT NULL, r2_uploaded INTEGER NOT NULL DEFAULT 0, last_attempt INTEGER NOT NULL DEFAULT 0)");
    // Harmless on upgraded databases where the columns already exist.
    SQL_FastQuery(g_RGDb, "ALTER TABLE abnormal_replay_cache ADD COLUMN r2_uploaded INTEGER NOT NULL DEFAULT 0");
    SQL_FastQuery(g_RGDb, "ALTER TABLE abnormal_replay_cache ADD COLUMN last_attempt INTEGER NOT NULL DEFAULT 0");
}

void EnsureReplayCacheDir()
{
    char dir[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, dir, sizeof(dir), "data/cngokz-recordguard");
    if (!DirExists(dir))
    {
        CreateDirectory(dir, 511);
    }

    BuildPath(Path_SM, dir, sizeof(dir), CNGOKZ_REPLAY_CACHE_DIR);
    if (!DirExists(dir))
    {
        CreateDirectory(dir, 511);
    }
}
