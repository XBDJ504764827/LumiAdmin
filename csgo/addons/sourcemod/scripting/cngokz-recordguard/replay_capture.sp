public Action GOKZ_RP_OnReplaySaved(int client, int replayType, const char[] map, int course, int timeType, float time, const char[] filePath, bool tempReplay)
{
    if (replayType != ReplayType_Run)
    {
        return Plugin_Continue;
    }
    if (!IsHeldReplayMatch(client, course, timeType, time))
    {
        return Plugin_Continue;
    }

    if (filePath[0] == '\0' || !FileExists(filePath))
    {
        LogError("[cngokz-recordguard] Held abnormal replay file missing for %N.", client);
        return Plugin_Continue;
    }

    EnsureReplayCacheDir();

    char destination[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, destination, sizeof(destination), "%s/%s.replay", CNGOKZ_REPLAY_CACHE_DIR, g_HeldIdempotencyKey[client]);
    if (!FileCopy(filePath, destination))
    {
        LogError("[cngokz-recordguard] Failed to copy abnormal replay: %s", filePath);
        return Plugin_Continue;
    }

    strcopy(g_HeldReplayPath[client], sizeof(g_HeldReplayPath[]), destination);
    g_HeldReplayCopied[client] = true;
    SaveReplayCacheForClient(client);
    TryUploadHeldReplay(client);
    return Plugin_Continue;
}

bool IsHeldReplayMatch(int client, int course, int timeType, float time)
{
    return IsValidClient(client)
        && g_HeldActive[client]
        && g_HeldUserId[client] == GetClientUserId(client)
        && g_HeldCourse[client] == course
        && g_HeldTimeType[client] == timeType
        && FloatAbs(g_HeldRunTime[client] - time) <= 0.001;
}

bool FileCopy(const char[] source, const char[] destination)
{
    File fileSource = OpenFile(source, "rb");
    if (fileSource == null)
    {
        return false;
    }

    File fileDestination = OpenFile(destination, "wb");
    if (fileDestination == null)
    {
        delete fileSource;
        return false;
    }

    int[] buffer = new int[32];
    int read = 0;
    while (!IsEndOfFile(fileSource))
    {
        read = ReadFile(fileSource, buffer, 32, 1);
        if (read > 0)
        {
            fileDestination.Write(buffer, read, 1);
        }
    }

    delete fileSource;
    delete fileDestination;
    return true;
}
