void HoldAbnormalRecord(int client, int course, int mode, int timeType, float runTime, int teleportsUsed, int mapId, const char[] modeShort, const char[] timeTypeName, float threshold)
{
    ClearHeldRecord(client);

    g_HeldActive[client] = true;
    g_HeldUserId[client] = GetClientUserId(client);
    g_HeldCourse[client] = course;
    g_HeldMode[client] = mode;
    g_HeldTimeType[client] = timeType;
    g_HeldTeleports[client] = teleportsUsed;
    g_HeldMapId[client] = mapId;
    g_HeldRunTime[client] = runTime;
    g_HeldThreshold[client] = threshold;
    strcopy(g_HeldMapName[client], sizeof(g_HeldMapName[]), g_CurrentMapName);
    GetClientName(client, g_HeldPlayerName[client], sizeof(g_HeldPlayerName[]));
    GetClientAuthId(client, AuthId_SteamID64, g_HeldSteamId64[client], sizeof(g_HeldSteamId64[]), true);
    GetClientAuthId(client, AuthId_Steam2, g_HeldSteamId2[client], sizeof(g_HeldSteamId2[]), true);

    Format(
        g_HeldIdempotencyKey[client],
        sizeof(g_HeldIdempotencyKey[]),
        "%d-%d-%s-%d-%d-%d",
        GetTime(),
        GetGameTickCount(),
        g_HeldSteamId64[client],
        mapId,
        course,
        RoundToNearest(runTime * 1000.0));

    bool replaySaveForced = CNGOKZ_RP_ForceSaveRun(client, course, runTime, teleportsUsed);
    RecordGuardConsole("forced GOKZ replay save for abnormal review: player=%N result=%d", client, replaySaveForced ? 1 : 0);
    TryCaptureRecentReplay(client);
    CreateAbnormalRecord(client, modeShort, timeTypeName);
    DataPack replayWatch = new DataPack();
    replayWatch.WriteCell(g_HeldUserId[client]);
    replayWatch.WriteString(g_HeldIdempotencyKey[client]);
    CreateTimer(5.0, Timer_CheckHeldReplayArrival, replayWatch, TIMER_FLAG_NO_MAPCHANGE | TIMER_DATA_HNDL_CLOSE);
    RecordGuardConsole(
        "abnormal run held: player=%N steamid64=%s map=%s course=%d mode=%s type=%s time=%.3f threshold=%.3f idempotency=%s",
        client,
        g_HeldSteamId64[client],
        g_CurrentMapName,
        course,
        modeShort,
        timeTypeName,
        runTime,
        threshold,
        g_HeldIdempotencyKey[client]);
    DebugLog("held abnormal record client=%N map=%s time=%.3f threshold=%.3f", client, g_CurrentMapName, runTime, threshold);
}

public Action Timer_CheckHeldReplayArrival(Handle timer, DataPack replayWatch)
{
    replayWatch.Reset();
    int userId = replayWatch.ReadCell();
    char idempotencyKey[CNGOKZ_MAX_IDEMPOTENCY_KEY];
    replayWatch.ReadString(idempotencyKey, sizeof(idempotencyKey));
    int client = FindHeldClientByUserId(userId);
    if (client > 0 && StrEqual(g_HeldIdempotencyKey[client], idempotencyKey) && !g_HeldReplayCopied[client])
    {
        RecordGuardConsole("no replay file arrived within 5 seconds for held record: player=%N idempotency=%s; GOKZ Replays may have disabled this non-PB/non-server-record run", client, idempotencyKey);
    }
    return Plugin_Stop;
}

void ClearHeldRecord(int client)
{
    if (client < 1 || client > MaxClients)
    {
        return;
    }

    g_HeldActive[client] = false;
    g_HeldUserId[client] = 0;
    g_HeldCourse[client] = 0;
    g_HeldMode[client] = 0;
    g_HeldTimeType[client] = 0;
    g_HeldTeleports[client] = 0;
    g_HeldMapId[client] = 0;
    g_HeldRunTime[client] = 0.0;
    g_HeldThreshold[client] = 0.0;
    g_HeldRecordCreated[client] = false;
    g_HeldReplayCopied[client] = false;
    g_HeldReplayUploaded[client] = false;
    g_HeldReplayUploadInFlight[client] = false;
    g_HeldRecordId[client][0] = '\0';
    g_HeldIdempotencyKey[client][0] = '\0';
    g_HeldReplayPath[client][0] = '\0';
    g_HeldMapName[client][0] = '\0';
    g_HeldSteamId64[client][0] = '\0';
    g_HeldSteamId2[client][0] = '\0';
    g_HeldPlayerName[client][0] = '\0';
}

void CreateAbnormalRecord(int client, const char[] modeShort, const char[] timeTypeName)
{
    HTTPRequest request = CreateJsonRequest("/abnormal-records");
    if (request == null)
    {
        RecordGuardConsole("cannot create abnormal-record API request for player=%N", client);
        return;
    }

    char apiBaseUrl[CNGOKZ_MAX_URL_LENGTH];
    char token[CNGOKZ_MAX_TOKEN_LENGTH];
    int port = 0;
    if (!GetApiConfig(apiBaseUrl, sizeof(apiBaseUrl), token, sizeof(token), port))
    {
        RecordGuardConsole("cannot create abnormal record: plugin API config is incomplete");
        return;
    }

    JSONObject payload = new JSONObject();
    payload.SetString("report_token", token);
    payload.SetInt("port", port);
    payload.SetString("idempotency_key", g_HeldIdempotencyKey[client]);
    payload.SetString("steam_id64", g_HeldSteamId64[client]);
    payload.SetString("steam_id2", g_HeldSteamId2[client]);
    payload.SetString("player_name", g_HeldPlayerName[client]);
    payload.SetString("map_name", g_HeldMapName[client]);
    payload.SetInt("map_id", g_HeldMapId[client]);
    payload.SetInt("course", g_HeldCourse[client]);
    payload.SetString("mode", modeShort);
    payload.SetString("time_type", timeTypeName);
    payload.SetInt("teleports", g_HeldTeleports[client]);
    payload.SetFloat("run_time_seconds", g_HeldRunTime[client]);
    payload.SetFloat("threshold_seconds", g_HeldThreshold[client]);

    request.Post(payload, OnCreateAbnormalRecordResponse, g_HeldUserId[client]);
}

public void OnCreateAbnormalRecordResponse(HTTPResponse response, any value, const char[] error)
{
    int client = FindHeldClientByUserId(value);
    if (client <= 0)
    {
        RecordGuardConsole("abnormal-record API response ignored: player is no longer active");
        return;
    }

    if (error[0] != '\0')
    {
        LogError("[cngokz-recordguard] Create abnormal record failed: %s", error);
        RecordGuardConsole("create abnormal record failed: %s", error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("[cngokz-recordguard] Create abnormal record returned HTTP status %d.", response.Status);
        RecordGuardConsole("create abnormal record returned HTTP status %d", response.Status);
        return;
    }

    JSONObject root = view_as<JSONObject>(response.Data);
    JSONObject item = view_as<JSONObject>(root.Get("item"));
    if (item == null || !item.GetString("id", g_HeldRecordId[client], sizeof(g_HeldRecordId[])))
    {
        LogError("[cngokz-recordguard] Create abnormal record response has no item.id.");
        RecordGuardConsole("create abnormal record response has no item.id");
        return;
    }

    g_HeldRecordCreated[client] = true;
    RecordGuardConsole("abnormal record created: record_id=%s player=%N replay_copied=%d", g_HeldRecordId[client], client, g_HeldReplayCopied[client] ? 1 : 0);
    SaveReplayCacheForClient(client);
    TryUploadHeldReplay(client);
}

int FindHeldClientByUserId(int userId)
{
    for (int client = 1; client <= MaxClients; client++)
    {
        if (g_HeldActive[client] && g_HeldUserId[client] == userId)
        {
            return client;
        }
    }
    return 0;
}

void TryUploadHeldReplay(int client)
{
    if (!g_HeldActive[client] || !g_HeldRecordCreated[client] || !g_HeldReplayCopied[client] || g_HeldReplayUploaded[client] || g_HeldReplayUploadInFlight[client])
    {
        return;
    }

    if (!FileExists(g_HeldReplayPath[client]))
    {
        LogError("[cngokz-recordguard] Abnormal replay cache missing: %s", g_HeldReplayPath[client]);
        return;
    }

    RecordGuardConsole("dispatching abnormal replay upload: record_id=%s file=%s", g_HeldRecordId[client], g_HeldReplayPath[client]);
    if (UploadHeldReplayToR2(client))
    {
        g_HeldReplayUploadInFlight[client] = true;
    }
    else
    {
        RecordGuardConsole("abnormal replay upload was not dispatched: record_id=%s", g_HeldRecordId[client]);
    }
}

void SaveReplayCacheForClient(int client)
{
    if (g_RGDb == null || g_HeldRecordId[client][0] == '\0' || g_HeldReplayPath[client][0] == '\0')
    {
        return;
    }

    char recordId[128];
    char idem[160];
    char replayPath[PLATFORM_MAX_PATH * 2];
    SQL_EscapeString(g_RGDb, g_HeldRecordId[client], recordId, sizeof(recordId));
    SQL_EscapeString(g_RGDb, g_HeldIdempotencyKey[client], idem, sizeof(idem));
    SQL_EscapeString(g_RGDb, g_HeldReplayPath[client], replayPath, sizeof(replayPath));

    char query[1024];
    Format(query, sizeof(query), "INSERT OR REPLACE INTO abnormal_replay_cache (record_id, idempotency_key, replay_path, created_at, r2_uploaded, last_attempt) VALUES ('%s', '%s', '%s', %d, 0, 0)", recordId, idem, replayPath, GetTime());
    SQL_FastQuery(g_RGDb, query);
    RecordGuardConsole("abnormal replay cached locally: record_id=%s file=%s", g_HeldRecordId[client], g_HeldReplayPath[client]);
}
