import { Modal } from '../../shared/Modal.jsx';

/**
 * IP 封禁解封范围选择。
 *
 * - 仅解封该账号：保留 IP 封禁，其他同 IP 账号仍被拦截；该账号从这条 IP 封禁中豁免。
 * - 账号与账号下 IP 全部解封：解封账号及其使用过的 IP；同 IP 上其他仍被封禁的账号
 *   转为 Steam 账号封禁，不再连坐。
 */
export function UnbanScopeModal({ open, item, acting = false, onClose, onConfirm }) {
  if (!open || !item) return null;
  const accountAlreadyUnbanned = Boolean(item.account_unbanned);

  return (
    <Modal
      open
      title="解封确认"
      subtitle="该封禁为 IP 封禁，请选择解封范围"
      onClose={onClose}
      wide
      footer={<button className="btn btn-outline" type="button" onClick={onClose}>取消</button>}
    >
      <div className="unban-scope-summary">
        <div><span>玩家</span><strong>{item.player || '待自动获取'}</strong></div>
        <div><span>SteamID64</span><code>{item.steam_id}</code></div>
        <div><span>封禁 IP</span><code>{item.ip_address || '未记录'}</code></div>
      </div>

      <div className="unban-scope-list">
        <div className="unban-scope-card">
          <div className="unban-scope-title">仅解封该账号</div>
          <p>
            保留该 IP 的封禁：其他使用该 IP 的账号仍会被拦截。
            该账号可以正常进入；如果进入时与其他仍在封禁中的账号共用 IP，会被阻止并提示，
            但不会自动再次封禁该账号。
          </p>
          <button
            type="button"
            className="btn btn-outline"
            disabled={acting || accountAlreadyUnbanned}
            onClick={() => onConfirm('account')}
          >
            {accountAlreadyUnbanned ? '该账号已仅解封' : acting ? '处理中...' : '仅解封该账号'}
          </button>
        </div>

        <div className="unban-scope-card danger">
          <div className="unban-scope-title">账号与账号下 IP 全部解封</div>
          <p>
            解封该账号以及它使用过的全部 IP。同一 IP 上其他仍被封禁的账号将转为
            Steam 账号封禁，继续按账号封禁，不再受 IP 连坐影响。
          </p>
          <button
            type="button"
            className="btn btn-danger"
            disabled={acting}
            onClick={() => onConfirm('account_ip')}
          >
            {acting ? '处理中...' : '账号与账号下 IP 全部解封'}
          </button>
        </div>
      </div>
    </Modal>
  );
}
