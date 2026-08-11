import {
  formatNumber,
  formatPercentChange,
  percentChange,
} from '../../pages/dashboard/dashboardData.js';
import { IconShield, IconUsers, IconActivity, IconServer } from '../../shared/Icons.jsx';

/**
 * 顶部核心统计卡片（数据全部来自 /api/dashboard 真实统计）：
 * - 白名单用户（含本周新增）
 * - 今日新增白名单（较昨日变化）
 * - 今日活跃玩家（较昨日变化）
 * - 在线服务器（含在线玩家 / 服务器总数）
 */

function StatCard({ icon, iconClass, label, value, trend }) {
  return (
    <div className="dash-overview-item">
      <div className={`dash-overview-icon ${iconClass}`}>{icon}</div>
      <div className="dash-overview-info">
        <div className="dash-overview-value">{value}</div>
        <div className="dash-overview-label">
          {label}
          {trend ? <span className={`dash-trend dash-trend-${trend.tone}`}>{trend.text}</span> : null}
        </div>
      </div>
    </div>
  );
}

/** 今日 vs 昨日趋势徽章：无法计算（昨日为 0）时如实标注，不伪造百分比 */
function dayOverDayTrend(current, previous) {
  const pct = percentChange(current, previous);
  if (pct == null) {
    if (current > 0) return { text: '较昨日新增', tone: 'up' };
    return { text: '与昨日持平', tone: 'flat' };
  }
  if (pct === 0) return { text: '与昨日持平', tone: 'flat' };
  return { text: `较昨日 ${formatPercentChange(pct)}`, tone: pct > 0 ? 'up' : 'down' };
}

export function DashboardStats({ stats }) {
  const analytics = stats.analytics ?? {};
  const whitelistTotal = analytics.whitelist_total ?? 0;
  const whitelistWeeklyNew = analytics.whitelist_weekly_new ?? 0;
  const whitelistTodayNew = analytics.whitelist_today_new ?? 0;
  const whitelistYesterdayNew = analytics.whitelist_yesterday_new ?? 0;
  const playersTodayActive = analytics.players_today_active ?? 0;
  const playersYesterdayActive = analytics.players_yesterday_active ?? 0;

  return (
    <div className="dash-overview">
      <StatCard
        icon={<IconShield size={20} />}
        iconClass="dash-icon-shield"
        label="白名单用户"
        value={formatNumber(whitelistTotal)}
        trend={
          whitelistWeeklyNew > 0
            ? { text: `本周 +${formatNumber(whitelistWeeklyNew)}`, tone: 'up' }
            : { text: '本周无新增', tone: 'flat' }
        }
      />
      <StatCard
        icon={<IconUsers size={20} />}
        iconClass="dash-icon-userplus"
        label="今日新增白名单"
        value={formatNumber(whitelistTodayNew)}
        trend={dayOverDayTrend(whitelistTodayNew, whitelistYesterdayNew)}
      />
      <StatCard
        icon={<IconActivity size={20} />}
        iconClass="dash-icon-activity"
        label="今日活跃玩家"
        value={formatNumber(playersTodayActive)}
        trend={dayOverDayTrend(playersTodayActive, playersYesterdayActive)}
      />
      <StatCard
        icon={<IconServer size={20} />}
        iconClass="dash-icon-server"
        label="在线服务器"
        value={formatNumber(stats.online_servers ?? 0)}
        trend={{
          text: `玩家 ${formatNumber(stats.online_players ?? 0)} · 共 ${formatNumber(stats.total_servers ?? 0)} 台`,
          tone: 'flat',
        }}
      />
    </div>
  );
}
