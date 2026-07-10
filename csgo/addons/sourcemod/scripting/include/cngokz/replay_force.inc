#if defined _cngokz_replay_force_included
#endinput
#endif
#define _cngokz_replay_force_included

native bool CNGOKZ_RP_ForceSaveRun(int client, int course, float time, int teleportsUsed);

public SharedPlugin __pl_cngokz_replay_force =
{
    name = "gokz-replays",
    file = "gokz-replays.smx",
    required = 1,
};
