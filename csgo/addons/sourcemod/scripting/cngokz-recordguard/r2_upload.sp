void RecordGuard_CreateLegacyR2UploadConVars()
{
    g_RGLegacyR2UploadEnabled = FindConVar("gokz_r2upload_enabled");
    if (g_RGLegacyR2UploadEnabled == null)
    {
        g_RGLegacyR2UploadEnabled = CreateConVar("gokz_r2upload_enabled", "1", "Compatibility switch for legacy gokz-r2upload R2 replay uploads.", _, true, 0.0, true, 1.0);
    }

    g_RGLegacyR2UploadUrl = FindConVar("gokz_r2upload_url");
    if (g_RGLegacyR2UploadUrl == null)
    {
        g_RGLegacyR2UploadUrl = CreateConVar("gokz_r2upload_url", "", "Compatibility Cloudflare Worker URL for legacy gokz-r2upload.");
    }

    g_RGLegacyR2UploadKey = FindConVar("gokz_r2upload_key");
    if (g_RGLegacyR2UploadKey == null)
    {
        g_RGLegacyR2UploadKey = CreateConVar("gokz_r2upload_key", "", "Compatibility Cloudflare Worker API key for legacy gokz-r2upload.");
    }

    g_RGLegacyR2UploadVerifyCert = FindConVar("gokz_r2upload_verify_cert");
    if (g_RGLegacyR2UploadVerifyCert == null)
    {
        g_RGLegacyR2UploadVerifyCert = CreateConVar("gokz_r2upload_verify_cert", "0", "Compatibility HTTPS certificate verification switch for legacy gokz-r2upload.", _, true, 0.0, true, 1.0);
    }
}

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
        return false;
    }

    char url[CNGOKZ_MAX_URL_LENGTH];
    char apiKey[CNGOKZ_MAX_TOKEN_LENGTH];
    bool verifyCert = false;
    if (!GetR2UploadConfig(url, sizeof(url), apiKey, sizeof(apiKey), verifyCert))
    {
        LogError("[cngokz-recordguard] Missing R2 upload URL or key; set cngokz_recordguard_r2upload_* or legacy gokz_r2upload_*.");
        return false;
    }

    if (!FileExists(g_HeldReplayPath[client]))
    {
        LogError("[cngokz-recordguard] Abnormal replay cache missing: %s", g_HeldReplayPath[client]);
        return false;
    }

    char modeShort[8];
    GetRecordGuardR2Mode(modeShort, sizeof(modeShort), g_HeldMode[client]);

    char storageKey[CNGOKZ_MAX_R2_KEY_LENGTH];
    Format(
        storageKey,
        sizeof(storageKey),
        "audit/%s/%s.replay",
        g_HeldRecordId[client],
        g_HeldIdempotencyKey[client]);

    char fileName[128];
    Format(fileName, sizeof(fileName), "%s.replay", g_HeldIdempotencyKey[client]);

    int fileSize = FileSize(g_HeldReplayPath[client]);
    if (fileSize < 0)
    {
        fileSize = 0;
    }

    Handle request = SteamWorks_CreateHTTPRequest(k_EHTTPMethodPOST, url);
    if (request == null)
    {
        LogError("[cngokz-recordguard] Failed to create SteamWorks R2 upload request.");
        return false;
    }

    int timeout = g_RGRequestTimeout != null ? g_RGRequestTimeout.IntValue : 30;
    SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, timeout);
    SteamWorks_SetHTTPRequestAbsoluteTimeoutMS(request, timeout * 1000);
    SteamWorks_SetHTTPRequestRequiresVerifiedCertificate(request, verifyCert);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-API-Key", apiKey);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-GOKZ-Mode", modeShort);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-Map", g_HeldMapName[client]);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-CNGOKZ-Object-Key", storageKey);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-CNGOKZ-Abnormal-Record-Id", g_HeldRecordId[client]);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-CNGOKZ-Replay-Category", "abnormal");

    char timeMs[16];
    IntToString(RoundToNearest(g_HeldRunTime[client] * 1000.0), timeMs, sizeof(timeMs));
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-Time-Ms", timeMs);

    if (!SteamWorks_SetHTTPRequestRawPostBodyFromFile(request, "application/octet-stream", g_HeldReplayPath[client]))
    {
        LogError("[cngokz-recordguard] Failed to attach replay body for R2 upload: %s", g_HeldReplayPath[client]);
        delete request;
        return false;
    }

    DataPack pack = new DataPack();
    pack.WriteCell(g_HeldUserId[client]);
    pack.WriteString(g_HeldRecordId[client]);
    pack.WriteString(storageKey);
    pack.WriteString(fileName);
    pack.WriteCell(fileSize);

    SteamWorks_SetHTTPRequestContextValue(request, pack);
    SteamWorks_SetHTTPCallbacks(request, OnAbnormalReplayR2Uploaded);

    if (!SteamWorks_SendHTTPRequest(request))
    {
        LogError("[cngokz-recordguard] Failed to send abnormal replay R2 upload request for %s.", storageKey);
        delete pack;
        delete request;
        return false;
    }

    DebugLog("uploading abnormal replay to R2 key=%s file=%s", storageKey, g_HeldReplayPath[client]);
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
        delete request;
        return;
    }

    delete request;
    NotifyReplayMetadataUploaded(userId, recordId, storageKey, fileName, fileSize);
}

void NotifyReplayMetadataUploaded(int userId, const char[] recordId, const char[] storageKey, const char[] fileName, int fileSize)
{
    char suffix[160];
    Format(suffix, sizeof(suffix), "/abnormal-records/%s/replay-metadata", recordId);

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
    payload.SetString("storage_key", storageKey);
    payload.SetString("file_name", fileName);
    payload.SetInt("file_size", fileSize);
    payload.SetString("content_type", "application/vnd.gokz.replay");

    request.Post(payload, OnReplayMetadataUploaded, userId);
}

public void OnReplayMetadataUploaded(HTTPResponse response, any value, const char[] error)
{
    int client = FindHeldClientByUserId(value);

    if (error[0] != '\0')
    {
        LogError("[cngokz-recordguard] Replay metadata update failed: %s", error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("[cngokz-recordguard] Replay metadata update returned HTTP status %d.", response.Status);
        return;
    }

    if (client > 0)
    {
        g_HeldReplayUploaded[client] = true;
        SaveReplayCacheForClient(client);
    }
}

bool GetR2UploadConfig(char[] url, int urlMaxLen, char[] apiKey, int keyMaxLen, bool &verifyCert)
{
    url[0] = '\0';
    apiKey[0] = '\0';
    verifyCert = false;
    bool usingLegacyConfig = false;

    if (g_RGR2UploadEnabled != null && !g_RGR2UploadEnabled.BoolValue)
    {
        return false;
    }

    if (g_RGLegacyR2UploadEnabled != null && !g_RGLegacyR2UploadEnabled.BoolValue)
    {
        return false;
    }

    if (g_RGLegacyR2UploadUrl != null)
    {
        g_RGLegacyR2UploadUrl.GetString(url, urlMaxLen);
        TrimString(url);
    }
    if (url[0] != '\0')
    {
        usingLegacyConfig = true;
    }
    else if (g_RGR2UploadUrl != null)
    {
        g_RGR2UploadUrl.GetString(url, urlMaxLen);
        TrimString(url);
    }

    if (g_RGLegacyR2UploadKey != null)
    {
        g_RGLegacyR2UploadKey.GetString(apiKey, keyMaxLen);
        TrimString(apiKey);
    }
    if (apiKey[0] == '\0')
    {
        usingLegacyConfig = false;
    }

    if (apiKey[0] == '\0' && g_RGR2UploadKey != null)
    {
        g_RGR2UploadKey.GetString(apiKey, keyMaxLen);
        TrimString(apiKey);
    }

    if (usingLegacyConfig && g_RGLegacyR2UploadVerifyCert != null)
    {
        verifyCert = g_RGLegacyR2UploadVerifyCert.BoolValue;
    }
    else if (g_RGR2UploadVerifyCert != null)
    {
        verifyCert = g_RGR2UploadVerifyCert.BoolValue;
    }

    return url[0] != '\0' && apiKey[0] != '\0';
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
