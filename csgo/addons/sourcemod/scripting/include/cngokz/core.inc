#if defined _cngokz_core_included
#endinput
#endif
#define _cngokz_core_included

native int CNGOKZCore_GetApiBaseUrl(char[] apiBaseUrl, int maxLen);
native int CNGOKZCore_GetReportToken(char[] token, int maxLen);
native int CNGOKZCore_GetServerPort();
native bool CNGOKZCore_IsDebugEnabled();

public SharedPlugin __pl_cngokz_core =
{
    name = "cngokz-core",
    file = "cngokz-core.smx",
#if defined REQUIRE_PLUGIN
    required = 1,
#else
    required = 0,
#endif
};

#if !defined REQUIRE_PLUGIN
public void __pl_cngokz_core_SetNTVOptional()
{
    MarkNativeAsOptional("CNGOKZCore_GetApiBaseUrl");
    MarkNativeAsOptional("CNGOKZCore_GetReportToken");
    MarkNativeAsOptional("CNGOKZCore_GetServerPort");
    MarkNativeAsOptional("CNGOKZCore_IsDebugEnabled");
}
#endif
