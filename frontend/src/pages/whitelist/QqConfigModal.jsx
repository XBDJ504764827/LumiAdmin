import React, { useEffect, useState } from 'react';
import { Modal } from '../../shared/Modal.jsx';

// 白名单 QQ 群绑定设置弹窗（群号 / 加群链接 / 允许群 openid / 绑定上限 / 验证码有效期）
export function QqConfigModal({ open, onClose, config, onSave, saving }) {
  const [enabled, setEnabled] = useState(true);
  const [groupNumber, setGroupNumber] = useState('');
  const [groupLink, setGroupLink] = useState('');
  const [groupOpenids, setGroupOpenids] = useState('');
  const [maxBindings, setMaxBindings] = useState(5);
  const [codeTtl, setCodeTtl] = useState(300);
  const [error, setError] = useState('');

  useEffect(() => {
    if (!open || !config) return;
    // 打开弹窗时用配置初始化表单；包在 startTransition 中避免级联同步渲染
    React.startTransition(() => {
      setEnabled(config.enabled ?? true);
      setGroupNumber(config.group_number ?? '');
      setGroupLink(config.group_link ?? '');
      setGroupOpenids((config.group_openids ?? []).join('\n'));
      setMaxBindings(config.max_bindings ?? 5);
      setCodeTtl(config.code_ttl_seconds ?? 300);
      setError('');
    });
  }, [open, config]);

  function submit() {
    if (!groupNumber.trim()) { setError('请填写 QQ 群号。'); return; }
    setError('');
    onSave({
      enabled,
      group_number: groupNumber.trim(),
      group_link: groupLink.trim() || null,
      group_openids: groupOpenids.split('\n').map((s) => s.trim()).filter(Boolean),
      max_bindings: Number(maxBindings) || 5,
      code_ttl_seconds: Number(codeTtl) || 300,
    });
  }

  return (
    <Modal
      open={open}
      title="QQ 群绑定设置"
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-outline" onClick={onClose} disabled={saving}>取消</button>
          <button className="btn btn-accent" onClick={submit} disabled={saving}>{saving ? '保存中...' : '保存'}</button>
        </>
      }
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <label style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span>要求玩家完成 QQ 群绑定后才能提交白名单</span>
        </label>

        <div className="form-group">
          <label>QQ 群号 <span className="text-accent">*</span></label>
          <input className="form-control" value={groupNumber} onChange={(e) => setGroupNumber(e.target.value)} placeholder="如 275164688" />
        </div>

        <div className="form-group">
          <label>一键加群链接</label>
          <input className="form-control" value={groupLink} onChange={(e) => setGroupLink(e.target.value)} placeholder="https://qm.qq.com/q/..." />
        </div>

        <div className="form-group">
          <label>允许绑定的群 openid（每行一个，留空不限制）</label>
          <textarea
            className="form-control"
            rows={3}
            value={groupOpenids}
            onChange={(e) => setGroupOpenids(e.target.value)}
            placeholder="QQ 机器人事件中的群 openid（非数字群号）"
          />
          <div className="form-hint">首次绑定时 Bot 会回传群 openid，可复制到此处锁定允许的群。</div>
        </div>

        <div style={{ display: 'flex', gap: 12 }}>
          <div className="form-group" style={{ flex: 1 }}>
            <label>单个 QQ 最多绑定 Steam 数</label>
            <input type="number" className="form-control" min={1} max={50} value={maxBindings} onChange={(e) => setMaxBindings(e.target.value)} />
          </div>
          <div className="form-group" style={{ flex: 1 }}>
            <label>验证码有效期（秒）</label>
            <input type="number" className="form-control" min={60} max={3600} value={codeTtl} onChange={(e) => setCodeTtl(e.target.value)} />
          </div>
        </div>

        {error ? <div className="form-hint" style={{ color: 'var(--danger-text)' }}>{error}</div> : null}
      </div>
    </Modal>
  );
}
