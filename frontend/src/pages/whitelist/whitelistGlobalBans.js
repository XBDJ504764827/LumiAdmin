// 全球封禁数据缓存、批量查询、解析和风险评估工具函数

const GLOBAL_BANS_SESSION_CACHE = new Map();

// 批量查询全球封禁记录（通过后端代理）
export async function fetchGlobalBansBatch(steamids) {
  const results = {};
  const missingSteamIds = [];

  for (const steamid of new Set(steamids.filter(Boolean))) {
    if (GLOBAL_BANS_SESSION_CACHE.has(steamid)) {
      results[steamid] = GLOBAL_BANS_SESSION_CACHE.get(steamid);
    } else {
      missingSteamIds.push(steamid);
    }
  }

  if (missingSteamIds.length === 0) return results;

  try {
    const response = await fetch('/api/public/global-bans/batch', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ steamids: missingSteamIds }),
    });
    if (!response.ok) return results;
    const data = await response.json();
    const fetchedResults = data.results || {};
    for (const [steamid, value] of Object.entries(fetchedResults)) {
      GLOBAL_BANS_SESSION_CACHE.set(steamid, value);
      results[steamid] = value;
    }
    return results;
  } catch {
    return results;
  }
}

// 解析封禁数据
// KZTimerGlobal（主 API）返回数组 [{...}, ...]
// GOKZ.TOP（备用 API）返回 { data: [...], count: N }
export function parseBanData(data) {
  if (Array.isArray(data) && data.length > 0) return data;
  if (Array.isArray(data.data) && data.data.length > 0) return data.data;
  return [];
}

// ---------------------------------------------------------------------------
// 全球封禁风险评估工具函数
// ---------------------------------------------------------------------------

function parseGlobalBanStats(stats) {
  if (!stats || typeof stats !== 'string') return {};
  const perfsMatch = stats.match(/Perfs:\s*(\d+)\s*\/\s*(\d+)/i);
  const averageMatch = stats.match(/Average:\s*(-?\d+(?:\.\d+)?)/i);
  return {
    perfs: perfsMatch ? Number(perfsMatch[1]) : null,
    perfTotal: perfsMatch ? Number(perfsMatch[2]) : null,
    average: averageMatch ? Number(averageMatch[1]) : null,
  };
}

function isPermanentGlobalBan(ban) {
  if (!ban?.expires_on) return true;
  const value = String(ban.expires_on);
  return value.startsWith('9999') || value.startsWith('2099');
}

function isActiveGlobalBan(ban) {
  if (!ban?.expires_on || isPermanentGlobalBan(ban)) return true;
  const expiresAt = new Date(ban.expires_on).getTime();
  return Number.isFinite(expiresAt) && expiresAt > Date.now();
}

export function inferGlobalBanRisk(bans) {
  if (!Array.isArray(bans) || bans.length === 0) return null;

  const typeScores = new Map();
  const reasons = [];
  let totalScore = 0;
  let strongestScore = 0;
  let permanentOrActiveCount = 0;
  let hackSignalCount = 0;
  let macroSignalCount = 0;
  let expiredFiniteCount = 0;

  for (const ban of bans) {
    const banType = String(ban?.ban_type ?? '').toLowerCase();
    const notes = String(ban?.notes ?? '').toLowerCase();
    const { perfs, perfTotal, average } = parseGlobalBanStats(ban?.stats);
    const hasPermanentOrActive = isActiveGlobalBan(ban);
    const perfsRatio = perfTotal ? perfs / perfTotal : 0;

    let recordType = banType || 'bhop异常';
    let recordScore = 0;
    const recordReasons = [];

    if (banType.includes('hack') || notes.includes("1's or 2's") || notes.includes('1s or 2s')) {
      recordType = banType || 'bhop_hack';
      recordScore += 5;
      hackSignalCount += 1;
      recordReasons.push(`命中 ${recordType} / 低滚轮模式特征`);
    }

    if (banType.includes('macro') || notes.includes('high scroll pattern')) {
      recordType = banType || 'bhop_macro';
      recordScore += 4;
      macroSignalCount += 1;
      recordReasons.push(`命中 ${recordType} / 高滚轮模式特征`);
    }

    if (average !== null && average <= 3) {
      recordType = banType || 'bhop_hack';
      recordScore += 3;
      hackSignalCount += 1;
      recordReasons.push(`滚轮平均值 ${average.toFixed(2)} 偏低`);
    }

    if (average !== null && average >= 14) {
      recordType = banType || 'bhop_macro';
      recordScore += 2;
      macroSignalCount += 1;
      recordReasons.push(`滚轮平均值 ${average.toFixed(2)} 偏高`);
    }

    if (perfsRatio >= 0.6) {
      recordScore += 2;
      recordReasons.push(`Perfs 命中 ${perfs}/${perfTotal}`);
    }

    if (hasPermanentOrActive) {
      recordScore += 3;
      permanentOrActiveCount += 1;
      recordReasons.push('封禁永久或仍未到期');
    } else if (ban?.expires_on) {
      expiredFiniteCount += 1;
    }

    if (recordScore > 0) {
      typeScores.set(recordType, (typeScores.get(recordType) ?? 0) + recordScore);
      totalScore += recordScore;
      strongestScore = Math.max(strongestScore, recordScore);
      reasons.push(...recordReasons.slice(0, 3));
    }
  }

  if (bans.length >= 2) {
    totalScore += 2;
    reasons.push(`存在 ${bans.length} 条全球封禁记录`);
  }

  const sortedTypes = [...typeScores.entries()].sort((a, b) => b[1] - a[1]);
  const label = sortedTypes[0]?.[0] ?? 'bhop异常';
  const allExpiredFinite = expiredFiniteCount === bans.length;
  const hasStrongSuspicion = totalScore >= 8 || strongestScore >= 7 || permanentOrActiveCount > 0 || hackSignalCount > 0 || macroSignalCount >= 2;

  if (hasStrongSuspicion) {
    return {
      tone: 'danger',
      label,
      title: `系统判断该玩家高度疑似 ${label}，请谨慎审核！`,
      reasons: [...new Set(reasons)].slice(0, 4),
    };
  }

  if (allExpiredFinite) {
    return {
      tone: 'warning',
      label: '误封嫌疑',
      title: '系统判断该玩家全球封禁存在误封嫌疑！',
      reasons: ['全球封禁均已到期', '未命中明确的永久封禁、hack 或重复宏特征', ...new Set(reasons)].slice(0, 4),
    };
  }

  return {
    tone: 'warning',
    label: '需要人工复核',
    title: '系统无法确认该全球封禁风险，请人工复核封禁详情。',
    reasons: [...new Set(reasons)].slice(0, 4),
  };
}
