import { serverStatusMeta } from '../../shared/serverStatus.js';
import { buildAccessSummary } from './communityAccess.js';

export function CommunityServerTable({ group, renderTokenCell, renderServerActions }) {
  return (
    <div className="table-responsive">
      <table className="data-table mobile-card-table">
        <thead>
          <tr>
            <th>服务器名称</th>
            <th>地址 / 端口</th>
            <th>Token 令牌</th>
            <th>状态</th>
            <th>访问限制</th>
            <th>当前人数</th>
            <th className="text-right">操作</th>
          </tr>
        </thead>
        <tbody>
          {group.servers.length === 0 ? (
            <tr><td colSpan={7} style={{ padding: 20, color: 'var(--text3)' }}>暂无服务器。</td></tr>
          ) : (
            group.servers.map((server) => {
              const status = serverStatusMeta(server.status);
              const playerCount = server.online_player_count ?? server.players?.length ?? 0;
              const maxPlayers = server.max_players ?? 0;
              return (
                <tr key={server.id}>
                  <td className="fw-600 mobile-card-primary" data-label="服务器名称">{server.name}</td>
                  <td className="steam-id" data-label="地址 / 端口">{server.ip}:{server.port}</td>
                  <td data-label="Token 令牌">{renderTokenCell(server)}</td>
                  <td data-label="状态">
                    <span className={`status-pill ${status.className}`}>
                      {status.label}
                    </span>
                  </td>
                  <td data-label="访问限制" style={{ fontSize: 12, color: 'var(--text3)' }}>{buildAccessSummary(server, group)}</td>
                  <td data-label="当前人数">
                    {status.online ? `${playerCount} / ${maxPlayers}` : <span className="text-muted-light">0 / {maxPlayers}</span>}
                  </td>
                  <td className="text-right mobile-card-actions" data-label="操作">{renderServerActions(server)}</td>
                </tr>
              );
            })
          )}
        </tbody>
      </table>
    </div>
  );
}
