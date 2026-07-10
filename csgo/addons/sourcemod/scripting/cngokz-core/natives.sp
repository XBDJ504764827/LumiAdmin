public int Native_GetApiBaseUrl(Handle plugin, int numParams)
{
    int maxLen = GetNativeCell(2);
    char apiBaseUrl[512];
    if (g_CNGOKZApiBaseUrl == null)
    {
        SetNativeString(1, "", maxLen);
        return 0;
    }

    g_CNGOKZApiBaseUrl.GetString(apiBaseUrl, sizeof(apiBaseUrl));
    TrimString(apiBaseUrl);
    SetNativeString(1, apiBaseUrl, maxLen);
    return apiBaseUrl[0] != '\0' ? 1 : 0;
}

public int Native_GetReportToken(Handle plugin, int numParams)
{
    int maxLen = GetNativeCell(2);
    char token[CNGOKZ_MAX_SERVER_TOKEN];
    if (!CNGOKZCore_GetCurrentReportToken(token, sizeof(token)))
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
    if (!CNGOKZCore_GetCurrentServerPort(port))
    {
        return 0;
    }
    return port;
}

public int Native_IsDebugEnabled(Handle plugin, int numParams)
{
    return g_CNGOKZDebugLog != null && g_CNGOKZDebugLog.BoolValue;
}

public int Native_IsReplayR2Enabled(Handle plugin, int numParams)
{
    return g_CNGOKZReplayR2Enabled != null && g_CNGOKZReplayR2Enabled.BoolValue;
}

public int Native_IsWRReplayR2Enabled(Handle plugin, int numParams)
{
    return g_CNGOKZReplayR2WREnabled != null && g_CNGOKZReplayR2WREnabled.BoolValue;
}

public int Native_GetReplayR2Config(Handle plugin, int numParams)
{
    int urlMaxLen = GetNativeCell(2);
    int keyMaxLen = GetNativeCell(4);
    char url[512];
    char apiKey[256];

    if (g_CNGOKZReplayR2Url != null)
    {
        g_CNGOKZReplayR2Url.GetString(url, sizeof(url));
        TrimString(url);
    }
    if (g_CNGOKZReplayR2Key != null)
    {
        g_CNGOKZReplayR2Key.GetString(apiKey, sizeof(apiKey));
        TrimString(apiKey);
    }

    SetNativeString(1, url, urlMaxLen);
    SetNativeString(3, apiKey, keyMaxLen);
    SetNativeCellRef(5, g_CNGOKZReplayR2VerifyCert != null && g_CNGOKZReplayR2VerifyCert.BoolValue);
    return url[0] != '\0' && apiKey[0] != '\0';
}
