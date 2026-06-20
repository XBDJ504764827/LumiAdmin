#if defined _manger_shared_included
#endinput
#endif
#define _manger_shared_included

/**
 * 从配置行中提取引号内的值。
 * 行格式: key "value"
 * @param line    配置行文本
 * @param key     要匹配的键名
 * @param value   输出值缓冲区
 * @param maxLen  输出缓冲区最大长度
 * @return        成功返回 true
 */
stock bool ParseConfigValueLine(const char[] line, const char[] key, char[] value, int maxLen)
{
    value[0] = '\0';
    if (StrContains(line, key, false) != 0)
    {
        return false;
    }

    int firstQuote = FindCharInString(line, '"');
    if (firstQuote < 0)
    {
        return false;
    }

    int secondQuote = FindCharInString(line[firstQuote + 1], '"');
    if (secondQuote < 0)
    {
        return false;
    }
    secondQuote += firstQuote + 1;

    int copyLength = secondQuote - firstQuote - 1;
    if (copyLength >= maxLen)
    {
        copyLength = maxLen - 1;
    }

    for (int i = 0; i < copyLength; i++)
    {
        value[i] = line[firstQuote + 1 + i];
    }
    value[copyLength] = '\0';
    return true;
}

/**
 * 从配置行中提取引号内的值，并返回第二个引号之后的偏移量。
 * @param line        配置行文本
 * @param firstQuote  第一个引号的位置
 * @param value       输出值缓冲区
 * @param maxLen      输出缓冲区最大长度
 * @param nextOffset  输出：第二个引号之后的偏移量
 * @return            成功返回 true
 */
stock bool CopyQuotedValue(const char[] line, int firstQuote, char[] value, int maxLen, int &nextOffset)
{
    value[0] = '\0';

    int secondQuote = FindCharInString(line[firstQuote + 1], '"');
    if (secondQuote < 0)
    {
        return false;
    }
    secondQuote += firstQuote + 1;

    int copyLength = secondQuote - firstQuote - 1;
    if (copyLength >= maxLen)
    {
        copyLength = maxLen - 1;
    }

    for (int i = 0; i < copyLength; i++)
    {
        value[i] = line[firstQuote + 1 + i];
    }
    value[copyLength] = '\0';
    nextOffset = secondQuote + 1;
    return true;
}

/**
 * 解析 manger_server 配置行，提取端口和 token。
 * 行格式: manger_server "port" "token"
 * @param line       配置行文本
 * @param port       输出端口号
 * @param token      输出 token 缓冲区
 * @param tokenMaxLen token 缓冲区最大长度
 * @return           成功返回 true
 */
stock bool ParseServerMappingLine(const char[] line, int &port, char[] token, int tokenMaxLen)
{
    token[0] = '\0';
    port = 0;

    if (StrContains(line, "manger_server", false) != 0)
    {
        return false;
    }

    int firstQuote = FindCharInString(line, '"');
    if (firstQuote < 0)
    {
        return false;
    }

    char portText[16];
    int nextOffset = 0;
    if (!CopyQuotedValue(line, firstQuote, portText, sizeof(portText), nextOffset))
    {
        return false;
    }

    int tokenQuote = FindCharInString(line[nextOffset], '"');
    if (tokenQuote < 0)
    {
        return false;
    }
    tokenQuote += nextOffset;

    if (!CopyQuotedValue(line, tokenQuote, token, tokenMaxLen, nextOffset))
    {
        return false;
    }

    port = StringToInt(portText);
    TrimString(token);
    return port > 0;
}
