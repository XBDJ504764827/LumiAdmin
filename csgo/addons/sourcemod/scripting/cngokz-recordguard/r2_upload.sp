void RecordGuard_InitR2Upload()
{
    g_RGR2SteamWorksOK = (GetExtensionFileStatus("SteamWorks.ext") > 0);
}

void RecordGuard_OnAllPluginsLoaded()
{
    RecordGuard_InitR2Upload();
    if (!g_RGR2SteamWorksOK)
    {
        LogError("[cngokz-recordguard] SteamWorks extension not loaded; abnormal replays cannot be uploaded to R2.");
        return;
    }

    if (!SteamWorks_IsConnected())
    {
        LogError("[cngokz-recordguard] SteamWorks is loaded but the game server is not connected to Steam; R2 replay uploads may fail.");
    }
}

bool UploadHeldReplayToR2(int client)
{
    if (!g_RGR2SteamWorksOK)
    {
        LogError("[cngokz-recordguard] SteamWorks extension is unavailable; cannot upload abnormal replay.");
        RecordGuardConsole("upload blocked: SteamWorks extension is unavailable");
        return false;
    }

    if (!FileExists(g_HeldReplayPath[client]))
    {
        LogError("[cngokz-recordguard] Abnormal replay cache missing: %s", g_HeldReplayPath[client]);
        return false;
    }

    char modeShort[8];
    GetRecordGuardR2Mode(modeShort, sizeof(modeShort), g_HeldMode[client]);

    char timeMs[16];
    IntToString(RoundToNearest(g_HeldRunTime[client] * 1000.0), timeMs, sizeof(timeMs));

    return UploadAbnormalReplayFileToR2(
        g_HeldUserId[client],
        g_HeldRecordId[client],
        g_HeldIdempotencyKey[client],
        g_HeldReplayPath[client],
        modeShort,
        g_HeldMapName[client],
        timeMs);
}

bool UploadAbnormalReplayFileToR2(
    int userId,
    const char[] recordId,
    const char[] idempotencyKey,
    const char[] replayPath,
    const char[] modeShort,
    const char[] mapName,
    const char[] timeMs)
{
    char url[CNGOKZ_MAX_URL_LENGTH];
    char apiKey[CNGOKZ_MAX_TOKEN_LENGTH];
    bool verifyCert = false;
    if (!GetR2UploadConfig(url, sizeof(url), apiKey, sizeof(apiKey), verifyCert))
    {
        RecordGuardConsole("upload blocked: shared R2 URL/key config is incomplete or disabled");
        return false;
    }
    if (!FileExists(replayPath))
    {
        RecordGuardConsole("upload blocked: replay cache file does not exist: %s", replayPath);
        return false;
    }

    char storageKey[CNGOKZ_MAX_R2_KEY_LENGTH];
    Format(
        storageKey,
        sizeof(storageKey),
        "audit/%s/%s.replay",
        recordId,
        idempotencyKey);

    char fileName[128];
    Format(fileName, sizeof(fileName), "%s.replay", idempotencyKey);

    int fileSize = FileSize(replayPath);
    if (fileSize < 0)
    {
        fileSize = 0;
    }

    Handle request = SteamWorks_CreateHTTPRequest(k_EHTTPMethodPOST, url);
    if (request == null)
    {
        LogError("[cngokz-recordguard] Failed to create SteamWorks R2 upload request.");
        RecordGuardConsole("failed to create SteamWorks upload request: record_id=%s", recordId);
        return false;
    }

    int timeout = g_RGRequestTimeout != null ? g_RGRequestTimeout.IntValue : 30;
    SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, timeout);
    SteamWorks_SetHTTPRequestAbsoluteTimeoutMS(request, timeout * 1000);
    SteamWorks_SetHTTPRequestRequiresVerifiedCertificate(request, verifyCert);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-API-Key", apiKey);
    if (modeShort[0] != '\0')
    {
        SteamWorks_SetHTTPRequestHeaderValue(request, "X-GOKZ-Mode", modeShort);
    }
    if (mapName[0] != '\0')
    {
        SteamWorks_SetHTTPRequestHeaderValue(request, "X-Map", mapName);
    }
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-CNGOKZ-Object-Key", storageKey);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-CNGOKZ-Abnormal-Record-Id", recordId);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-CNGOKZ-Replay-Category", "abnormal");
    if (timeMs[0] != '\0')
    {
        SteamWorks_SetHTTPRequestHeaderValue(request, "X-Time-Ms", timeMs);
    }

    if (!SteamWorks_SetHTTPRequestRawPostBodyFromFile(request, "application/octet-stream", replayPath))
    {
        LogError("[cngokz-recordguard] Failed to attach replay body for R2 upload: %s", replayPath);
        RecordGuardConsole("failed to attach replay body: record_id=%s file=%s", recordId, replayPath);
        delete request;
        return false;
    }

    DataPack pack = new DataPack();
    pack.WriteCell(userId);
    pack.WriteString(recordId);
    pack.WriteString(storageKey);
    pack.WriteString(fileName);
    pack.WriteCell(fileSize);

    SteamWorks_SetHTTPRequestContextValue(request, pack);
    SteamWorks_SetHTTPCallbacks(request, OnAbnormalReplayR2Uploaded);

    if (!SteamWorks_SendHTTPRequest(request))
    {
        LogError("[cngokz-recordguard] Failed to send abnormal replay R2 upload request for %s.", storageKey);
        RecordGuardConsole("failed to dispatch Worker request: record_id=%s key=%s", recordId, storageKey);
        delete pack;
        delete request;
        return false;
    }

    MarkR2UploadAttempt(recordId);
    RecordGuardConsole("Worker upload request sent: record_id=%s key=%s size=%d", recordId, storageKey, fileSize);
    DebugLog("uploading abnormal replay to R2 key=%s file=%s", storageKey, replayPath);
    return true;
}

public void OnAbnormalReplayR2Uploaded(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, DataPack pack)
{
    pack.Reset();
    int userId = pack.ReadCell();
    char recordId[CNGOKZ_MAX_RECORD_ID];
    char storageKey[CNGOKZ_MAX_R2_KEY_LENGTH];
    char fileName[128];
    pack.ReadString(recordId, sizeof(recordId));
    pack.ReadString(storageKey, sizeof(storageKey));
    pack.ReadString(fileName, sizeof(fileName));
    int fileSize = pack.ReadCell();
    delete pack;

    int client = FindHeldClientByUserId(userId);
    if (client > 0)
    {
        g_HeldReplayUploadInFlight[client] = false;
    }

    int status = view_as<int>(statusCode);
    if (failure || !requestSuccessful || status < 200 || status >= 300)
    {
        bool timedOut = false;
        SteamWorks_GetHTTPRequestWasTimedOut(request, timedOut);
        LogError("[cngokz-recordguard] R2 replay upload failed for record %s. failure=%d successful=%d status=%d timeout=%d",
            recordId,
            failure ? 1 : 0,
            requestSuccessful ? 1 : 0,
            status,
            timedOut ? 1 : 0);
        RecordGuardConsole("Worker upload failed: record_id=%s status=%d failure=%d successful=%d timeout=%d", recordId, status, failure ? 1 : 0, requestSuccessful ? 1 : 0, timedOut ? 1 : 0);
        delete request;
        return;
    }

    delete request;
    RecordGuardConsole("Worker upload succeeded: record_id=%s key=%s status=%d; notifying website", recordId, storageKey, status);
    NotifyReplayMetadataUploaded(userId, recordId, storageKey, fileName, fileSize);
}

void NotifyReplayMetadataUploaded(int userId, const char[] recordId, const char[] storageKey, const char[] fileName, int fileSize)
{
    char suffix[160];
    Format(suffix, sizeof(suffix), "/abnormal-records/%s/replay-metadata", recordId);

    HTTPRequest request = CreateJsonRequest(suffix);
    if (request == null)
    {
        RecordGuardConsole("metadata notification blocked: cannot create website request for record_id=%s", recordId);
        return;
    }

    char apiBaseUrl[CNGOKZ_MAX_URL_LENGTH];
    char token[CNGOKZ_MAX_TOKEN_LENGTH];
    int port = 0;
    if (!GetApiConfig(apiBaseUrl, sizeof(apiBaseUrl), token, sizeof(token), port))
    {
        RecordGuardConsole("metadata notification blocked: plugin API config is incomplete for record_id=%s", recordId);
        return;
    }

    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", port);
    payload.SetString("storage_key", storageKey);
    payload.SetString("file_name", fileName);
    payload.SetInt("file_size", fileSize);
    payload.SetString("content_type", "application/vnd.gokz.replay");

    DataPack context = new DataPack();
    context.WriteCell(userId);
    context.WriteString(recordId);
    request.Post(payload, OnReplayMetadataUploaded, context);
    RecordGuardConsole("website metadata request sent: record_id=%s key=%s", recordId, storageKey);
}

public void OnReplayMetadataUploaded(HTTPResponse response, any value, const char[] error)
{
    DataPack context = view_as<DataPack>(value);
    context.Reset();
    int userId = context.ReadCell();
    char recordId[CNGOKZ_MAX_RECORD_ID];
    context.ReadString(recordId, sizeof(recordId));
    delete context;
    int client = FindHeldClientByUserId(userId);

    if (error[0] != '\0')
    {
        LogError("[cngokz-recordguard] Replay metadata update failed: %s", error);
        RecordGuardConsole("website metadata update failed: record_id=%s error=%s", recordId, error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("[cngokz-recordguard] Replay metadata update returned HTTP status %d.", response.Status);
        RecordGuardConsole("website metadata update failed: record_id=%s HTTP=%d", recordId, response.Status);
        return;
    }

    if (client > 0)
    {
        g_HeldReplayUploaded[client] = true;
        g_HeldReplayPath[client][0] = '\0';
    }
    MarkR2UploadComplete(recordId);
    char replayPath[PLATFORM_MAX_PATH];
    if (LoadReplayPath(recordId, replayPath, sizeof(replayPath)))
    {
        DeleteCachedReplay(recordId, replayPath);
    }
    RecordGuardConsole("abnormal replay fully attached to website record: record_id=%s", recordId);
}

void StartR2RetryTimer()
{
    if (g_RGR2RetryTimer == null)
    {
        g_RGR2RetryTimer = CreateTimer(30.0, Timer_RetryPendingR2Uploads, _, TIMER_REPEAT);
        RecordGuardConsole("persistent replay retry timer started (interval=30s)");
    }
}

void StopR2RetryTimer()
{
    if (g_RGR2RetryTimer != null)
    {
        delete g_RGR2RetryTimer;
        g_RGR2RetryTimer = null;
    }
}

public Action Timer_RetryPendingR2Uploads(Handle timer)
{
    RetryPendingR2Uploads();
    return Plugin_Continue;
}

void RetryPendingR2Uploads()
{
    if (g_RGDb == null || !g_RGR2SteamWorksOK)
    {
        return;
    }

    char query[512];
    Format(query, sizeof(query), "SELECT record_id, idempotency_key, replay_path FROM abnormal_replay_cache WHERE r2_uploaded = 0 AND last_attempt <= %d ORDER BY created_at ASC LIMIT 5", GetTime() - 60);
    DBResultSet results = SQL_Query(g_RGDb, query);
    if (results == null)
    {
        return;
    }

    int pendingCount = 0;
    while (SQL_FetchRow(results))
    {
        pendingCount++;
        char recordId[CNGOKZ_MAX_RECORD_ID];
        char idempotencyKey[CNGOKZ_MAX_IDEMPOTENCY_KEY];
        char replayPath[PLATFORM_MAX_PATH];
        SQL_FetchString(results, 0, recordId, sizeof(recordId));
        SQL_FetchString(results, 1, idempotencyKey, sizeof(idempotencyKey));
        SQL_FetchString(results, 2, replayPath, sizeof(replayPath));

        if (!FileExists(replayPath))
        {
            LogError("[cngokz-recordguard] Pending abnormal replay cache file is missing: %s", replayPath);
            RecordGuardConsole("retry skipped: cached replay is missing record_id=%s file=%s", recordId, replayPath);
            MarkR2UploadAttempt(recordId);
            continue;
        }

        RecordGuardConsole("retrying pending replay: record_id=%s file=%s", recordId, replayPath);
        UploadAbnormalReplayFileToR2(0, recordId, idempotencyKey, replayPath, "", "", "");
    }
    delete results;
    if (pendingCount > 0)
    {
        RecordGuardConsole("persistent retry scan processed %d pending replay(s)", pendingCount);
    }
    CleanupUploadedReplayCache();
}

void CleanupUploadedReplayCache()
{
    if (g_RGDb == null)
    {
        return;
    }

    DBResultSet results = SQL_Query(g_RGDb, "SELECT record_id, replay_path FROM abnormal_replay_cache WHERE r2_uploaded = 1 ORDER BY created_at ASC LIMIT 20");
    if (results == null)
    {
        return;
    }

    while (SQL_FetchRow(results))
    {
        char recordId[CNGOKZ_MAX_RECORD_ID];
        char replayPath[PLATFORM_MAX_PATH];
        SQL_FetchString(results, 0, recordId, sizeof(recordId));
        SQL_FetchString(results, 1, replayPath, sizeof(replayPath));
        DeleteCachedReplay(recordId, replayPath);
    }
    delete results;
}

void MarkR2UploadAttempt(const char[] recordId)
{
    if (g_RGDb == null)
    {
        return;
    }
    char escaped[128];
    SQL_EscapeString(g_RGDb, recordId, escaped, sizeof(escaped));
    char query[256];
    Format(query, sizeof(query), "UPDATE abnormal_replay_cache SET last_attempt = %d WHERE record_id = '%s'", GetTime(), escaped);
    SQL_FastQuery(g_RGDb, query);
}

void MarkR2UploadComplete(const char[] recordId)
{
    if (g_RGDb == null)
    {
        return;
    }
    char escaped[128];
    SQL_EscapeString(g_RGDb, recordId, escaped, sizeof(escaped));
    char query[256];
    Format(query, sizeof(query), "UPDATE abnormal_replay_cache SET r2_uploaded = 1 WHERE record_id = '%s'", escaped);
    SQL_FastQuery(g_RGDb, query);
}

bool GetR2UploadConfig(char[] url, int urlMaxLen, char[] apiKey, int keyMaxLen, bool &verifyCert)
{
    url[0] = '\0';
    apiKey[0] = '\0';
    verifyCert = false;
    if (g_RGR2UploadEnabled != null && !g_RGR2UploadEnabled.BoolValue)
    {
        return false;
    }
    if (!CNGOKZCore_IsReplayR2Enabled())
    {
        return false;
    }
    return CNGOKZCore_GetReplayR2Config(url, urlMaxLen, apiKey, keyMaxLen, verifyCert);
}

void GetRecordGuardR2Mode(char[] buffer, int maxLen, int mode)
{
    if (mode >= 0 && mode < MODE_COUNT)
    {
        strcopy(buffer, maxLen, gC_ModeNamesShort[mode]);
        StringToLower(buffer);
        return;
    }

    strcopy(buffer, maxLen, "unk");
}

void StringToLower(char[] value)
{
    for (int i = 0; value[i] != '\0'; i++)
    {
        if (value[i] >= 'A' && value[i] <= 'Z')
        {
            value[i] = view_as<char>(value[i] + 32);
        }
    }
}
