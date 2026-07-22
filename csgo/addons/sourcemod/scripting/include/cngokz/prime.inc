#if defined _cngokz_prime_included
#endinput
#endif
#define _cngokz_prime_included

// CS Prime 许可 AppID（与 CS2KZ UserHasLicenseForApp 一致）
#define CNGOKZ_CS_PRIME_APPID 624820

/**
 * 查询在线玩家是否为 CS 优先账户。
 * 未知时返回 false；需要区分未知请用 CNGOKZPrime_GetStatus。
 */
native bool CNGOKZPrime_IsPrime(int client);

/**
 * 按 SteamID64 查询是否为 CS 优先账户（仅查缓存，不会主动向 Steam 查询）。
 */
native bool CNGOKZPrime_IsPrimeSteam64(const char[] steamId64);

/**
 * 获取玩家 Prime 状态。
 * @return 1=是, 0=否, -1=未知/未检测
 */
native int CNGOKZPrime_GetStatus(int client);

/**
 * 按 SteamID64 获取缓存中的 Prime 状态。
 * @return 1=是, 0=否, -1=未知/无缓存
 */
native int CNGOKZPrime_GetStatusSteam64(const char[] steamId64);

/**
 * 强制重新检测在线玩家（忽略未过期的“非 Prime”缓存；Prime=true 仍保留）。
 * @return true 表示检测成功完成（含确认非 Prime）
 */
native bool CNGOKZPrime_Recheck(int client);

/**
 * 玩家 Prime 状态确定后触发。
 * isPrime: true=优先账户
 * known: true=已确认；false=检测失败（NoAuth 等）
 */
forward void CNGOKZPrime_OnChecked(int client, bool isPrime, bool known);

public SharedPlugin __pl_cngokz_prime =
{
    name = "cngokz-prime",
    file = "cngokz-prime.smx",
#if defined REQUIRE_PLUGIN
    required = 1,
#else
    required = 0,
#endif
};

#if !defined REQUIRE_PLUGIN
public void __pl_cngokz_prime_SetNTVOptional()
{
    MarkNativeAsOptional("CNGOKZPrime_IsPrime");
    MarkNativeAsOptional("CNGOKZPrime_IsPrimeSteam64");
    MarkNativeAsOptional("CNGOKZPrime_GetStatus");
    MarkNativeAsOptional("CNGOKZPrime_GetStatusSteam64");
    MarkNativeAsOptional("CNGOKZPrime_Recheck");
}
#endif
