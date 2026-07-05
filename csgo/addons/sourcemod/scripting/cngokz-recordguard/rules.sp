void SyncRules()
{
    if (g_RGEnabled == null || !g_RGEnabled.BoolValue)
    {
        return;
    }

    HTTPRequest request = CreateJsonRequest("/abnormal-record-rules");
    if (request == null)
    {
        return;
    }

    ApplyServerHeaders(request);
    request.Get(OnRulesSynced);
}

public void OnRulesSynced(HTTPResponse response, any value, const char[] error)
{
    if (error[0] != '\0')
    {
        LogError("[cngokz-recordguard] Rule sync failed: %s", error);
        return;
    }

    if (response.Status < HTTPStatus_OK || response.Status >= HTTPStatus_MultipleChoices)
    {
        LogError("[cngokz-recordguard] Rule sync returned HTTP status %d.", response.Status);
        return;
    }

    JSONObject root = view_as<JSONObject>(response.Data);
    if (root == null)
    {
        LogError("[cngokz-recordguard] Rule sync returned empty JSON.");
        return;
    }

    JSON rawItems = root.Get("items");
    if (rawItems == null)
    {
        g_RuleCount = 0;
        return;
    }

    JSONArray items = view_as<JSONArray>(rawItems);
    int count = items.Length;
    if (count > CNGOKZ_MAX_RULES)
    {
        count = CNGOKZ_MAX_RULES;
    }

    g_RuleCount = 0;
    for (int i = 0; i < count; i++)
    {
        JSONObject item = view_as<JSONObject>(items.Get(i));
        if (item == null)
        {
            continue;
        }

        char mapName[128];
        if (!item.GetString("map_name", mapName, sizeof(mapName)) || mapName[0] == '\0')
        {
            continue;
        }

        strcopy(g_RuleMap[g_RuleCount], sizeof(g_RuleMap[]), mapName);
        TrimString(g_RuleMap[g_RuleCount]);
        LowerString(g_RuleMap[g_RuleCount]);

        g_RuleCourse[g_RuleCount] = item.GetInt("course");
        g_RuleMode[g_RuleCount][0] = '\0';
        g_RuleTimeType[g_RuleCount][0] = '\0';

        if (!item.IsNull("mode"))
        {
            item.GetString("mode", g_RuleMode[g_RuleCount], sizeof(g_RuleMode[]));
            TrimString(g_RuleMode[g_RuleCount]);
            LowerString(g_RuleMode[g_RuleCount]);
        }
        if (!item.IsNull("time_type"))
        {
            item.GetString("time_type", g_RuleTimeType[g_RuleCount], sizeof(g_RuleTimeType[]));
            TrimString(g_RuleTimeType[g_RuleCount]);
            LowerString(g_RuleTimeType[g_RuleCount]);
        }

        g_RuleThreshold[g_RuleCount] = item.GetFloat("threshold_seconds");
        if (g_RuleThreshold[g_RuleCount] <= 0.0)
        {
            continue;
        }
        g_RuleCount++;
    }

    DebugLog("synced %d abnormal record rules", g_RuleCount);
}

stock void LowerString(char[] value)
{
    for (int i = 0; value[i] != '\0'; i++)
    {
        if (value[i] >= 'A' && value[i] <= 'Z')
        {
            value[i] = view_as<char>(value[i] + 32);
        }
    }
}
