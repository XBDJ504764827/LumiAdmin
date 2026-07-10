void PollApprovedRecords()
{
    if (g_RGEnabled == null || !g_RGEnabled.BoolValue)
    {
        return;
    }

    HTTPRequest request = CreateJsonRequest("/abnormal-records/poll-approved");
    if (request == null)
    {
        return;
    }

    char apiBaseUrl[CNGOKZ_MAX_URL_LENGTH];
    char token[CNGOKZ_MAX_TOKEN_LENGTH];
    int port = 0;
    if (!GetApiConfig(apiBaseUrl, sizeof(apiBaseUrl), token, sizeof(token), port))
    {
        return;
    }

    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", port);
    payload.SetInt("limit", 5);
    request.Post(payload, OnApprovedRecordsPolled);
}

public void OnApprovedRecordsPolled(HTTPResponse response, any value, const char[] error)
{
    if (error[0] != '\0')
    {
        LogError("[cngokz-recordguard] Poll approved failed: %s", error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("[cngokz-recordguard] Poll approved returned HTTP status %d.", response.Status);
        return;
    }

    JSONObject root = view_as<JSONObject>(response.Data);
    JSON rawItems = root.Get("items");
    if (rawItems == null)
    {
        return;
    }

    JSONArray items = view_as<JSONArray>(rawItems);
    for (int i = 0; i < items.Length; i++)
    {
        JSONObject item = view_as<JSONObject>(items.Get(i));
        if (item != null)
        {
            SubmitApprovedRecord(item);
        }
    }
}

void SubmitApprovedRecord(JSONObject item)
{
    char recordId[CNGOKZ_MAX_RECORD_ID];
    char steamId2[32];
    char modeShort[16];
    char modeGlobal[32];
    char replayPath[PLATFORM_MAX_PATH];
    char storageKey[CNGOKZ_MAX_R2_KEY_LENGTH];

    item.GetString("id", recordId, sizeof(recordId));
    item.GetString("steam_id2", steamId2, sizeof(steamId2));
    item.GetString("mode", modeShort, sizeof(modeShort));
    item.GetString("replay_storage_key", storageKey, sizeof(storageKey));
    GetGlobalModeNameFromShort(modeShort, modeGlobal, sizeof(modeGlobal));

    int mapId = item.GetInt("map_id");
    int course = item.GetInt("course");
    int teleports = item.GetInt("teleports");
    float runTime = item.GetFloat("run_time_seconds");

    if (recordId[0] == '\0' || steamId2[0] == '\0' || mapId <= 0)
    {
        SubmitRecordResult(recordId, false, 0, "missing steam_id2 or map_id");
        return;
    }

    if (LoadReplayPath(recordId, replayPath, sizeof(replayPath)) && FileExists(replayPath))
    {
        StartApprovedGlobalSubmission(recordId, steamId2, modeGlobal, mapId, course, teleports, runTime, replayPath);
        return;
    }

    if (storageKey[0] == '\0')
    {
        SubmitRecordResult(recordId, false, 0, "R2 replay storage key missing");
        return;
    }

    DownloadApprovedReplay(recordId, steamId2, modeGlobal, mapId, course, teleports, runTime, storageKey);
}

void StartApprovedGlobalSubmission(
    const char[] recordId,
    const char[] steamId2,
    const char[] modeGlobal,
    int mapId,
    int course,
    int teleports,
    float runTime,
    const char[] replayPath)
{

    DataPack pack = new DataPack();
    pack.WriteString(recordId);
    pack.WriteString(replayPath);

    if (!GlobalAPI_CreateRecord(OnGlobalRecordCreated, pack, steamId2, mapId, modeGlobal, course, GetTickrate(), teleports, runTime))
    {
        delete pack;
        SubmitRecordResult(recordId, false, 0, "GlobalAPI_CreateRecord failed to dispatch");
    }
}

void DownloadApprovedReplay(
    const char[] recordId,
    const char[] steamId2,
    const char[] modeGlobal,
    int mapId,
    int course,
    int teleports,
    float runTime,
    const char[] storageKey)
{
    char uploadUrl[CNGOKZ_MAX_URL_LENGTH];
    char apiKey[CNGOKZ_MAX_TOKEN_LENGTH];
    bool verifyCert = false;
    if (!GetR2UploadConfig(uploadUrl, sizeof(uploadUrl), apiKey, sizeof(apiKey), verifyCert))
    {
        SubmitRecordResult(recordId, false, 0, "R2 download config missing");
        return;
    }

    char baseUrl[CNGOKZ_MAX_URL_LENGTH];
    strcopy(baseUrl, sizeof(baseUrl), uploadUrl);
    int length = strlen(baseUrl);
    if (length >= 7 && StrContains(baseUrl, "/upload", false) == length - 7)
    {
        baseUrl[length - 7] = '\0';
    }
    else
    {
        while (length > 0 && baseUrl[length - 1] == '/')
        {
            baseUrl[--length] = '\0';
        }
    }

    char downloadUrl[CNGOKZ_MAX_URL_LENGTH];
    Format(downloadUrl, sizeof(downloadUrl), "%s/%s", baseUrl, storageKey);

    EnsureReplayCacheDir();
    char replayPath[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, replayPath, sizeof(replayPath), "%s/%s.replay", CNGOKZ_REPLAY_CACHE_DIR, recordId);

    Handle request = SteamWorks_CreateHTTPRequest(k_EHTTPMethodGET, downloadUrl);
    if (request == null)
    {
        SubmitRecordResult(recordId, false, 0, "failed to create R2 download request");
        return;
    }

    int timeout = g_RGRequestTimeout != null ? g_RGRequestTimeout.IntValue : 30;
    SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, timeout);
    SteamWorks_SetHTTPRequestAbsoluteTimeoutMS(request, timeout * 1000);
    SteamWorks_SetHTTPRequestRequiresVerifiedCertificate(request, verifyCert);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-API-Key", apiKey);

    DataPack pack = new DataPack();
    pack.WriteString(recordId);
    pack.WriteString(steamId2);
    pack.WriteString(modeGlobal);
    pack.WriteCell(mapId);
    pack.WriteCell(course);
    pack.WriteCell(teleports);
    pack.WriteFloat(runTime);
    pack.WriteString(replayPath);
    SteamWorks_SetHTTPRequestContextValue(request, pack);
    SteamWorks_SetHTTPCallbacks(request, OnApprovedReplayDownloaded);

    if (!SteamWorks_SendHTTPRequest(request))
    {
        delete pack;
        delete request;
        SubmitRecordResult(recordId, false, 0, "failed to dispatch R2 replay download");
    }
}

public void OnApprovedReplayDownloaded(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, DataPack pack)
{
    pack.Reset();
    char recordId[CNGOKZ_MAX_RECORD_ID];
    char steamId2[32];
    char modeGlobal[32];
    char replayPath[PLATFORM_MAX_PATH];
    pack.ReadString(recordId, sizeof(recordId));
    pack.ReadString(steamId2, sizeof(steamId2));
    pack.ReadString(modeGlobal, sizeof(modeGlobal));
    int mapId = pack.ReadCell();
    int course = pack.ReadCell();
    int teleports = pack.ReadCell();
    float runTime = pack.ReadFloat();
    pack.ReadString(replayPath, sizeof(replayPath));
    delete pack;

    int status = view_as<int>(statusCode);
    if (failure || !requestSuccessful || status < 200 || status >= 300)
    {
        delete request;
        SubmitRecordResult(recordId, false, 0, "R2 replay download failed");
        return;
    }

    if (!SteamWorks_WriteHTTPResponseBodyToFile(request, replayPath))
    {
        delete request;
        SubmitRecordResult(recordId, false, 0, "failed to write R2 replay to local cache");
        return;
    }
    delete request;

    StartApprovedGlobalSubmission(recordId, steamId2, modeGlobal, mapId, course, teleports, runTime, replayPath);
}

public int OnGlobalRecordCreated(JSON_Object response, GlobalAPIRequestData request, DataPack pack)
{
    pack.Reset();
    char recordId[CNGOKZ_MAX_RECORD_ID];
    char replayPath[PLATFORM_MAX_PATH];
    pack.ReadString(recordId, sizeof(recordId));
    pack.ReadString(replayPath, sizeof(replayPath));

    if (request.Failure)
    {
        delete pack;
        SubmitRecordResult(recordId, false, 0, "GlobalAPI_CreateRecord request failed");
        return 0;
    }

    int globalRecordId = response.GetInt("record_id");
    if (globalRecordId <= 0)
    {
        delete pack;
        SubmitRecordResult(recordId, false, 0, "GlobalAPI response missing record_id");
        return 0;
    }

    DataPack replayPack = new DataPack();
    replayPack.WriteString(recordId);
    replayPack.WriteString(replayPath);
    replayPack.WriteCell(globalRecordId);

    if (!GlobalAPI_CreateReplayForRecordId(OnGlobalReplayCreated, replayPack, globalRecordId, replayPath))
    {
        delete replayPack;
        SubmitRecordResult(recordId, false, globalRecordId, "GlobalAPI_CreateReplayForRecordId failed to dispatch");
    }

    delete pack;
    return 0;
}

public int OnGlobalReplayCreated(JSON_Object response, GlobalAPIRequestData request, DataPack pack)
{
    pack.Reset();
    char recordId[CNGOKZ_MAX_RECORD_ID];
    char replayPath[PLATFORM_MAX_PATH];
    int globalRecordId = 0;
    pack.ReadString(recordId, sizeof(recordId));
    pack.ReadString(replayPath, sizeof(replayPath));
    globalRecordId = pack.ReadCell();
    delete pack;

    if (request.Failure)
    {
        SubmitRecordResult(recordId, false, globalRecordId, "GlobalAPI_CreateReplayForRecordId request failed");
        return 0;
    }

    SubmitRecordResult(recordId, true, globalRecordId, "");
    DeleteCachedReplay(recordId, replayPath);
    return 0;
}

void SubmitRecordResult(const char[] recordId, bool success, int globalRecordId, const char[] error)
{
    if (recordId[0] == '\0')
    {
        return;
    }

    char suffix[160];
    Format(suffix, sizeof(suffix), "/abnormal-records/%s/submit-result", recordId);
    HTTPRequest request = CreateJsonRequest(suffix);
    if (request == null)
    {
        return;
    }

    char apiBaseUrl[CNGOKZ_MAX_URL_LENGTH];
    char token[CNGOKZ_MAX_TOKEN_LENGTH];
    int port = 0;
    if (!GetApiConfig(apiBaseUrl, sizeof(apiBaseUrl), token, sizeof(token), port))
    {
        return;
    }

    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", port);
    payload.SetString("status", success ? "submitted" : "failed");
    if (success)
    {
        payload.SetInt("global_record_id", globalRecordId);
    }
    else
    {
        payload.SetString("error", error);
    }
    request.Post(payload, OnSubmitResultResponse);
}

public void OnSubmitResultResponse(HTTPResponse response, any value, const char[] error)
{
    if (error[0] != '\0')
    {
        LogError("[cngokz-recordguard] Submit result failed: %s", error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("[cngokz-recordguard] Submit result returned HTTP status %d.", response.Status);
    }
}

bool LoadReplayPath(const char[] recordId, char[] replayPath, int maxLen)
{
    replayPath[0] = '\0';
    if (g_RGDb == null)
    {
        return false;
    }

    char escaped[128];
    SQL_EscapeString(g_RGDb, recordId, escaped, sizeof(escaped));

    char query[256];
    Format(query, sizeof(query), "SELECT replay_path FROM abnormal_replay_cache WHERE record_id = '%s' LIMIT 1", escaped);
    DBResultSet results = SQL_Query(g_RGDb, query);
    if (results == null)
    {
        return false;
    }

    bool found = false;
    if (SQL_FetchRow(results))
    {
        SQL_FetchString(results, 0, replayPath, maxLen);
        found = replayPath[0] != '\0';
    }
    delete results;
    return found;
}

void DeleteCachedReplay(const char[] recordId, const char[] replayPath)
{
    if (replayPath[0] != '\0' && FileExists(replayPath))
    {
        if (!DeleteFile(replayPath))
        {
            return;
        }
    }
    if (g_RGDb == null)
    {
        return;
    }

    char escaped[128];
    SQL_EscapeString(g_RGDb, recordId, escaped, sizeof(escaped));
    char query[256];
    Format(query, sizeof(query), "DELETE FROM abnormal_replay_cache WHERE record_id = '%s'", escaped);
    SQL_FastQuery(g_RGDb, query);
}
