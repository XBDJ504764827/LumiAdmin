import React, { useEffect, useState } from 'react';
import { Modal } from '../../shared/Modal.jsx';
import { ToggleSwitch } from '../community/CommunityComponents.jsx';

// 白名单 QQ 绑定设置弹窗（机器人名称 / 机器人 QQ 号 / 绑定上限 / 验证码有效期）
export function QqConfigModal({ open, onClose, config, onSave, saving }) {
  const [enabled, setEnabled] = useState(true);
  const [botName, setBotName] = useState('');
  const [botQq, setBotQq] = useState('');
  const [maxBindings, setMaxBindings] = useState(5);
  const [codeTtl, setCodeTtl] = useState(300);
  const [error, setError] = useState('');

  useEffect(() => {
    if (!open || !config) return;
    // 打开弹窗时用配置初始化表单；包在 startTransition 中避免级联同步渲染
    React.startTransition(() => {
      setEnabled(config.enabled ?? true);
      setBotName(config.bot_name ?? '');
      setBotQq(config.bot_qq ?? '');
      setMaxBindings(config.max_bindings ?? 5);
      setCodeTtl(config.code_ttl_seconds ?? 300);
      setError('');
    });
  }, [open, config]);

  function submit() {
    if (!botQq.trim()) { setError('请填写机器人 QQ 号。'); return; }
    setError('');
    onSave({
      enabled,
      bot_name: botName.trim() || null,
      bot_qq: botQq.trim(),
      max_bindings: Number(maxBindings) || 5,
      code_ttl_seconds: Number(codeTtl) || 300,
    });
  }

  return (
    <Modal
      open={open}
      title="QQ 绑定设置"
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-outline" onClick={onClose} disabled={saving}>取消</button>
          <button className="btn btn-accent" onClick={submit} disabled={saving}>{saving ? '保存中...' : '保存'}</button>
        </>
      }
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <ToggleSwitch checked={enabled} onChange={setEnabled} disabled={saving} />
          <span>要求玩家完成 QQ 绑定后才能提交白名单</span>
        </div>

        <div className="form-group">
          <label>机器人名称 <span className="text-accent">*</span></label>
          <input className="form-control" value={botName} onChange={(e) => setBotName(e.target.value)} placeholder="如 CNGOKZBOT" />
        </div>

        <div className="form-group">
          <label>机器人 QQ 号 <span className="text-accent">*</span></label>
          <input className="form-control" value={botQq} onChange={(e) => setBotQq(e.target.value)} placeholder="如 3889010779" />
          <div className="form-hint">公开申请页会引导玩家添加该 QQ 并私聊发送验证码。</div>
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
