#include <sourcemod>
#include <sdktools>
#include <SteamWorks>
#include <cngokz/prime>

#pragma newdecls required
#pragma semicolon 1

#define CNGOKZ_PRIME_VERSION "0.2.0"
#define CNGOKZ_PRIME_DB_NAME "cngokz_prime_cache"
#define CNGOKZ_PRIME_FALSE_TTL_SECS 21600  // 非 Prime 缓存 6 小时后重查
#define CNGOKZ_PRIME_TRUE_TTL_SECS 2592000 // Prime 缓存 30 天

Database g_PrimeDb = null;
// steamid64 -> "1"/"0"
StringMap g_PrimeMemory = null;
// steamid64 -> expires_at unix
StringMap g_PrimeExpires = null;
// client -> status: -1 unknown, 0 no, 1 yes
int g_ClientPrimeStatus[MAXPLAYERS + 1];

GlobalForward g_FwdPrimeChecked = null;

public Plugin myinfo =
{
    name = "CNGOKZ Prime",
    author = "XBDJ",
    description = "Detect and cache CS Prime (AppID 624820) for access control.",
    version = CNGOKZ_PRIME_VERSION,
    url = ""
};

public APLRes AskPluginLoad2(Handle myself, bool late, char[] error, int errMax)
{
    CreateNative("CNGOKZPrime_IsPrime", Native_IsPrime);
    CreateNative("CNGOKZPrime_IsPrimeSteam64", Native_IsPrimeSteam64);
    CreateNative("CNGOKZPrime_GetStatus", Native_GetStatus);
    CreateNative("CNGOKZPrime_GetStatusSteam64", Native_GetStatusSteam64);
    CreateNative("CNGOKZPrime_Recheck", Native_Recheck);
    g_FwdPrimeChecked = new GlobalForward("CNGOKZPrime_OnChecked", ET_Ignore, Param_Cell, Param_Cell, Param_Cell);
    RegPluginLibrary("cngokz-prime");
    return APLRes_Success;
}

public void OnPluginStart()
{
    g_PrimeMemory = new StringMap();
    g_PrimeExpires = new StringMap();

    for (int i = 0; i <= MaxClients; i++)
    {
        g_ClientPrimeStatus[i] = -1;
    }

    InitPrimeDatabase();
    RegAdminCmd("sm_cngokz_prime", CommandPrimeStatus, ADMFLAG_GENERIC, "sm_cngokz_prime [target] - recheck CS Prime status");
    PrintToServer("[cngokz-prime] Loaded with SteamWorks license and CS:GO player-resource checks.");
}

public void OnClientDisconnect(int client)
{
    g_ClientPrimeStatus[client] = -1;
}

public void OnClientAuthorized(int client, const char[] auth)
{
    if (client <= 0 || client > MaxClients || IsFakeClient(client))
    {
        return;
    }

    CheckClientPrime(client, false);
}

public int SteamWorks_OnValidateClient(int ownerAuthId, int authId)
{
    for (int client = 1; client <= MaxClients; client++)
    {
        if (!IsClientConnected(client) || IsFakeClient(client))
        {
            continue;
        }

        if (GetSteamAccountID(client, false) == authId)
        {
            CheckClientPrime(client, true);
            return 0;
        }
    }

    return 0;
}

// ─── Public helpers ───────────────────────────────────────────

bool CheckClientPrime(int client, bool force)
{
    if (client <= 0 || client > MaxClients || !IsClientConnected(client) || IsFakeClient(client))
    {
        return false;
    }

    char steamId64[32];
    if (!GetClientAuthId(client, AuthId_SteamID64, steamId64, sizeof(steamId64), true))
    {
        g_ClientPrimeStatus[client] = -1;
        FirePrimeChecked(client, false, false);
        return false;
    }

    int cached = GetCachedPrimeStatus(steamId64);
    if (!force && cached == 1)
    {
        g_ClientPrimeStatus[client] = 1;
        FirePrimeChecked(client, true, true);
        return true;
    }
    if (!force && cached == 0)
    {
        g_ClientPrimeStatus[client] = 0;
        FirePrimeChecked(client, false, true);
        return true;
    }

    if (HasGameReportedPrime(client))
    {
        SetPrimeCache(steamId64, true);
        g_ClientPrimeStatus[client] = 1;
        FirePrimeChecked(client, true, true);
        LogMessage("[cngokz-prime] Prime confirmed by CS:GO player resource for %s", steamId64);
        return true;
    }

    if (GetExtensionFileStatus("SteamWorks.ext") <= 0 || !SteamWorks_IsConnected())
    {
        g_ClientPrimeStatus[client] = -1;
        FirePrimeChecked(client, false, false);
        LogError("[cngokz-prime] SteamWorks unavailable; cannot check prime for %s", steamId64);
        return false;
    }

    EUserHasLicenseForAppResult result = SteamWorks_HasLicenseForApp(client, CNGOKZ_CS_PRIME_APPID);
    if (result == k_EUserHasLicenseResultHasLicense)
    {
        SetPrimeCache(steamId64, true);
        g_ClientPrimeStatus[client] = 1;
        FirePrimeChecked(client, true, true);
        LogMessage("[cngokz-prime] Prime confirmed by SteamWorks for %s", steamId64);
        return true;
    }

    if (result == k_EUserHasLicenseResultDoesNotHaveLicense)
    {
        SetPrimeCache(steamId64, false);
        g_ClientPrimeStatus[client] = 0;
        FirePrimeChecked(client, false, true);
        LogMessage("[cngokz-prime] SteamWorks confirmed non-Prime for %s", steamId64);
        return true;
    }

    // NoAuth 或其它：不写入非 Prime 缓存，保持未知
    g_ClientPrimeStatus[client] = -1;
    FirePrimeChecked(client, false, false);
    LogMessage("[cngokz-prime] SteamWorks returned NoAuth for %s; status remains unknown", steamId64);
    return false;
}

bool HasGameReportedPrime(int client)
{
    if (!IsClientInGame(client))
    {
        return false;
    }

    int playerResource = GetPlayerResourceEntity();
    if (playerResource == -1 || !HasEntProp(playerResource, Prop_Send, "m_bHasPrime"))
    {
        return false;
    }

    return GetEntProp(playerResource, Prop_Send, "m_bHasPrime", 1, client) != 0;
}

public Action CommandPrimeStatus(int client, int args)
{
    int target = client;
    if (args >= 1)
    {
        char targetArg[64];
        GetCmdArg(1, targetArg, sizeof(targetArg));
        target = FindTarget(client, targetArg, true, false);
    }

    if (target <= 0 || target > MaxClients || !IsClientConnected(target) || IsFakeClient(target))
    {
        ReplyToCommand(client, "[cngokz-prime] A connected human target is required.");
        return Plugin_Handled;
    }

    CheckClientPrime(target, true);
    int status = g_ClientPrimeStatus[target];
    char steamId64[32];
    GetClientAuthId(target, AuthId_SteamID64, steamId64, sizeof(steamId64), true);
    ReplyToCommand(client, "[cngokz-prime] %N (%s): status=%d (1=Prime, 0=non-Prime, -1=unknown)", target, steamId64, status);
    return Plugin_Handled;
}

// ─── Cache ────────────────────────────────────────────────────

int GetCachedPrimeStatus(const char[] steamId64)
{
    char value[8];
    int expiresAt = 0;

    if (g_PrimeMemory.GetString(steamId64, value, sizeof(value)))
    {
        g_PrimeExpires.GetValue(steamId64, expiresAt);
        if (expiresAt > GetTime())
        {
            return StringToInt(value);
        }
        g_PrimeMemory.Remove(steamId64);
        g_PrimeExpires.Remove(steamId64);
    }

    if (g_PrimeDb == null)
    {
        return -1;
    }

    char escaped[64];
    char query[256];
    SQL_EscapeString(g_PrimeDb, steamId64, escaped, sizeof(escaped));
    Format(query, sizeof(query),
        "SELECT is_prime, expires_at FROM prime_cache WHERE steam_id64 = '%s' LIMIT 1",
        escaped);

    DBResultSet results = SQL_Query(g_PrimeDb, query);
    if (results == null)
    {
        return -1;
    }

    int status = -1;
    if (SQL_FetchRow(results))
    {
        int isPrime = SQL_FetchInt(results, 0);
        expiresAt = SQL_FetchInt(results, 1);
        if (expiresAt > GetTime())
        {
            status = isPrime ? 1 : 0;
            value[0] = '\0';
            IntToString(status, value, sizeof(value));
            g_PrimeMemory.SetString(steamId64, value);
            g_PrimeExpires.SetValue(steamId64, expiresAt);
        }
        else
        {
            char delQuery[192];
            Format(delQuery, sizeof(delQuery), "DELETE FROM prime_cache WHERE steam_id64 = '%s'", escaped);
            SQL_FastQuery(g_PrimeDb, delQuery);
        }
    }
    delete results;
    return status;
}

void SetPrimeCache(const char[] steamId64, bool isPrime)
{
    int now = GetTime();
    int ttl = isPrime ? CNGOKZ_PRIME_TRUE_TTL_SECS : CNGOKZ_PRIME_FALSE_TTL_SECS;
    int expiresAt = now + ttl;

    char value[8];
    IntToString(isPrime ? 1 : 0, value, sizeof(value));
    g_PrimeMemory.SetString(steamId64, value);
    g_PrimeExpires.SetValue(steamId64, expiresAt);

    if (g_PrimeDb == null)
    {
        return;
    }

    char escaped[64];
    char query[320];
    SQL_EscapeString(g_PrimeDb, steamId64, escaped, sizeof(escaped));
    Format(query, sizeof(query),
        "INSERT OR REPLACE INTO prime_cache (steam_id64, is_prime, checked_at, expires_at) VALUES ('%s', %d, %d, %d)",
        escaped, isPrime ? 1 : 0, now, expiresAt);
    if (!SQL_FastQuery(g_PrimeDb, query))
    {
        LogError("[cngokz-prime] failed to write prime cache for %s", steamId64);
    }
}

void InitPrimeDatabase()
{
    char error[256];
    g_PrimeDb = SQLite_UseDatabase(CNGOKZ_PRIME_DB_NAME, error, sizeof(error));
    if (g_PrimeDb == null)
    {
        LogError("[cngokz-prime] SQLite open failed: %s", error);
        return;
    }

    SQL_FastQuery(g_PrimeDb, "CREATE TABLE IF NOT EXISTS prime_cache (steam_id64 TEXT PRIMARY KEY NOT NULL, is_prime INTEGER NOT NULL, checked_at INTEGER NOT NULL, expires_at INTEGER NOT NULL)");
    SQL_FastQuery(g_PrimeDb, "CREATE INDEX IF NOT EXISTS idx_prime_expires ON prime_cache(expires_at)");
}

void FirePrimeChecked(int client, bool isPrime, bool known)
{
    Call_StartForward(g_FwdPrimeChecked);
    Call_PushCell(client);
    Call_PushCell(isPrime);
    Call_PushCell(known);
    Call_Finish();
}

// ─── Natives ──────────────────────────────────────────────────

public int Native_IsPrime(Handle plugin, int numParams)
{
    int client = GetNativeCell(1);
    if (client <= 0 || client > MaxClients)
    {
        return 0;
    }
    return g_ClientPrimeStatus[client] == 1 ? 1 : 0;
}

public int Native_IsPrimeSteam64(Handle plugin, int numParams)
{
    char steamId64[32];
    GetNativeString(1, steamId64, sizeof(steamId64));
    return GetCachedPrimeStatus(steamId64) == 1 ? 1 : 0;
}

public int Native_GetStatus(Handle plugin, int numParams)
{
    int client = GetNativeCell(1);
    if (client <= 0 || client > MaxClients)
    {
        return -1;
    }
    return g_ClientPrimeStatus[client];
}

public int Native_GetStatusSteam64(Handle plugin, int numParams)
{
    char steamId64[32];
    GetNativeString(1, steamId64, sizeof(steamId64));
    return GetCachedPrimeStatus(steamId64);
}

public int Native_Recheck(Handle plugin, int numParams)
{
    int client = GetNativeCell(1);
    return CheckClientPrime(client, true) ? 1 : 0;
}
