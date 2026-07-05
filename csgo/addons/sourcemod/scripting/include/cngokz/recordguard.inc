#if defined _cngokz_recordguard_included
#endinput
#endif
#define _cngokz_recordguard_included

native bool CNGOKZ_RecordGuard_ShouldHoldRecord(int client, int course, int mode, int timeType, float runTime, int teleportsUsed, int mapId);

public SharedPlugin __pl_cngokz_recordguard =
{
    name = "cngokz-recordguard",
    file = "cngokz-recordguard.smx",
#if defined REQUIRE_PLUGIN
    required = 1,
#else
    required = 0,
#endif
};

#if !defined REQUIRE_PLUGIN
public void __pl_cngokz_recordguard_SetNTVOptional()
{
    MarkNativeAsOptional("CNGOKZ_RecordGuard_ShouldHoldRecord");
}
#endif
