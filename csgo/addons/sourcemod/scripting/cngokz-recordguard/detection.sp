public int Native_ShouldHoldRecord(Handle plugin, int numParams)
{
    int client = GetNativeCell(1);
    int course = GetNativeCell(2);
    int mode = GetNativeCell(3);
    int timeType = GetNativeCell(4);
    float runTime = GetNativeCell(5);
    int teleportsUsed = GetNativeCell(6);
    int mapId = GetNativeCell(7);

    return ShouldHoldRecord(client, course, mode, timeType, runTime, teleportsUsed, mapId);
}

bool ShouldHoldRecord(int client, int course, int mode, int timeType, float runTime, int teleportsUsed, int mapId)
{
    if (g_RGEnabled == null || !g_RGEnabled.BoolValue)
    {
        return false;
    }
    if (!IsValidClient(client) || IsFakeClient(client))
    {
        return false;
    }

    char modeShort[16];
    GetModeShortName(mode, modeShort, sizeof(modeShort));

    char timeTypeName[8];
    GetTimeTypeName(timeType, timeTypeName, sizeof(timeTypeName));

    float threshold = 0.0;
    if (!FindMatchingThreshold(g_CurrentMapName, course, modeShort, timeTypeName, threshold))
    {
        return false;
    }

    if (runTime > threshold)
    {
        return false;
    }

    HoldAbnormalRecord(client, course, mode, timeType, runTime, teleportsUsed, mapId, modeShort, timeTypeName, threshold);
    return true;
}

bool FindMatchingThreshold(const char[] mapName, int course, const char[] mode, const char[] timeType, float &threshold)
{
    char normalizedMap[128];
    strcopy(normalizedMap, sizeof(normalizedMap), mapName);
    TrimString(normalizedMap);
    LowerString(normalizedMap);

    int bestScore = -1;
    threshold = 0.0;

    for (int i = 0; i < g_RuleCount; i++)
    {
        bool exactMap = StrEqual(g_RuleMap[i], normalizedMap, false);
        bool allMaps = StrEqual(g_RuleMap[i], "*", false);
        if (!exactMap && !allMaps)
        {
            continue;
        }

        // An exact map rule always wins over the all-maps default.
        int score = exactMap ? 8 : 0;
        if (g_RuleCourse[i] == course)
        {
            score += 4;
        }
        else if (g_RuleCourse[i] != 0)
        {
            continue;
        }

        if (g_RuleMode[i][0] != '\0')
        {
            if (!StrEqual(g_RuleMode[i], mode, false))
            {
                continue;
            }
            score += 2;
        }

        if (g_RuleTimeType[i][0] != '\0')
        {
            if (!StrEqual(g_RuleTimeType[i], timeType, false))
            {
                continue;
            }
            score += 1;
        }

        if (score > bestScore)
        {
            bestScore = score;
            threshold = g_RuleThreshold[i];
        }
    }

    return bestScore >= 0 && threshold > 0.0;
}

void GetModeShortName(int mode, char[] buffer, int maxLen)
{
    switch (mode)
    {
        case Mode_Vanilla:
        {
            strcopy(buffer, maxLen, "vnl");
        }
        case Mode_SimpleKZ:
        {
            strcopy(buffer, maxLen, "skz");
        }
        case Mode_KZTimer:
        {
            strcopy(buffer, maxLen, "kzt");
        }
        default:
        {
            strcopy(buffer, maxLen, "unknown");
        }
    }
}

void GetGlobalModeNameFromShort(const char[] modeShort, char[] buffer, int maxLen)
{
    if (StrEqual(modeShort, "vnl", false))
    {
        strcopy(buffer, maxLen, "kz_vanilla");
    }
    else if (StrEqual(modeShort, "skz", false))
    {
        strcopy(buffer, maxLen, "kz_simple");
    }
    else if (StrEqual(modeShort, "kzt", false))
    {
        strcopy(buffer, maxLen, "kz_timer");
    }
    else
    {
        strcopy(buffer, maxLen, modeShort);
    }
}

void GetTimeTypeName(int timeType, char[] buffer, int maxLen)
{
    if (timeType == TimeType_Pro)
    {
        strcopy(buffer, maxLen, "pro");
    }
    else
    {
        strcopy(buffer, maxLen, "tp");
    }
}
