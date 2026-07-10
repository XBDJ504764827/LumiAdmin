public Action GOKZ_RP_OnReplaySaved(int client, int replayType, const char[] map, int course, int timeType, float time, const char[] filePath, bool tempReplay)
{
    if (replayType != ReplayType_Run)
    {
        return Plugin_Continue;
    }
    CacheRecentReplay(client, course, timeType, time, filePath);

    if (!IsHeldReplayMatch(client, course, timeType, time))
    {
        if (client >= 1 && client <= MaxClients && g_HeldActive[client])
        {
            RecordGuardConsole(
                "GOKZ replay did not match held run: player=%N actual(course=%d type=%d time=%.3f) expected(course=%d type=%d time=%.3f)",
                client,
                course,
                timeType,
                time,
                g_HeldCourse[client],
                g_HeldTimeType[client],
                g_HeldRunTime[client]);
        }
        return Plugin_Continue;
    }

    CaptureHeldReplay(client, filePath, tempReplay);
    return Plugin_Continue;
}

public void GOKZ_RP_OnTimerEnd_Post(int client, const char[] filePath, int course, float time, int teleportsUsed)
{
    int timeType = GOKZ_GetTimeTypeEx(teleportsUsed);
    RecordGuardConsole(
        "GOKZ replay processing finished: player=%N course=%d type=%d time=%.3f file=%s held=%d",
        client,
        course,
        timeType,
        time,
        filePath[0] != '\0' ? filePath : "<empty>",
        g_HeldActive[client] ? 1 : 0);

    if (filePath[0] == '\0')
    {
        if (g_HeldActive[client])
        {
            RecordGuardConsole("GOKZ Replays did not write a file for the held abnormal run; it was likely disabled after missing PB/server-record eligibility");
        }
        return;
    }

    CacheRecentReplay(client, course, timeType, time, filePath);
    if (IsHeldReplayMatch(client, course, timeType, time))
    {
        CaptureHeldReplay(client, filePath, false);
    }
}

void CacheRecentReplay(int client, int course, int timeType, float time, const char[] filePath)
{
    if (!IsValidClient(client) || filePath[0] == '\0')
    {
        return;
    }
    g_RecentReplayValid[client] = true;
    g_RecentReplayUserId[client] = GetClientUserId(client);
    g_RecentReplayCourse[client] = course;
    g_RecentReplayTimeType[client] = timeType;
    g_RecentReplayTime[client] = time;
    strcopy(g_RecentReplayPath[client], sizeof(g_RecentReplayPath[]), filePath);
    RecordGuardConsole("GOKZ run replay observed: player=%N course=%d type=%d time=%.3f file=%s held=%d", client, course, timeType, time, filePath, g_HeldActive[client] ? 1 : 0);
}

void TryCaptureRecentReplay(int client)
{
    if (client < 1 || client > MaxClients || !g_RecentReplayValid[client])
    {
        RecordGuardConsole("no recent GOKZ replay available yet for held player=%N; waiting for replay callback", client);
        return;
    }
    if (g_RecentReplayUserId[client] != GetClientUserId(client)
        || g_RecentReplayCourse[client] != g_HeldCourse[client]
        || g_RecentReplayTimeType[client] != g_HeldTimeType[client]
        || FloatAbs(g_RecentReplayTime[client] - g_HeldRunTime[client]) > 0.01)
    {
        RecordGuardConsole(
            "recent replay does not match held run: player=%N recent(course=%d type=%d time=%.3f) held(course=%d type=%d time=%.3f)",
            client,
            g_RecentReplayCourse[client],
            g_RecentReplayTimeType[client],
            g_RecentReplayTime[client],
            g_HeldCourse[client],
            g_HeldTimeType[client],
            g_HeldRunTime[client]);
        return;
    }
    RecordGuardConsole("using replay that arrived before abnormal detection: player=%N file=%s", client, g_RecentReplayPath[client]);
    CaptureHeldReplay(client, g_RecentReplayPath[client], false);
}

void CaptureHeldReplay(int client, const char[] filePath, bool tempReplay)
{
    if (g_HeldReplayCopied[client])
    {
        return;
    }

    if (filePath[0] == '\0' || !FileExists(filePath))
    {
        LogError("[cngokz-recordguard] Held abnormal replay file missing for %N.", client);
        RecordGuardConsole("matched abnormal replay is missing: player=%N source=%s", client, filePath);
        return;
    }

    RecordGuardConsole("matched GOKZ replay: player=%N source=%s temp=%d", client, filePath, tempReplay ? 1 : 0);

    EnsureReplayCacheDir();

    char destination[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, destination, sizeof(destination), "%s/%s.replay", CNGOKZ_REPLAY_CACHE_DIR, g_HeldIdempotencyKey[client]);
    if (!FileCopy(filePath, destination))
    {
        LogError("[cngokz-recordguard] Failed to copy abnormal replay: %s", filePath);
        RecordGuardConsole("failed to copy abnormal replay: source=%s destination=%s", filePath, destination);
        return;
    }

    strcopy(g_HeldReplayPath[client], sizeof(g_HeldReplayPath[]), destination);
    g_HeldReplayCopied[client] = true;
    ClearRecentReplay(client);
    RecordGuardConsole("abnormal replay copied: player=%N destination=%s record_created=%d", client, destination, g_HeldRecordCreated[client] ? 1 : 0);
    SaveReplayCacheForClient(client);
    TryUploadHeldReplay(client);
}

bool IsHeldReplayMatch(int client, int course, int timeType, float time)
{
    return IsValidClient(client)
        && g_HeldActive[client]
        && g_HeldUserId[client] == GetClientUserId(client)
        && g_HeldCourse[client] == course
        && g_HeldTimeType[client] == timeType
        && FloatAbs(g_HeldRunTime[client] - time) <= 0.01;
}

void ClearRecentReplay(int client)
{
    if (client < 1 || client > MaxClients)
    {
        return;
    }
    g_RecentReplayValid[client] = false;
    g_RecentReplayUserId[client] = 0;
    g_RecentReplayCourse[client] = 0;
    g_RecentReplayTimeType[client] = 0;
    g_RecentReplayTime[client] = 0.0;
    g_RecentReplayPath[client][0] = '\0';
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
