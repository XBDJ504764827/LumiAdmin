#define LEGACY_SERVER_PLUGIN "addons/sourcemod/plugins/manger_online_reporter.smx"
#define LEGACY_SERVER_DISABLED_DIR "addons/sourcemod/plugins/disabled"
#define LEGACY_SERVER_DISABLED_PLUGIN "addons/sourcemod/plugins/disabled/manger_online_reporter.smx"

void UnloadLegacyServerPlugin()
{
    ServerCommand("sm plugins unload manger_online_reporter");
    ServerExecute();
}

bool IsLegacyServerSurfaceOccupied()
{
    return GetFeatureStatus(FeatureType_Native, "MangerReporter_GetApiBaseUrl") == FeatureStatus_Available
        || LibraryExists("manger_online_reporter")
        || FindPluginByFile("manger_online_reporter.smx") != INVALID_HANDLE
        || FileExists(LEGACY_SERVER_PLUGIN);
}

bool DisableLegacyServerBinary()
{
    UnloadLegacyServerPlugin();

    if (!FileExists(LEGACY_SERVER_PLUGIN))
    {
        return true;
    }

    if (!DirExists(LEGACY_SERVER_DISABLED_DIR) && !CreateDirectory(LEGACY_SERVER_DISABLED_DIR, 511))
    {
        LogError("[cngokz-server] Failed to create %s", LEGACY_SERVER_DISABLED_DIR);
        return false;
    }

    if (FileExists(LEGACY_SERVER_DISABLED_PLUGIN) && !DeleteFile(LEGACY_SERVER_DISABLED_PLUGIN))
    {
        LogError("[cngokz-server] Failed to replace existing disabled manger_online_reporter.smx");
        return false;
    }

    if (RenameFile(LEGACY_SERVER_DISABLED_PLUGIN, LEGACY_SERVER_PLUGIN))
    {
        LogMessage("[cngokz-server] Moved legacy manger_online_reporter.smx to plugins/disabled");
        return true;
    }

    LogError("[cngokz-server] Failed to move legacy manger_online_reporter.smx to plugins/disabled");
    return false;
}
