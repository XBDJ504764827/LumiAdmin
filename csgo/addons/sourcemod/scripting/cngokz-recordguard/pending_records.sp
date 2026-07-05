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

    CreateAbnormalRecord(client, modeShort, timeTypeName);
    DebugLog("held abnormal record client=%N map=%s time=%.3f threshold=%.3f", client, g_CurrentMapName, runTime, threshold);
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
        return;
    }

    if (error[0] != '\0')
    {
        LogError("[cngokz-recordguard] Create abnormal record failed: %s", error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("[cngokz-recordguard] Create abnormal record returned HTTP status %d.", response.Status);
        return;
    }

    JSONObject root = view_as<JSONObject>(response.Data);
    JSONObject item = view_as<JSONObject>(root.Get("item"));
    if (item == null || !item.GetString("id", g_HeldRecordId[client], sizeof(g_HeldRecordId[])))
    {
        LogError("[cngokz-recordguard] Create abnormal record response has no item.id.");
        return;
    }

    g_HeldRecordCreated[client] = true;
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
    if (!g_HeldActive[client] || !g_HeldRecordCreated[client] || !g_HeldReplayCopied[client] || g_HeldReplayUploaded[client])
    {
        return;
    }

    if (!FileExists(g_HeldReplayPath[client]))
    {
        LogError("[cngokz-recordguard] Abnormal replay cache missing: %s", g_HeldReplayPath[client]);
        return;
    }

    char suffix[128];
    Format(suffix, sizeof(suffix), "/abnormal-records/%s/replay", g_HeldRecordId[client]);
    HTTPRequest request = CreateJsonRequest(suffix);
    if (request == null)
    {
        return;
    }

    ApplyServerHeaders(request);
    request.SetHeader("Content-Type", "application/vnd.gokz.replay");
    request.SetHeader("x-cngokz-file-name", "%s.replay", g_HeldIdempotencyKey[client]);
    request.UploadFile(g_HeldReplayPath[client], OnReplayUploaded, g_HeldUserId[client]);
}

public void OnReplayUploaded(HTTPStatus status, any value, const char[] error)
{
    int client = FindHeldClientByUserId(value);
    if (client <= 0)
    {
        return;
    }

    if (error[0] != '\0')
    {
        LogError("[cngokz-recordguard] Replay upload failed: %s", error);
        return;
    }

    if (status < HTTPStatus_OK || status >= HTTPStatus_MultipleChoices)
    {
        LogError("[cngokz-recordguard] Replay upload returned HTTP status %d.", status);
        return;
    }

    g_HeldReplayUploaded[client] = true;
    SaveReplayCacheForClient(client);
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
    Format(query, sizeof(query), "INSERT OR REPLACE INTO abnormal_replay_cache (record_id, idempotency_key, replay_path, created_at) VALUES ('%s', '%s', '%s', %d)", recordId, idem, replayPath, GetTime());
    SQL_FastQuery(g_RGDb, query);
}
