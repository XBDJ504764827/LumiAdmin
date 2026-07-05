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

    item.GetString("id", recordId, sizeof(recordId));
    item.GetString("steam_id2", steamId2, sizeof(steamId2));
    item.GetString("mode", modeShort, sizeof(modeShort));
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

    if (!LoadReplayPath(recordId, replayPath, sizeof(replayPath)) || !FileExists(replayPath))
    {
        SubmitRecordResult(recordId, false, 0, "local replay cache missing");
        return;
    }

    DataPack pack = new DataPack();
    pack.WriteString(recordId);
    pack.WriteString(replayPath);

    if (!GlobalAPI_CreateRecord(OnGlobalRecordCreated, pack, steamId2, mapId, modeGlobal, course, GetTickrate(), teleports, runTime))
    {
        delete pack;
        SubmitRecordResult(recordId, false, 0, "GlobalAPI_CreateRecord failed to dispatch");
    }
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
        DeleteFile(replayPath);
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
