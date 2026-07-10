#define CNGOKZ_WR_CACHE_DIR "data/cngokz-replay/wrcache"
#define CNGOKZ_WR_RUNS_DIR "data/gokz-replays/_runs"

bool g_CNGOKZWRSteamWorksOK = false;

void CNGOKZReplayR2_Init()
{
    g_CNGOKZWRSteamWorksOK = (GetExtensionFileStatus("SteamWorks.ext") > 0);
    if (!g_CNGOKZWRSteamWorksOK)
    {
        LogError("[cngokz-global] SteamWorks extension not loaded; WR replay R2 uploads are disabled.");
    }
}

void CNGOKZReplayR2_OnMapStart(const char[] map)
{
    if (map[0] != '\0')
    {
        CreateTimer(3.0, Timer_CNGOKZReplayR2BackfillMap, _, TIMER_FLAG_NO_MAPCHANGE);
    }
}

public Action Timer_CNGOKZReplayR2BackfillMap(Handle timer)
{
    CNGOKZReplayR2_BackfillExistingRecords(gC_CurrentMap);
    return Plugin_Stop;
}

void CNGOKZReplayR2_OnReplaySaved(int client, int replayType, const char[] map, int course, int timeType, const char[] filePath, bool tempReplay)
{
    if (!CNGOKZCore_IsReplayR2Enabled() || !CNGOKZCore_IsWRReplayR2Enabled())
    {
        return;
    }
    if (replayType != ReplayType_Run || tempReplay || filePath[0] == '\0')
    {
        return;
    }
    if (LibraryExists("cngokz-recordguard") && CNGOKZ_RecordGuard_IsHoldingClient(client))
    {
        CNGOKZReplayR2_Debug("skipping held abnormal replay for client %N", client);
        return;
    }

    int mode = GOKZ_GetCoreOption(client, Option_Mode);
    CNGOKZReplayR2_UploadWRWithShuffle(map, mode, course, timeType, filePath);
}

void CNGOKZReplayR2_UploadWRWithShuffle(const char[] map, int mode, int course, int timeType, const char[] newWRPath)
{
    if (!g_CNGOKZWRSteamWorksOK)
    {
        return;
    }

    char gokzMode[8];
    CNGOKZReplayR2_GetMode(gokzMode, sizeof(gokzMode), mode);
    char typeName[4];
    CNGOKZReplayR2_GetTimeType(typeName, sizeof(typeName), timeType);
    char cacheFile[PLATFORM_MAX_PATH];
    CNGOKZReplayR2_BuildCachePath(cacheFile, sizeof(cacheFile), map, course, mode, timeType);

    if (FileExists(cacheFile))
    {
        CNGOKZReplayR2_UploadFile(map, gokzMode, typeName, course, 1, cacheFile);
    }

    bool sent = CNGOKZReplayR2_UploadFile(map, gokzMode, typeName, course, 0, newWRPath);
    if (sent && FileExists(newWRPath))
    {
        CNGOKZReplayR2_EnsureCacheDir(map);
        CNGOKZReplayR2_CopyFile(newWRPath, cacheFile);
    }
}

void CNGOKZReplayR2_BackfillExistingRecords(const char[] map)
{
    if (!g_CNGOKZWRSteamWorksOK || !CNGOKZCore_IsReplayR2Enabled() || !CNGOKZCore_IsWRReplayR2Enabled() || map[0] == '\0')
    {
        return;
    }

    char dir[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, dir, sizeof(dir), "%s/%s", CNGOKZ_WR_RUNS_DIR, map);
    DirectoryListing listing = OpenDirectory(dir);
    if (listing == null)
    {
        return;
    }

    char fileName[PLATFORM_MAX_PATH];
    char fullPath[PLATFORM_MAX_PATH];
    FileType fileType;
    while (listing.GetNext(fileName, sizeof(fileName), fileType))
    {
        if (fileType != FileType_File || StrContains(fileName, ".replay", false) == -1)
        {
            continue;
        }

        int course;
        int mode;
        int timeType;
        if (!CNGOKZReplayR2_ParseRunFileName(fileName, course, mode, timeType))
        {
            continue;
        }

        char cacheFile[PLATFORM_MAX_PATH];
        CNGOKZReplayR2_BuildCachePath(cacheFile, sizeof(cacheFile), map, course, mode, timeType);
        if (FileExists(cacheFile))
        {
            continue;
        }

        BuildPath(Path_SM, fullPath, sizeof(fullPath), "%s/%s/%s", CNGOKZ_WR_RUNS_DIR, map, fileName);
        char gokzMode[8];
        CNGOKZReplayR2_GetMode(gokzMode, sizeof(gokzMode), mode);
        char typeName[4];
        CNGOKZReplayR2_GetTimeType(typeName, sizeof(typeName), timeType);
        if (CNGOKZReplayR2_UploadFile(map, gokzMode, typeName, course, 0, fullPath))
        {
            CNGOKZReplayR2_EnsureCacheDir(map);
            CNGOKZReplayR2_CopyFile(fullPath, cacheFile);
        }
    }
    delete listing;
}

bool CNGOKZReplayR2_ParseRunFileName(const char[] fileName, int &course, int &mode, int &timeType)
{
    char buffer[PLATFORM_MAX_PATH];
    strcopy(buffer, sizeof(buffer), fileName);
    int dot = StrContains(buffer, ".replay", false);
    if (dot == -1)
    {
        return false;
    }
    buffer[dot] = '\0';

    char parts[4][16];
    if (ExplodeString(buffer, "_", parts, sizeof(parts), sizeof(parts[])) < 4)
    {
        return false;
    }
    course = StringToInt(parts[0]);
    mode = CNGOKZReplayR2_FindMode(parts[1]);
    timeType = CNGOKZReplayR2_FindTimeType(parts[3]);
    return course >= 0 && mode != -1 && timeType != -1;
}

bool CNGOKZReplayR2_UploadFile(const char[] map, const char[] gokzMode, const char[] typeName, int course, int rank, const char[] filePath)
{
    char url[512];
    char apiKey[256];
    bool verifyCert = true;
    if (!CNGOKZCore_GetReplayR2Config(url, sizeof(url), apiKey, sizeof(apiKey), verifyCert) || !FileExists(filePath))
    {
        return false;
    }

    char route[32];
    Format(route, sizeof(route), "%s%d_%d", typeName, course, rank);
    // Preserve the historical main-course keys: tp0/pro0 and tp1/pro1.
    if (course == 0)
    {
        Format(route, sizeof(route), "%s%d", typeName, rank);
    }

    Handle request = SteamWorks_CreateHTTPRequest(k_EHTTPMethodPOST, url);
    if (request == null)
    {
        return false;
    }
    SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, 60);
    SteamWorks_SetHTTPRequestAbsoluteTimeoutMS(request, 60000);
    SteamWorks_SetHTTPRequestRequiresVerifiedCertificate(request, verifyCert);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-API-Key", apiKey);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-GOKZ-Mode", gokzMode);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-Map", map);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-Route", route);

    if (!SteamWorks_SetHTTPRequestRawPostBodyFromFile(request, "application/octet-stream", filePath))
    {
        delete request;
        return false;
    }
    SteamWorks_SetHTTPCallbacks(request, OnCNGOKZReplayR2UploadCompleted);
    if (!SteamWorks_SendHTTPRequest(request))
    {
        delete request;
        return false;
    }

    CNGOKZReplayR2_Debug("uploading WR -> wr/%s/%s/%s.replay", gokzMode, map, route);
    return true;
}

public void OnCNGOKZReplayR2UploadCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode)
{
    int status = view_as<int>(statusCode);
    if (failure || !requestSuccessful || status < 200 || status >= 300)
    {
        LogError("[cngokz-global] WR R2 upload failed. failure=%d successful=%d status=%d", failure ? 1 : 0, requestSuccessful ? 1 : 0, status);
    }
    delete request;
}

void CNGOKZReplayR2_GetMode(char[] buffer, int maxLen, int mode)
{
    if (mode < 0 || mode >= MODE_COUNT)
    {
        strcopy(buffer, maxLen, "unk");
        return;
    }
    strcopy(buffer, maxLen, gC_ModeNamesShort[mode]);
    for (int i = 0; buffer[i] != '\0'; i++)
    {
        if (buffer[i] >= 'A' && buffer[i] <= 'Z')
        {
            buffer[i] = view_as<char>(buffer[i] + 32);
        }
    }
}

void CNGOKZReplayR2_GetTimeType(char[] buffer, int maxLen, int timeType)
{
    strcopy(buffer, maxLen, timeType == TimeType_Pro ? "pro" : "tp");
}

int CNGOKZReplayR2_FindMode(const char[] shortName)
{
    for (int i = 0; i < MODE_COUNT; i++)
    {
        if (StrEqual(shortName, gC_ModeNamesShort[i], false))
        {
            return i;
        }
    }
    return -1;
}

int CNGOKZReplayR2_FindTimeType(const char[] name)
{
    if (StrEqual(name, "NUB", false)) return TimeType_Nub;
    if (StrEqual(name, "PRO", false)) return TimeType_Pro;
    return -1;
}

void CNGOKZReplayR2_BuildCachePath(char[] buffer, int maxLen, const char[] map, int course, int mode, int timeType)
{
    char gokzMode[8];
    CNGOKZReplayR2_GetMode(gokzMode, sizeof(gokzMode), mode);
    char typeName[4];
    CNGOKZReplayR2_GetTimeType(typeName, sizeof(typeName), timeType);
    BuildPath(Path_SM, buffer, maxLen, "%s/%s/%d_%s_%s.replay", CNGOKZ_WR_CACHE_DIR, map, course, gokzMode, typeName);
}

void CNGOKZReplayR2_EnsureCacheDir(const char[] map)
{
    char dir[PLATFORM_MAX_PATH];
    BuildPath(Path_SM, dir, sizeof(dir), "%s", CNGOKZ_WR_CACHE_DIR);
    if (!DirExists(dir)) CreateDirectory(dir, 511);
    BuildPath(Path_SM, dir, sizeof(dir), "%s/%s", CNGOKZ_WR_CACHE_DIR, map);
    if (!DirExists(dir)) CreateDirectory(dir, 511);
}

bool CNGOKZReplayR2_CopyFile(const char[] source, const char[] destination)
{
    File sourceFile = OpenFile(source, "rb");
    if (sourceFile == null) return false;
    File destinationFile = OpenFile(destination, "wb");
    if (destinationFile == null)
    {
        delete sourceFile;
        return false;
    }

    int[] buffer = new int[32];
    while (!IsEndOfFile(sourceFile))
    {
        int count = ReadFile(sourceFile, buffer, 32, 1);
        destinationFile.Write(buffer, count, 1);
    }
    delete sourceFile;
    delete destinationFile;
    return true;
}

void CNGOKZReplayR2_Debug(const char[] format, any ...)
{
    if (!CNGOKZCore_IsDebugEnabled()) return;
    char message[512];
    VFormat(message, sizeof(message), format, 2);
    LogMessage("[cngokz-global] %s", message);
}
