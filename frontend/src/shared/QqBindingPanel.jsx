import React, { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api.js';
import { useAuth } from '../state/store.js';
import { useToast } from './Toast.jsx';
import { useConfirmDialog } from './ConfirmModal.jsx';
import { formatChinaDateTime } from './time.js';

// QQ 绑定信息面板 + 管理员私聊：白名单审核/玩家详情共用。
//
// 功能：
// - 展示 Steam 绑定的 QQ openid / 昵称 / 验证时间
// - 管理员向玩家发起 QQ 私聊（已开通主动消息权限时可直接发送）
// - 展示与玩家的聊天记录（管理员发送 / 玩家回复）
// - 解绑（换绑前需先解绑）
export function QqBindingPanel({ steamid64, compact = false }) {
  const { session } = useAuth();
  const { toast } = useToast();
  const { confirm, dialog } = useConfirmDialog();
  const token = session?.token ?? null;
  // 私聊 / 解绑仅限 developer / admin（后端同样鉴权）
  const canManage = session?.role === 'developer' || session?.role === 'admin';

  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [chatText, setChatText] = useState('');
  const [sending, setSending] = useState(false);

  const load = useCallback(async () => {
    if (!token || !steamid64) return;
    setLoading(true);
    try {
      const result = await api.whitelistQqBinding(token, steamid64);
      setData(result);
    } catch {
      setData(null);
    } finally {
      setLoading(false);
    }
  }, [token, steamid64]);

  useEffect(() => {
    React.startTransition(() => { load(); });
  }, [load]);

  if (!steamid64) return null;

  const binding = data?.binding;
  const messages = data?.chat_messages || [];

  async function handleSend() {
    if (sending) return;
    const content = chatText.trim();
    if (!content) { toast({ title: '请输入消息内容', tone: 'warning' }); return; }
    setSending(true);
    try {
      await api.sendWhitelistQqChat(token, steamid64, { content });
      toast({ title: '已发送' });
      setChatText('');
      await load();
    } catch (e) {
      toast({ title: '发送失败', message: e.message, tone: 'danger' });
      await load();
    } finally {
      setSending(false);
    }
  }

  async function handleUnbind() {
    const ok = await confirm({
      title: '解除 QQ 绑定',
      message: '确定解除该 Steam 账号的 QQ 绑定吗？解除后玩家需重新私聊验证才能申请白名单。',
      tone: 'danger',
      confirmText: '确认解绑',
    });
    if (!ok) return;
    try {
      await api.deleteWhitelistQqBinding(token, steamid64);
      toast({ title: '已解绑' });
      await load();
    } catch (e) {
      toast({ title: '解绑失败', message: e.message, tone: 'danger' });
    }
  }

  if (loading && !data) {
    return <div className="form-group"><label className="mb-4">QQ 绑定</label><div className="text-muted-light fs-12">加载中…</div></div>;
  }

  return (
    <div className="form-group">
      <label className="mb-4">QQ 绑定（联系方式）</label>
      {binding ? (
        <div style={{ color: 'var(--text2)', fontSize: 13, display: 'flex', flexDirection: 'column', gap: 4 }}>
          <div>QQ openid：<code className="steam-id">{binding.qq_openid}</code></div>
          <div>QQ 昵称：{binding.qq_username || '-'}</div>
          <div>绑定时间：{formatChinaDateTime(binding.verified_at)}</div>
          <div>同 QQ 绑定数：{data?.qq_binding_count ?? 1}</div>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 6 }}>
            {canManage ? <button className="action-btn action-btn-accent" type="button" onClick={() => setChatOpen((v) => !v)}>{chatOpen ? '收起聊天' : '私聊玩家'}</button> : null}
            {canManage ? <button className="action-btn action-btn-danger" type="button" onClick={handleUnbind}>解绑</button> : null}
          </div>

          {chatOpen && canManage ? (
            <div style={{ marginTop: 10, border: '1px solid var(--border)', borderRadius: 8, background: 'var(--surface2)' }}>
              <div className="qq-chat-list">
                {messages.length === 0 ? (
                  <div className="qq-chat-empty">暂无聊天记录</div>
                ) : messages.map((m) => (
                  <div key={m.id} className={`qq-chat-row ${m.direction === 'admin_to_player' ? 'mine' : 'theirs'} ${m.status === 'failed' ? 'failed' : ''}`}>
                    <div className="qq-chat-bubble">
                      <div className="qq-chat-content">{m.content}</div>
                      <div className="qq-chat-meta">
                        {formatChinaDateTime(m.created_at, { seconds: false })}
                        {m.operator_name ? ` · ${m.operator_name}` : ''}
                        {m.status === 'failed' ? ` · 发送失败（${m.error || '未知原因'}）` : ''}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
              <div style={{ padding: 10, borderTop: '1px solid var(--border)' }}>
                <textarea
                  className="form-control"
                  rows={compact ? 2 : 3}
                  value={chatText}
                  maxLength={500}
                  placeholder="输入要私聊发送给玩家的内容"
                  onChange={(e) => setChatText(e.target.value)}
                  disabled={sending}
                />
                <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
                  <button className="btn btn-accent" type="button" onClick={handleSend} disabled={sending}>{sending ? '发送中...' : '发送私聊'}</button>
                </div>
              </div>
            </div>
          ) : null}
        </div>
      ) : (
        <div className="text-muted-light fs-12">该玩家尚未完成 QQ 私聊验证绑定。</div>
      )}

      {dialog}
    </div>
  );
}
