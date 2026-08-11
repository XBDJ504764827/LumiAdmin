import { useAuth } from '../../state/store.js';
import { api } from '../../lib/api.js';
import { publicApi } from '../../lib/publicApi.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { normalizeAdminPreviewRows } from './dashboardData.js';
import { formatChinaToday, getChinaHour } from '../../shared/time.js';
import { DashboardStats } from '../../components/dashboard/DashboardStats.jsx';
import { WhitelistTrendChart } from '../../components/dashboard/WhitelistTrendChart.jsx';
import { ServerActivityChart } from '../../components/dashboard/ServerActivityChart.jsx';
import { ServerStatusChart } from '../../components/dashboard/ServerStatusChart.jsx';
import { ServerRankingChart } from '../../components/dashboard/ServerRankingChart.jsx';

function formatToday() {
  return formatChinaToday();
}

function greeting() {
  const h = getChinaHour();
  if (h < 6) return '夜深了';
  if (h < 12) return '早上好';
  if (h < 14) return '中午好';
  if (h < 18) return '下午好';
  return '晚上好';
}

function formatFps(fps) {
  if (!fps || fps === 0) return '--';
  return Math.round(fps);
}

function formatCpu(cpu) {
  if (!cpu || cpu === 0) return '--';
  return `${Math.round(cpu)}%`;
}

function formatTickrate(tickrate) {
  if (!tickrate || tickrate === 0) return '--';
  return Math.round(tickrate);
}

function formatPlayerRatio(current, max) {
  if (!current && !max) return '0 / 0';
  return `${current ?? 0} / ${max ?? 0}`;
}

function cpuColor(cpu) {
  if (!cpu) return 'var(--text3)';
  if (cpu < 50) return 'var(--teal)';
  if (cpu < 80) return 'var(--warning-text)';
  return 'var(--danger-text)';
}

export function DashboardPage() {
  const { session } = useAuth();
  const _token = session?.token ?? null;
  
  const metrics = useApiQuery(
    ['dashboard'],
    (token) => api.dashboard(token),
    { refetchInterval: 30_000 } // 每30秒自动刷新
  );
  
  const publicWhitelist = useApiQuery(
    ['publicWhitelist'],
    () => publicApi.publicWhitelist(),
    { auth: false }
  );

  if (metrics.isLoading) {
    return (
      <div id="dashboard" className="content-section active">
        <div className="breadcrumb"><span>首页</span><span className="sep">›</span><span className="current">仪表盘</span></div>
        <div className="dash-hero">
          <div className="dash-hero-text">
            <div className="dash-hero-title">{greeting()}，{session?.displayName ?? 'Alex'}</div>
            <div className="dash-hero-sub">{formatToday()} — 服务器状态与管理概览</div>
          </div>
        </div>
        <div className="card">
          <div className="card-body" style={{ textAlign: 'center', padding: 40 }}>
            <div className="loading-state"><div className="loading-spinner" />正在加载仪表盘数据...</div>
          </div>
        </div>
      </div>
    );
  }

  if (metrics.error) {
    return (
      <div id="dashboard" className="content-section active">
        <div className="breadcrumb"><span>首页</span><span className="sep">›</span><span className="current">仪表盘</span></div>
        <div className="dash-hero">
          <div className="dash-hero-text">
            <div className="dash-hero-title">{greeting()}，{session?.displayName ?? 'Alex'}</div>
            <div className="dash-hero-sub">{formatToday()}</div>
          </div>
        </div>
        <div className="card">
          <div className="card-body" style={{ textAlign: 'center', padding: 40 }}>
            <div style={{ color: 'var(--accent)', marginBottom: 16 }}>{metrics.error.message || '加载数据失败'}</div>
            <button className="btn btn-outline" onClick={() => metrics.refetch()}>重新加载</button>
          </div>
        </div>
      </div>
    );
  }

  const stats = metrics.data?.data ?? { total_servers: 0, online_servers: 0, offline_servers: 0, communities: 0, online_players: 0, admins: 0, admin_preview: [], whitelist_stats: { pending: 0, approved: 0, rejected: 0, revoked: 0 }, server_performance: { avg_fps: 0, avg_cpu_usage: 0, avg_tickrate: 0, total_players: 0, total_max_players: 0 } };
  const adminPreviewRows = normalizeAdminPreviewRows(stats.admin_preview);
  const ws = stats.whitelist_stats ?? { pending: 0, approved: 0, rejected: 0, revoked: 0 };
  const perf = stats.server_performance ?? { avg_fps: 0, avg_cpu_usage: 0, avg_tickrate: 0, total_players: 0, total_max_players: 0 };
  const playerPercent = perf.total_max_players > 0 ? Math.round((perf.total_players / perf.total_max_players) * 100) : 0;

  return (
    <div id="dashboard" className="content-section active">
      <div className="breadcrumb"><span>首页</span><span className="sep">›</span><span className="current">仪表盘</span></div>

      {/* ── 欢迎横幅 ── */}
      <div className="dash-hero">
        <div className="dash-hero-text">
          <div className="dash-hero-title">{greeting()}，{session?.displayName ?? 'Alex'}</div>
          <div className="dash-hero-sub">{formatToday()} — 服务器状态与管理概览</div>
        </div>
        <div className="dash-hero-badge">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>
          <span>每 30 秒自动刷新</span>
        </div>
      </div>

      {/* ── 核心统计卡片（真实数据 + 趋势） ── */}
      <div className="dash-section-label">
        <span className="dash-section-dot" />
        核心概览
      </div>
      <DashboardStats stats={stats} />

      {/* ── 服务器性能 ── */}
      <div className="dash-section-label">
        <span className="dash-section-dot" />
        服务器性能
      </div>
      <div className="dash-perf-grid">
        <div className="dash-perf-card">
          <div className="dash-perf-header">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--accent2)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12" /></svg>
            <span>平均 FPS</span>
          </div>
          <div className="dash-perf-value">{formatFps(perf.avg_fps)}</div>
          <div className="dash-perf-bar"><div className="dash-perf-fill dash-perf-fill-blue" style={{ width: `${Math.min(100, (perf.avg_fps || 0) / 1.28)}%` }} /></div>
        </div>
        <div className="dash-perf-card">
          <div className="dash-perf-header">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" /><path d="M3 9h18" /><path d="M9 21V9" /></svg>
            <span>CPU 使用率</span>
          </div>
          <div className="dash-perf-value" style={{ color: cpuColor(perf.avg_cpu_usage) }}>{formatCpu(perf.avg_cpu_usage)}</div>
          <div className="dash-perf-bar"><div className="dash-perf-fill" style={{ width: `${Math.min(100, perf.avg_cpu_usage || 0)}%`, background: cpuColor(perf.avg_cpu_usage) }} /></div>
        </div>
        <div className="dash-perf-card">
          <div className="dash-perf-header">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--teal)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>
            <span>平均 Tickrate</span>
          </div>
          <div className="dash-perf-value">{formatTickrate(perf.avg_tickrate)}</div>
          <div className="dash-perf-bar"><div className="dash-perf-fill dash-perf-fill-teal" style={{ width: `${Math.min(100, (perf.avg_tickrate || 0) / 1.28)}%` }} /></div>
        </div>
        <div className="dash-perf-card">
          <div className="dash-perf-header">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--pink)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" /></svg>
            <span>玩家容量</span>
          </div>
          <div className="dash-perf-value">{formatPlayerRatio(perf.total_players, perf.total_max_players)}</div>
          <div className="dash-perf-bar"><div className="dash-perf-fill dash-perf-fill-pink" style={{ width: `${playerPercent}%` }} /></div>
          <div className="dash-perf-hint">使用率 {playerPercent}%</div>
        </div>
      </div>

      {/* ── 白名单增长趋势 ── */}
      <div className="dash-section-label">
        <span className="dash-section-dot" />
        数据趋势
      </div>
      <WhitelistTrendChart />

      {/* ── 服务器活跃度 + 状态分布 ── */}
      <div className="dash-section-label">
        <span className="dash-section-dot" />
        服务器实时状态
      </div>
      <div className="dash-charts-grid">
        <ServerActivityChart />
        <ServerStatusChart />
      </div>

      {/* ── 服务器活跃度排行 ── */}
      <div className="dash-section-label">
        <span className="dash-section-dot" />
        活跃排行
      </div>
      <ServerRankingChart />

      {/* ── 下半区：管理员 + 白名单 ── */}
      <div className="dash-lower-grid">
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">快捷管理员预览</div>
              <div className="card-sub">当前所有网站管理员</div>
            </div>
          </div>
          <div className="card-body p-0">
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr><th>管理员</th><th>权限组</th><th>状态</th></tr>
                </thead>
                <tbody>
                  {adminPreviewRows.map((admin) => (
                    <tr key={`${admin.role}-${admin.displayName}`}>
                      <td><div className="user-cell"><div className="avatar avatar-info">{admin.initials}</div>{admin.displayName}</div></td>
                      <td><span className={`role-badge ${admin.role}`}>{admin.roleLabel}</span></td>
                      <td><span className="status-pill pill-online">{admin.status}</span></td>
                    </tr>
                  ))}
                  {adminPreviewRows.length === 0 && <tr><td colSpan="3" style={{ textAlign: 'center', color: 'var(--text2)', padding: 24 }}>暂无管理员数据</td></tr>}
                </tbody>
              </table>
            </div>
          </div>
        </div>

        <div className="dash-wl-card">
          <div className="dash-wl-header">
            <div>
              <div className="dash-wl-title">白名单统计</div>
              <div className="dash-wl-sub">全部申请状态汇总</div>
            </div>
          </div>
          <div className="dash-wl-grid">
            <div className="dash-wl-item dash-wl-pending">
              <div className="dash-wl-num">{ws.pending}</div>
              <div className="dash-wl-label">待审核</div>
            </div>
            <div className="dash-wl-item dash-wl-approved">
              <div className="dash-wl-num">{ws.approved}</div>
              <div className="dash-wl-label">已通过</div>
            </div>
            <div className="dash-wl-item dash-wl-rejected">
              <div className="dash-wl-num">{ws.rejected}</div>
              <div className="dash-wl-label">已拒绝</div>
            </div>
            <div className="dash-wl-item dash-wl-revoked">
              <div className="dash-wl-num">{ws.revoked}</div>
              <div className="dash-wl-label">已撤销</div>
            </div>
          </div>
        </div>
      </div>

      {/* ── 白名单公示快照 ── */}
      <div className="card">
        <div className="card-header">
          <div>
            <div className="card-title">白名单公示快照</div>
            <div className="card-sub">最近通过的申请</div>
          </div>
        </div>
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr><th>游戏昵称</th><th>SteamID64</th><th>状态</th><th>通过时间</th></tr>
              </thead>
              <tbody>
                {(publicWhitelist.data?.items ?? []).map((x) => (
                  <tr key={`${x.nickname}-${x.steam_id64}`}>
                    <td className="fw-600">{x.nickname}</td>
                    <td className="steam-id">{x.steam_id64}</td>
                    <td><span className="status-pill pill-online">已通过</span></td>
                    <td className="text-muted-light">{x.submitted_at}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
}
