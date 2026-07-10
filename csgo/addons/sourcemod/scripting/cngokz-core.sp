#include <sourcemod>
#include <manger_shared>
#include <cngokz/core>

#pragma newdecls required
#pragma semicolon 1

#define CNGOKZ_CORE_VERSION "0.1.0"
#define CNGOKZ_DEFAULT_API_BASE_URL "http://127.0.0.1:8080/api/plugin"
#define CNGOKZ_MAX_SERVER_TOKEN 256
#define CNGOKZ_CFG_FOLDER "sourcemod/cngokz-lumiadmin"

ConVar g_CNGOKZApiBaseUrl = null;
ConVar g_CNGOKZDebugLog = null;
ConVar g_CNGOKZHostPort = null;
ConVar g_CNGOKZReplayR2Enabled = null;
ConVar g_CNGOKZReplayR2WREnabled = null;
ConVar g_CNGOKZReplayR2Url = null;
ConVar g_CNGOKZReplayR2Key = null;
ConVar g_CNGOKZReplayR2VerifyCert = null;
StringMap g_CNGOKZServerTokenMap = null;
char g_CNGOKZCachedReportToken[CNGOKZ_MAX_SERVER_TOKEN];
int g_CNGOKZCachedReportPort = -1;
bool g_CNGOKZHasCachedReportToken = false;

#include "cngokz-core/config.sp"
#include "cngokz-core/natives.sp"

public Plugin myinfo =
{
    name = "CNGOKZ Core",
    author = "XBDJ",
    description = "Shared config and server identity for CNGOKZ plugins.",
    version = CNGOKZ_CORE_VERSION,
    url = ""
};

public APLRes AskPluginLoad2(Handle myself, bool late, char[] error, int errMax)
{
    CreateNative("CNGOKZCore_GetApiBaseUrl", Native_GetApiBaseUrl);
    CreateNative("CNGOKZCore_GetReportToken", Native_GetReportToken);
    CreateNative("CNGOKZCore_GetServerPort", Native_GetServerPort);
    CreateNative("CNGOKZCore_IsDebugEnabled", Native_IsDebugEnabled);
    CreateNative("CNGOKZCore_IsReplayR2Enabled", Native_IsReplayR2Enabled);
    CreateNative("CNGOKZCore_IsWRReplayR2Enabled", Native_IsWRReplayR2Enabled);
    CreateNative("CNGOKZCore_GetReplayR2Config", Native_GetReplayR2Config);
    RegPluginLibrary("cngokz-core");
    return APLRes_Success;
}

public void OnPluginStart()
{
    CNGOKZCore_OnPluginStart();
}

public void OnConfigsExecuted()
{
    CNGOKZCore_LoadLegacyConfigFallback();
}
