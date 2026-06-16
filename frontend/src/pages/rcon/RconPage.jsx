import { useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/store.js';
import { useApiQuery } from '../../shared/useApiQuery.js';
import { useToast } from '../../shared/Toast.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';

export const COMMAND_CATEGORIES = [
  {
    name: '地图管理',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="20" height="20">
        <path d="M1 6v16l7-4 8 4 7-4V2l-7 4-8-4-7 4z" /><path d="M8 2v16" /><path d="M16 6v16" />
      </svg>
    ),
    commands: [
      { name: '恢复默认地图', desc: '切换到 kz_yes，当前服务器地图损坏玩家无法进入时使用', command: 'sm_map kz_yes', danger: true },
      { name: '重启当前地图', desc: '重新加载当前地图，所有玩家将重新进入', command: 'sm_restart', danger: true },
    ],
  },
  {
    name: '服务器管理',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="20" height="20">
        <rect x="2" y="2" width="20" height="8" rx="2" /><rect x="2" y="14" width="20" height="8" rx="2" /><circle cx="6" cy="6" r="1" /><circle cx="6" cy="18" r="1" />
      </svg>
    ),
    commands: [
      { name: '重载管理员列表', desc: '重新加载管理员权限配置', command: 'sm_reloadadmins', danger: false },
      { name: '重载插件配置', desc: '重新加载所有 SourceMod 插件', command: 'sm_reload', danger: true },
    ],
  },
];

export function RconPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast } = useToast();
  const token = session?.token ?? null;

  const [selectedServerId, setSelectedServerId] = useState('');
  const [executing, setExecuting] = useState('');
  const [customCommand, setCustomCommand] = useState('');

  const { data: serversResponse, isLoading, error: loadError } = useApiQuery(
    ['communityServers'],
    (token) => api.servers(token),
  );

  const groups = serversResponse?.groups ?? [];

  // 服务器列表（扁平化）
  const allServers = groups.flatMap((g) =>
    (g.servers ?? []).map((s) => ({
      id: s.id,
      name: s.name,
      ip: s.ip,
      port: s.port,
      status: s.status,
      groupName: g.name,
    }))
  );
  const selectedServer = allServers.find((s) => s.id === selectedServerId);

  async function handleExecute(cmd) {
    if (!selectedServerId) {
      toast({ title: '请先选择服务器', message: '在执行命令前需要选择目标服务器。', tone: 'danger' });
      return;
    }
    const confirmed = await confirm({
      title: '确认执行命令',
      message: `即将在服务器「${selectedServer.name}」上执行：\n\n${cmd}\n\n此操作可能影响在线玩家，确定继续？`,
      confirmText: '确认执行',
    });
    if (!confirmed) return;

    setExecuting(cmd);
    try {
      await api.executeRcon(token, selectedServerId, { command: cmd });
      toast({ title: '执行成功', message: `命令已发送至「${selectedServer.name}」。` });
    } catch (e) {
      toast({ title: '执行失败', message: e.message, tone: 'danger' });
    } finally {
      setExecuting('');
    }
  }

  async function handleCustomExecute() {
    const cmd = customCommand.trim();
    if (!cmd) return;
    await handleExecute(cmd);
    setCustomCommand('');
  }

  // handleExecute 内部需要 token，但 useApiMutation 不太适合这里因为需要 selectedServerId
  // 使用 useApiQuery 已获取 serversResponse，token 通过 useApiQuery 自动注入

  return (
    <div id="rcon" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">RCON 命令</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">RCON 快捷命令</div>
          <div className="page-sub">通过 RCON 协议向游戏服务器发送远程控制命令。</div>
        </div>
      </div>

      {/* 服务器选择器 */}
      <div className="card">
        <div className="card-header">
          <div>
            <div className="card-title">目标服务器</div>
            <div className="card-sub">选择要执行命令的游戏服务器</div>
          </div>
        </div>
        <div className="card-body">
          {isLoading ? (
            <div className="text-muted-light">正在加载服务器列表...</div>
          ) : loadError ? (
            <div style={{ color: 'var(--danger)' }}>{loadError.message}</div>
          ) : allServers.length === 0 ? (
            <div className="text-muted-light">暂无可用服务器，请先在社区组管理中添加服务器。</div>
          ) : (
            <select
              className="form-control"
              value={selectedServerId}
              onChange={(e) => setSelectedServerId(e.target.value)}
            >
              <option value="">-- 请选择服务器 --</option>
              {allServers.map((s) => (
                <option key={s.id} value={s.id}>
                  [{s.groupName}] {s.name} ({s.ip}:{s.port}) {s.status === 'online' ? '● 在线' : '○ 离线'}
                </option>
              ))}
            </select>
          )}
          {selectedServer && (
            <div style={{ marginTop: 8, fontSize: 12, color: selectedServer.status === 'online' ? 'var(--success-text)' : 'var(--danger-text)' }}>
              {selectedServer.status === 'online' ? `● ${selectedServer.name} 在线` : `○ ${selectedServer.name} 离线 — 命令可能无法执行`}
            </div>
          )}
        </div>
      </div>

      {/* 命令目录 */}
      {COMMAND_CATEGORIES.map((cat) => (
        <div className="card mt-16" key={cat.name}>
          <div className="card-header">
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span className="text-accent">{cat.icon}</span>
              <div>
                <div className="card-title">{cat.name}</div>
                <div className="card-sub">共 {cat.commands.length} 个命令</div>
              </div>
            </div>
          </div>
          <div className="card-body p-0">
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>命令名称</th>
                    <th>说明</th>
                    <th>RCON 指令</th>
                    <th className="text-right">操作</th>
                  </tr>
                </thead>
                <tbody>
                  {cat.commands.map((cmd) => (
                    <tr key={cmd.command}>
                      <td className="fw-600">
                        {cmd.name}
                        {cmd.danger ? <span style={{ marginLeft: 6, fontSize: 11, color: 'var(--danger-text)' }}>⚠ 高影响</span> : null}
                      </td>
                      <td style={{ color: 'var(--text2)', fontSize: 13 }}>{cmd.desc}</td>
                      <td><code style={{ fontSize: 12, background: 'var(--surface2)', padding: '2px 6px', borderRadius: 4 }}>{cmd.command}</code></td>
                      <td className="text-right">
                        <button
                          className="action-btn action-btn-accent"
                          disabled={!selectedServerId || !!executing}
                          onClick={() => handleExecute(cmd.command)}
                        >
                          {executing === cmd.command ? '执行中...' : '执行'}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      ))}

      {/* 自定义命令 */}
      <div className="card mt-16">
        <div className="card-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span className="text-accent">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="20" height="20">
                <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" /><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
              </svg>
            </span>
            <div>
              <div className="card-title">自定义命令</div>
              <div className="card-sub">输入任意 RCON 命令并发送到服务器</div>
            </div>
          </div>
        </div>
        <div className="card-body">
          <div style={{ display: 'flex', gap: 8 }}>
            <input
              type="text"
              className="form-control"
              value={customCommand}
              onChange={(e) => setCustomCommand(e.target.value)}
              placeholder={'输入 RCON 命令，如 sm_kick "玩家名"'}
              disabled={!selectedServerId || !!executing}
              onKeyDown={(e) => { if (e.key === 'Enter' && customCommand.trim()) handleCustomExecute(); }}
            />
            <button
              className="btn btn-primary flex-shrink-0"
              disabled={!selectedServerId || !customCommand.trim() || !!executing}
              onClick={handleCustomExecute}
            >
              {executing === customCommand.trim() ? '执行中...' : '执行'}
            </button>
          </div>
          <div style={{ marginTop: 6, fontSize: 11, color: 'var(--text3)' }}>
            支持所有 SourceMod RCON 命令，按 Enter 快速执行
          </div>
        </div>
      </div>

      {dialog}
    </div>
  );
}
