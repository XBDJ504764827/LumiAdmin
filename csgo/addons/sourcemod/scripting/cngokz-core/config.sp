void CNGOKZCore_OnPluginStart()
{
    g_CNGOKZApiBaseUrl = FindConVar("cngokz_api_base_url");
    if (g_CNGOKZApiBaseUrl == null)
    {
        g_CNGOKZApiBaseUrl = CreateConVar("cngokz_api_base_url", CNGOKZ_DEFAULT_API_BASE_URL, "CNGOKZ plugin API base URL.");
    }

    g_CNGOKZDebugLog = CreateConVar("cngokz_debug_log", "0", "Enable verbose CNGOKZ plugin debug logs.", _, true, 0.0, true, 1.0);
    g_CNGOKZHostPort = FindConVar("hostport");

    RegServerCmd("cngokz_server", CNGOKZCore_CommandServerMapping, "Register one CNGOKZ server mapping.");
    RegServerCmd("manger_server", CNGOKZCore_CommandServerMapping, "Register one legacy Manger server mapping.");

    if (g_CNGOKZServerTokenMap == null)
    {
        g_CNGOKZServerTokenMap = new StringMap();
    }

    CNGOKZCore_ResetServerTokenMappings();
    AutoExecConfig(true, "cngokz-core", CNGOKZ_CFG_FOLDER);
}

Action CNGOKZCore_CommandServerMapping(int args)
{
    if (args < 2)
    {
        return Plugin_Handled;
    }

    char portText[64];
    char token[CNGOKZ_MAX_SERVER_TOKEN];
    GetCmdArg(1, portText, sizeof(portText));
    GetCmdArg(2, token, sizeof(token));
    TrimString(token);

    int port = StringToInt(portText);
    if (port <= 0 || token[0] == '\0')
    {
        return Plugin_Handled;
    }

    char portKey[16];
    IntToString(port, portKey, sizeof(portKey));
    g_CNGOKZServerTokenMap.SetString(portKey, token);
    CNGOKZCore_InvalidateConfigCache();
    return Plugin_Handled;
}

void CNGOKZCore_ResetServerTokenMappings()
{
    if (g_CNGOKZServerTokenMap != null)
    {
        g_CNGOKZServerTokenMap.Clear();
    }
    CNGOKZCore_InvalidateConfigCache();
}

void CNGOKZCore_InvalidateConfigCache()
{
    g_CNGOKZCachedReportToken[0] = '\0';
    g_CNGOKZCachedReportPort = -1;
    g_CNGOKZHasCachedReportToken = false;
}

bool CNGOKZCore_GetCurrentServerPort(int &port)
{
    if (g_CNGOKZHostPort == null)
    {
        g_CNGOKZHostPort = FindConVar("hostport");
    }
    if (g_CNGOKZHostPort == null)
    {
        return false;
    }
    port = g_CNGOKZHostPort.IntValue;
    return port > 0;
}

bool CNGOKZCore_GetCurrentReportToken(char[] token, int maxLen)
{
    token[0] = '\0';

    int currentPort = 0;
    if (!CNGOKZCore_GetCurrentServerPort(currentPort))
    {
        return false;
    }

    if (g_CNGOKZHasCachedReportToken && g_CNGOKZCachedReportPort == currentPort)
    {
        strcopy(token, maxLen, g_CNGOKZCachedReportToken);
        return token[0] != '\0';
    }

    char portKey[16];
    IntToString(currentPort, portKey, sizeof(portKey));
    if (g_CNGOKZServerTokenMap == null || !g_CNGOKZServerTokenMap.GetString(portKey, token, maxLen))
    {
        return false;
    }

    strcopy(g_CNGOKZCachedReportToken, sizeof(g_CNGOKZCachedReportToken), token);
    g_CNGOKZCachedReportPort = currentPort;
    g_CNGOKZHasCachedReportToken = true;
    return token[0] != '\0';
}

void CNGOKZCore_LoadLegacyConfigFallback()
{
    char path[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, path, sizeof(path), "../../cfg/sourcemod/manger_online_reporter.cfg");

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
        if (ParseConfigValueLine(line, "manger_api_base_url", value, sizeof(value)))
        {
            char currentUrl[512];
            g_CNGOKZApiBaseUrl.GetString(currentUrl, sizeof(currentUrl));
            TrimString(currentUrl);
            if (StrEqual(currentUrl, CNGOKZ_DEFAULT_API_BASE_URL, false) && value[0] != '\0')
            {
                g_CNGOKZApiBaseUrl.SetString(value);
            }
            continue;
        }

        int port = 0;
        char token[CNGOKZ_MAX_SERVER_TOKEN];
        if (ParseServerMappingLine(line, port, token, sizeof(token)))
        {
            char portKey[16];
            IntToString(port, portKey, sizeof(portKey));
            if (g_CNGOKZServerTokenMap != null)
            {
                g_CNGOKZServerTokenMap.SetString(portKey, token);
            }
        }
    }

    delete file;
    CNGOKZCore_InvalidateConfigCache();
}
