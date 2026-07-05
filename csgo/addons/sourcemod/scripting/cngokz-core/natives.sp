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
