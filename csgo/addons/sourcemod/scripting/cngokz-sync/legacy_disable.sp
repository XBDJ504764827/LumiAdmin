#define LEGACY_EDGE_SYNC_PLUGIN "addons/sourcemod/plugins/manger_edge_sync.smx"
#define LEGACY_EDGE_SYNC_DISABLED_DIR "addons/sourcemod/plugins/disabled"
#define LEGACY_EDGE_SYNC_DISABLED_PLUGIN "addons/sourcemod/plugins/disabled/manger_edge_sync.smx"

void UnloadLegacyEdgeSyncPlugin()
{
    ServerCommand("sm plugins unload manger_edge_sync");
    ServerExecute();
}

bool IsLegacyEdgeSyncSurfaceOccupied()
{
    return GetFeatureStatus(FeatureType_Native, "EdgeSync_EnqueueOperation") == FeatureStatus_Available
        || LibraryExists("manger_edge_sync")
        || FindPluginByFile("manger_edge_sync.smx") != INVALID_HANDLE
        || FileExists(LEGACY_EDGE_SYNC_PLUGIN);
}

bool DisableLegacyEdgeSyncBinary()
{
    UnloadLegacyEdgeSyncPlugin();

    if (!FileExists(LEGACY_EDGE_SYNC_PLUGIN))
    {
        return true;
    }

    if (!DirExists(LEGACY_EDGE_SYNC_DISABLED_DIR) && !CreateDirectory(LEGACY_EDGE_SYNC_DISABLED_DIR, 511))
    {
        LogError("[cngokz-sync] Failed to create %s", LEGACY_EDGE_SYNC_DISABLED_DIR);
        return false;
    }

    if (FileExists(LEGACY_EDGE_SYNC_DISABLED_PLUGIN) && !DeleteFile(LEGACY_EDGE_SYNC_DISABLED_PLUGIN))
    {
        LogError("[cngokz-sync] Failed to replace existing disabled manger_edge_sync.smx");
        return false;
    }

    if (RenameFile(LEGACY_EDGE_SYNC_DISABLED_PLUGIN, LEGACY_EDGE_SYNC_PLUGIN))
    {
        LogMessage("[cngokz-sync] Moved legacy manger_edge_sync.smx to plugins/disabled");
        return true;
    }

    LogError("[cngokz-sync] Failed to move legacy manger_edge_sync.smx to plugins/disabled");
    return false;
}
