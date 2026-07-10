#include <sourcemod>
#include <SteamWorks>
#include <ripext>
#include <GlobalAPI>
#include <gokz>
#include <gokz/core>
#include <gokz/global>
#include <gokz/replays>
#include <gokz/kzplayer>
#include <cngokz/core>
#include <cngokz/recordguard>

#pragma newdecls required
#pragma semicolon 1

#define CNGOKZ_RECORDGUARD_VERSION "0.1.0"
#define CNGOKZ_MAX_URL_LENGTH 512
#define CNGOKZ_MAX_TOKEN_LENGTH 256
#define CNGOKZ_MAX_RECORD_ID 64
#define CNGOKZ_MAX_RULES 512
#define CNGOKZ_MAX_IDEMPOTENCY_KEY 96
#define CNGOKZ_MAX_R2_KEY_LENGTH 256
#define CNGOKZ_RECORDGUARD_DB "cngokz_recordguard"
#define CNGOKZ_REPLAY_CACHE_DIR "data/cngokz-recordguard/abnormal"

native int MangerReporter_GetApiBaseUrl(char[] apiBaseUrl, int maxLen);
native int MangerReporter_GetReportToken(char[] token, int maxLen);
native int MangerReporter_GetServerPort();

ConVar g_RGEnabled = null;
ConVar g_RGRuleSyncInterval = null;
ConVar g_RGPollInterval = null;
ConVar g_RGRequestTimeout = null;
ConVar g_RGDebugLog = null;
ConVar g_RGTickrate = null;
ConVar g_RGR2UploadEnabled = null;

Handle g_RGRuleTimer = null;
Handle g_RGPollTimer = null;
Database g_RGDb = null;
bool g_RGR2SteamWorksOK = false;

int g_RuleCount = 0;
char g_RuleMap[CNGOKZ_MAX_RULES][128];
int g_RuleCourse[CNGOKZ_MAX_RULES];
char g_RuleMode[CNGOKZ_MAX_RULES][16];
char g_RuleTimeType[CNGOKZ_MAX_RULES][8];
float g_RuleThreshold[CNGOKZ_MAX_RULES];

bool g_HeldActive[MAXPLAYERS + 1];
int g_HeldUserId[MAXPLAYERS + 1];
int g_HeldCourse[MAXPLAYERS + 1];
int g_HeldMode[MAXPLAYERS + 1];
int g_HeldTimeType[MAXPLAYERS + 1];
int g_HeldTeleports[MAXPLAYERS + 1];
int g_HeldMapId[MAXPLAYERS + 1];
float g_HeldRunTime[MAXPLAYERS + 1];
float g_HeldThreshold[MAXPLAYERS + 1];
bool g_HeldRecordCreated[MAXPLAYERS + 1];
bool g_HeldReplayCopied[MAXPLAYERS + 1];
bool g_HeldReplayUploaded[MAXPLAYERS + 1];
bool g_HeldReplayUploadInFlight[MAXPLAYERS + 1];
char g_HeldRecordId[MAXPLAYERS + 1][CNGOKZ_MAX_RECORD_ID];
char g_HeldIdempotencyKey[MAXPLAYERS + 1][CNGOKZ_MAX_IDEMPOTENCY_KEY];
char g_HeldReplayPath[MAXPLAYERS + 1][PLATFORM_MAX_PATH];
char g_HeldMapName[MAXPLAYERS + 1][128];
char g_HeldSteamId64[MAXPLAYERS + 1][32];
char g_HeldSteamId2[MAXPLAYERS + 1][32];
char g_HeldPlayerName[MAXPLAYERS + 1][MAX_NAME_LENGTH];

#include "cngokz-recordguard/config.sp"
#include "cngokz-recordguard/rules.sp"
#include "cngokz-recordguard/detection.sp"
#include "cngokz-recordguard/r2_upload.sp"
#include "cngokz-recordguard/replay_capture.sp"
#include "cngokz-recordguard/pending_records.sp"
#include "cngokz-recordguard/global_submit.sp"

public Plugin myinfo =
{
    name = "CNGOKZ Record Guard",
    author = "XBDJ",
    description = "Holds abnormal GOKZ records for LumiAdmin review before global submission.",
    version = CNGOKZ_RECORDGUARD_VERSION,
    url = ""
};

public APLRes AskPluginLoad2(Handle myself, bool late, char[] error, int errMax)
{
    CreateNative("CNGOKZ_RecordGuard_ShouldHoldRecord", Native_ShouldHoldRecord);
    CreateNative("CNGOKZ_RecordGuard_IsHoldingClient", Native_IsHoldingClient);
    RegPluginLibrary("cngokz-recordguard");
    MarkNativeAsOptional("MangerReporter_GetApiBaseUrl");
    MarkNativeAsOptional("MangerReporter_GetReportToken");
    MarkNativeAsOptional("MangerReporter_GetServerPort");
    return APLRes_Success;
}

public void OnPluginStart()
{
    RecordGuard_OnPluginStart();
}

public void OnAllPluginsLoaded()
{
    RecordGuard_OnAllPluginsLoaded();
}

public void OnMapStart()
{
    GetCurrentMapDisplayName(g_CurrentMapName, sizeof(g_CurrentMapName));
    InitRecordGuardDb();
    EnsureReplayCacheDir();
    SyncRules();
    StartRuleSyncTimer();
    StartApprovedPollTimer();
}

public void OnMapEnd()
{
    StopRuleSyncTimer();
    StopApprovedPollTimer();
}

public void OnPluginEnd()
{
    StopRuleSyncTimer();
    StopApprovedPollTimer();
}

public void OnClientDisconnect(int client)
{
    ClearHeldRecord(client);
}
