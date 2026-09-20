import React, { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api.js';
import { useAuth } from '../state/store.js';
import { useToast } from './Toast.jsx';
import { useConfirmDialog } from './ConfirmModal.jsx';
import { formatChinaDateTime } from './time.js';

// QQ 绑定信息面板 + 群内 @玩家：白名单审核/玩家详情共用。
//
// 功能：
// - 展示 Steam 绑定的 QQ openid / 群 / 昵称 / 验证方式
// - 管理员在群内 @玩家（可编辑文案，同玩家 1 分钟冷却）
// - 解绑（换绑前需先解绑）
export function QqBindingPanel({ steamid64, compact = false }) {
  const { session } = useAuth();
  const { toast } = useToast();
  const { confirm, dialog } = useConfirmDialog();
  const token = session?.token ?? null;
  // 群内 @玩家 / 解绑仅限 developer / admin（后端同样鉴权）
  const canManage = session?.role === 'developer' || session?.role === 'admin';

  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [mentionOpen, setMentionOpen] = useState(false);
  const [mentionText, setMentionText] = useState('管理员请你查看白名单审核进度，尽快回复。');
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
  const latest = data?.latest_mention;

  async function handleMention() {
    if (sending) return;
    setSending(true);
    try {
      const result = await api.mentionWhitelistQq(token, steamid64, { content: mentionText.trim() || undefined });
      toast({ title: '已发送群内通知', message: result?.response?.message_id ? `消息ID：${result.response.message_id}` : undefined });
      setMentionOpen(false);
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
      message: '确定解除该 Steam 账号的 QQ 绑定吗？解除后玩家需重新加群验证才能申请白名单。',
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
          <div>群 openid：<code className="steam-id">{binding.qq_group_id}</code></div>
          <div>绑定时间：{formatChinaDateTime(binding.verified_at)}</div>
          <div>同 QQ 绑定数：{data?.qq_binding_count ?? 1}</div>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 6 }}>
            {canManage ? <button className="action-btn action-btn-accent" type="button" onClick={() => setMentionOpen(true)}>群内 @玩家</button> : null}
            {canManage ? <button className="action-btn action-btn-danger" type="button" onClick={handleUnbind}>解绑</button> : null}
          </div>
          {latest ? (
            <div style={{ marginTop: 6, fontSize: 12, color: latest.status === 'sent' ? 'var(--teal)' : 'var(--danger-text)' }}>
              最近群内通知：{formatChinaDateTime(latest.created_at, { seconds: false })} · {latest.status === 'sent' ? '已发送' : `失败（${latest.error || '未知原因'}）`}
            </div>
          ) : null}
        </div>
      ) : (
        <div className="text-muted-light fs-12">该玩家尚未完成 QQ 群验证绑定。</div>
      )}

      {mentionOpen ? (
        <div style={{ marginTop: 10, padding: 12, border: '1px solid var(--border)', borderRadius: 8, background: 'var(--surface2)' }}>
          <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 6 }}>群内通知内容（最多 200 字）</div>
          <textarea
            className="form-control"
            rows={compact ? 2 : 3}
            value={mentionText}
            maxLength={200}
            onChange={(e) => setMentionText(e.target.value)}
            disabled={sending}
          />
          <div style={{ display: 'flex', gap: 8, marginTop: 8, justifyContent: 'flex-end' }}>
            <button className="btn btn-outline" type="button" onClick={() => setMentionOpen(false)} disabled={sending}>取消</button>
            <button className="btn btn-accent" type="button" onClick={handleMention} disabled={sending}>{sending ? '发送中...' : '发送并 @玩家'}</button>
          </div>
        </div>
      ) : null}

      {dialog}
    </div>
  );
}
