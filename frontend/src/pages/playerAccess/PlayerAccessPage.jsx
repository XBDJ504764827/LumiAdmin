import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../../lib/api.js';
import { useAuth } from '../../state/auth.jsx';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { formatChinaDate } from '../../shared/time.js';

export function PlayerAccessPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const token = session?.token ?? null;

  const [rules, setRules] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  // 弹窗状态
  const [modalOpen, setModalOpen] = useState(false);
  const [editingRule, setEditingRule] = useState(null);
  const [form, setForm] = useState({
    steamid64: '',
    nickname: '',
    allowedCommunities: [],
    blockedCommunities: [],
    allowedServers: [],
    blockedServers: [],
  });
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState('');

  // 社区和服务器列表
  const [groups, setGroups] = useState([]);

  // 玩家搜索
  const [searchInput, setSearchInput] = useState('');
  const [searchResults, setSearchResults] = useState([]);
  const [searching, setSearching] = useState(false);
  const [selectedPlayer, setSelectedPlayer] = useState(null);
  const searchTimerRef = useRef(null);

  // 社区展开/折叠
  const [expandedGroups, setExpandedGroups] = useState({});

  const loadRules = useCallback(async () => {
    try {
      setLoading(true);
      setError('');
      const result = await api.playerAccessRules(token);
      setRules(result.items ?? []);
    } catch (e) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  }, [token]);

  const loadGroups = useCallback(async () => {
    try {
      const result = await api.servers(token);
      setGroups(result.groups ?? []);
    } catch { /* ignore */ }
  }, [token]);

  useEffect(() => { loadRules(); loadGroups(); }, [loadRules, loadGroups]);

  // 服务端搜索白名单玩家
  const searchWhitelist = useCallback(async (query) => {
    if (!query.trim()) { setSearchResults([]); return; }
    try {
      setSearching(true);
      const result = await api.whitelist(token, { search: query.trim(), status: 'approved', page_size: 20 });
      setSearchResults(result.items ?? []);
    } catch {
      setSearchResults([]);
    } finally {
      setSearching(false);
    }
  }, [token]);

  const handleSearchChange = useCallback((e) => {
    const value = e.target.value;
    setSearchInput(value);
    setSelectedPlayer(null);

    if (searchTimerRef.current) clearTimeout(searchTimerRef.current);
    if (!value.trim()) {
      setSearchResults([]);
      setSearching(false);
      return;
    }
    setSearching(true);
    searchTimerRef.current = setTimeout(() => searchWhitelist(value), 300);
  }, [searchWhitelist]);

  function openCreateModal() {
    setEditingRule(null);
    setForm({
      steamid64: '', nickname: '',
      allowedCommunities: [], blockedCommunities: [],
      allowedServers: [], blockedServers: [],
    });
    setSearchInput('');
    setSearchResults([]);
    setSelectedPlayer(null);
    setFormError('');
    setExpandedGroups({});
    setModalOpen(true);
  }

  function openEditModal(rule) {
    setEditingRule(rule);
    setForm({
      steamid64: rule.steamid64,
      nickname: rule.nickname,
      allowedCommunities: rule.allowed_communities ?? [],
      blockedCommunities: rule.blocked_communities ?? [],
      allowedServers: rule.allowed_servers ?? [],
      blockedServers: rule.blocked_servers ?? [],
    });
    setSearchInput('');
    setSearchResults([]);
    setSelectedPlayer({ steamid64: rule.steamid64, nickname: rule.nickname });
    setFormError('');
    // 编辑时展开所有有配置的社区
    const expanded = {};
    groups.forEach((g) => {
      const hasServerConfig = (g.servers ?? []).some(
        (s) => rule.allowed_servers?.includes(s.id) || rule.blocked_servers?.includes(s.id)
      );
      if (hasServerConfig || rule.allowed_communities?.includes(g.id) || rule.blocked_communities?.includes(g.id)) {
        expanded[g.id] = true;
      }
    });
    setExpandedGroups(expanded);
    setModalOpen(true);
  }

  function selectPlayer(player) {
    setSelectedPlayer(player);
    setForm((prev) => ({ ...prev, steamid64: player.steamid64, nickname: player.nickname }));
    setSearchInput('');
    setSearchResults([]);
  }

  function clearSelectedPlayer() {
    setSelectedPlayer(null);
    setForm((prev) => ({ ...prev, steamid64: '', nickname: '' }));
    setSearchInput('');
    setSearchResults([]);
  }

  function toggleGroup(groupId) {
    setExpandedGroups((prev) => ({ ...prev, [groupId]: !prev[groupId] }));
  }

  function setCommunityAccess(groupId, value) {
    setForm((prev) => ({
      ...prev,
      allowedCommunities: value === 'allowed' ? [...prev.allowedCommunities, groupId] : prev.allowedCommunities.filter((id) => id !== groupId),
      blockedCommunities: value === 'blocked' ? [...prev.blockedCommunities, groupId] : prev.blockedCommunities.filter((id) => id !== groupId),
    }));
  }

  function setServerAccess(serverId, value) {
    setForm((prev) => ({
      ...prev,
      allowedServers: value === 'allowed' ? [...prev.allowedServers, serverId] : prev.allowedServers.filter((id) => id !== serverId),
      blockedServers: value === 'blocked' ? [...prev.blockedServers, serverId] : prev.blockedServers.filter((id) => id !== serverId),
    }));
  }

  // 规则效果摘要
  const ruleSummary = useMemo(() => {
    const parts = [];
    groups.forEach((group) => {
      const isBlocked = form.blockedCommunities.includes(group.id);
      const isAllowed = form.allowedCommunities.includes(group.id);
      const servers = group.servers ?? [];

      // 找出该社区下有特殊配置的服务器
      const overrideServers = servers.filter(
        (s) => form.allowedServers.includes(s.id) || form.blockedServers.includes(s.id)
      );

      if (isBlocked) {
        const text = overrideServers.length > 0
          ? `禁止进入「${group.name}」，但 ${overrideServers.map((s) => form.allowedServers.includes(s.id) ? `强制允许进入「${s.name}」` : `强制禁止进入「${s.name}」`).join('、')}`
          : `禁止进入「${group.name}」的所有服务器`;
        parts.push({ type: 'blocked', text });
      } else if (isAllowed) {
        const blockedServers = overrideServers.filter((s) => form.blockedServers.includes(s.id));
        const text = blockedServers.length > 0
          ? `允许进入「${group.name}」所有服务器，但 ${blockedServers.map((s) => `强制禁止进入「${s.name}」`).join('、')}`
          : `允许进入「${group.name}」的所有服务器`;
        parts.push({ type: 'allowed', text });
      } else if (overrideServers.length > 0) {
        parts.push({
          type: 'override',
          text: `在「${group.name}」中：${overrideServers.map((s) => form.allowedServers.includes(s.id) ? `允许进入「${s.name}」` : `禁止进入「${s.name}」`).join('、')}`,
        });
      }
    });

    if (parts.length === 0) {
      return '该玩家可以进入所有服务器（默认全服通行）';
    }
    return parts.map((p) => p.text).join('；') + '。';
  }, [form, groups]);

  async function handleSubmit() {
    if (!selectedPlayer && !editingRule) {
      setFormError('请从搜索结果中选择一个白名单玩家');
      return;
    }
    try {
      setSubmitting(true);
      setFormError('');
      if (editingRule) {
        await api.updatePlayerAccessRule(token, editingRule.id, {
          nickname: form.nickname.trim(),
          allowed_communities: form.allowedCommunities,
          blocked_communities: form.blockedCommunities,
          allowed_servers: form.allowedServers,
          blocked_servers: form.blockedServers,
        });
        toast({ title: '更新成功', message: `${form.nickname} 的进服权限已更新。` });
      } else {
        await api.createPlayerAccessRule(token, {
          steamid64: form.steamid64.trim(),
          nickname: form.nickname.trim(),
          allowed_communities: form.allowedCommunities,
          blocked_communities: form.blockedCommunities,
          allowed_servers: form.allowedServers,
          blocked_servers: form.blockedServers,
        });
        toast({ title: '创建成功', message: `${form.nickname} 的进服权限已创建。` });
      }
      setModalOpen(false);
      loadRules();
    } catch (e) {
      setFormError(e.message);
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDelete(rule) {
    const confirmed = await confirm({
      title: '重置权限',
      message: `确定要重置 ${rule.nickname} 的进服权限为默认吗？`,
      confirmText: '确认重置',
    });
    if (!confirmed) return;
    try {
      await api.deletePlayerAccessRule(token, rule.id);
      toast({ title: '已重置', message: `${rule.nickname} 的权限已重置为默认。` });
      loadRules();
    } catch (e) {
      toast({ title: '操作失败', message: e.message, tone: 'danger' });
    }
  }

  // 列表中 tag 展示
  const idNameMap = useMemo(() => {
    const map = new Map();
    groups.forEach((g) => {
      map.set(g.id, { name: g.name, type: 'community' });
      (g.servers ?? []).forEach((s) => map.set(s.id, { name: s.name, type: 'server' }));
    });
    return map;
  }, [groups]);

  function renderAccessTags(rule) {
    const allowed = [...(rule.allowed_communities ?? []), ...(rule.allowed_servers ?? [])];
    const blocked = [...(rule.blocked_communities ?? []), ...(rule.blocked_servers ?? [])];
    if (allowed.length === 0 && blocked.length === 0) {
      return <span className="text-muted-light">默认全服通行</span>;
    }
    return (
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
        {allowed.map((id) => (
          <span key={id} className="status-pill" style={{ background: 'var(--success-bg)', color: 'var(--success-text)', padding: '2px 8px', borderRadius: 12, fontSize: 11, fontWeight: 500 }}>
            {idNameMap.get(id)?.name ?? id.slice(0, 8)}
          </span>
        ))}
        {blocked.map((id) => (
          <span key={id} className="status-pill" style={{ background: 'var(--danger-bg)', color: 'var(--danger-text)', padding: '2px 8px', borderRadius: 12, fontSize: 11, fontWeight: 500 }}>
            {idNameMap.get(id)?.name ?? id.slice(0, 8)}
          </span>
        ))}
      </div>
    );
  }

  function AccessSelect({ value, onChange, options, size }) {
    const colorMap = { default: 'var(--text3)', allowed: 'var(--success-text)', blocked: 'var(--danger-text)' };
    return (
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        style={{
          padding: size === 'sm' ? '4px 8px' : '6px 10px',
          border: '1px solid var(--border)',
          borderRadius: 8,
          fontSize: 12,
          outline: 'none',
          background: 'var(--surface)',
          color: colorMap[value] || 'var(--text)',
          cursor: 'pointer',
          minWidth: size === 'sm' ? 90 : 120,
          fontWeight: 500,
          transition: 'all 0.15s',
        }}
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>{opt.label}</option>
        ))}
      </select>
    );
  }

  return (
    <div id="playerAccess" className="content-section active">
      <div className="breadcrumb">
        <span>系统功能</span>
        <span className="sep">›</span>
        <span className="current">玩家进服设置</span>
      </div>

      <div className="page-header">
        <div>
          <div className="page-title">玩家进服权限配置</div>
          <div className="page-sub">针对已通过白名单的玩家，精确控制其能够进入或被禁止进入的社区与服务器节点。</div>
        </div>
        <button className="btn btn-primary" onClick={openCreateModal}>
          配置玩家进服权限
        </button>
      </div>

      <div className="card">
        <div className="card-header" style={{ borderBottom: 'none' }}>
          <div>
            <div className="card-title">特殊权限玩家列表</div>
            <div className="card-sub">此处仅展示被单独配置了允许/禁止进服规则的玩家，其余白名单玩家默认全服通行。</div>
          </div>
        </div>
        <div className="card-body" className="p-0">
          {loading ? (
            <div style={{ padding: 40, textAlign: 'center', color: 'var(--text3)' }}>加载中...</div>
          ) : error ? (
            <div style={{ padding: 40, textAlign: 'center', color: 'var(--accent)' }}>{error}</div>
          ) : rules.length === 0 ? (
            <div style={{ padding: 40, textAlign: 'center', color: 'var(--text3)' }}>暂无特殊权限配置，所有白名单玩家默认可进入所有服务器。</div>
          ) : (
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>玩家</th>
                    <th>进服规则</th>
                    <th>最后修改</th>
                    <th className="text-right">操作</th>
                  </tr>
                </thead>
                <tbody>
                  {rules.map((rule) => (
                    <tr key={rule.id}>
                      <td>
                        <div className="fw-600">{rule.nickname}</div>
                        <div className="steam-id" style={{ fontSize: 11, marginTop: 2 }}>{rule.steamid64}</div>
                      </td>
                      <td style={{ minWidth: 200 }}>{renderAccessTags(rule)}</td>
                      <td className="text-muted-light">{formatChinaDate(rule.updated_at)}</td>
                      <td style={{ textAlign: 'right', whiteSpace: 'nowrap' }}>
                        <div className="action-btn-group">
                          <button className="action-btn action-btn-accent" onClick={() => openEditModal(rule)}>编辑</button>
                          <button className="action-btn action-btn-danger" onClick={() => handleDelete(rule)}>重置</button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>

      {/* ========== 配置弹窗 ========== */}
      {modalOpen && (
        <div className="modal-overlay active" onClick={() => setModalOpen(false)}>
          <div className="modal" style={{ maxWidth: 580 }} onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2 style={{ fontSize: 16, fontWeight: 600 }}>
                {editingRule ? '编辑进服权限' : '配置玩家进服权限'}
              </h2>
              <span style={{ cursor: 'pointer', color: 'var(--text3)', fontSize: 18 }} onClick={() => setModalOpen(false)}>✕</span>
            </div>

            <div className="modal-body">
              {/* ── 选择玩家 ── */}
              <div className="mb-16">
                <label style={{ display: 'block', fontSize: 12.5, fontWeight: 500, marginBottom: 6 }}>选择玩家</label>
                {selectedPlayer ? (
                  <div style={{
                    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                    padding: '10px 14px', background: 'var(--success-bg)', borderRadius: 'var(--r-md)',
                    border: '1px solid var(--success-text)',
                  }}>
                    <div>
                      <span style={{ fontWeight: 600, color: 'var(--success-text)' }}>{selectedPlayer.nickname}</span>
                      <span className="steam-id" style={{ marginLeft: 10, fontSize: 11.5 }}>{selectedPlayer.steamid64}</span>
                    </div>
                    {!editingRule && (
                      <button onClick={clearSelectedPlayer} style={{
                        background: 'none', border: 'none', color: 'var(--text3)', cursor: 'pointer',
                        fontSize: 16, padding: '0 4px', lineHeight: 1,
                      }}>✕</button>
                    )}
                  </div>
                ) : (
                  <div style={{ position: 'relative' }}>
                    <input
                      type="text"
                      className="form-control"
                      value={searchInput}
                      onChange={handleSearchChange}
                      placeholder="搜索白名单玩家（昵称或 SteamID64）"
                      disabled={!!editingRule}
                      onFocus={() => { if (searchInput.trim() && searchResults.length === 0) searchWhitelist(searchInput); }}
                    />
                    {/* 搜索下拉 */}
                    {searchInput.trim() && (
                      <div style={{
                        position: 'absolute', top: '100%', left: 0, right: 0, zIndex: 20,
                        background: 'var(--surface)', border: '1px solid var(--border)',
                        borderRadius: 'var(--r-md)', marginTop: 4, maxHeight: 220,
                        overflowY: 'auto', boxShadow: 'var(--shadow-md)',
                      }}>
                        {searching ? (
                          <div style={{ padding: 16, textAlign: 'center', color: 'var(--text3)', fontSize: 13 }}>搜索中...</div>
                        ) : searchResults.length === 0 ? (
                          <div style={{ padding: 16, textAlign: 'center', color: 'var(--text3)', fontSize: 13 }}>
                            未找到匹配的白名单玩家
                          </div>
                        ) : (
                          searchResults.map((player) => (
                            <div
                              key={player.steamid64}
                              onClick={() => selectPlayer(player)}
                              style={{
                                padding: '10px 14px', cursor: 'pointer',
                                borderBottom: '1px solid var(--border)',
                                display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                                transition: 'background 0.15s',
                              }}
                              onMouseEnter={(e) => e.currentTarget.style.background = 'var(--surface2)'}
                              onMouseLeave={(e) => e.currentTarget.style.background = 'transparent'}
                            >
                              <span className="fw-500">{player.nickname}</span>
                              <span className="steam-id" className="fs-11">{player.steamid64}</span>
                            </div>
                          ))
                        )}
                      </div>
                    )}
                  </div>
                )}
              </div>

              {/* ── 优先级说明 ── */}
              <div style={{
                marginBottom: 16, padding: '10px 14px', borderRadius: 'var(--r-sm)',
                background: 'var(--info-bg)', fontSize: 12.5, color: 'var(--info-text)', lineHeight: 1.6,
              }}>
                优先级：<strong>服务器级 &gt; 社区级 &gt; 全局默认</strong>。未做任何配置时，该玩家可进入所有服务器。
              </div>

              {/* ── 进服规则树 ── */}
              <div className="mb-16">
                <label style={{ display: 'block', fontSize: 12.5, fontWeight: 500, marginBottom: 8 }}>进服规则</label>
                {groups.length === 0 ? (
                  <div style={{ padding: 16, textAlign: 'center', color: 'var(--text3)', fontSize: 13, background: 'var(--surface2)', borderRadius: 'var(--r-md)' }}>
                    暂无社区和服务器，请先在社区管理中创建。
                  </div>
                ) : (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                    {groups.map((group) => {
                      const servers = group.servers ?? [];
                      const communityValue = form.blockedCommunities.includes(group.id)
                        ? 'blocked' : form.allowedCommunities.includes(group.id)
                        ? 'allowed' : 'default';
                      const hasServerOverride = servers.some(
                        (s) => form.allowedServers.includes(s.id) || form.blockedServers.includes(s.id)
                      );
                      const isExpanded = expandedGroups[group.id] ?? false;

                      return (
                        <div key={group.id} style={{
                          border: '1px solid var(--border)', borderRadius: 'var(--r-md)',
                          overflow: 'hidden', background: 'var(--surface)',
                        }}>
                          {/* 社区行 */}
                          <div style={{
                            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                            padding: '10px 14px', background: 'var(--surface2)',
                          }}>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}>
                              <span style={{
                                fontSize: 14, width: 22, height: 22, display: 'flex', alignItems: 'center',
                                justifyContent: 'center', borderRadius: 6, background: 'var(--bg)', flexShrink: 0,
                              }}>
                                {isExpanded ? '📂' : '📁'}
                              </span>
                              <span style={{ fontWeight: 600, fontSize: 13.5, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                {group.name}
                              </span>
                              {hasServerOverride && !isExpanded && (
                                <span style={{ fontSize: 10, color: 'var(--accent2)', background: 'var(--info-bg)', padding: '1px 6px', borderRadius: 8, fontWeight: 500 }}>
                                  有覆盖
                                </span>
                              )}
                            </div>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
                              <AccessSelect
                                value={communityValue}
                                onChange={(v) => setCommunityAccess(group.id, v)}
                                options={[
                                  { value: 'default', label: '默认' },
                                  { value: 'allowed', label: '允许进入' },
                                  { value: 'blocked', label: '禁止进入' },
                                ]}
                              />
                              {servers.length > 0 && (
                                <button onClick={() => toggleGroup(group.id)} style={{
                                  background: 'none', border: '1px solid var(--border)', borderRadius: 6,
                                  padding: '3px 8px', cursor: 'pointer', fontSize: 11, color: 'var(--text2)',
                                  transition: 'all 0.15s',
                                }}>
                                  {isExpanded ? '收起' : `展开 ${servers.length} 台服务器`}
                                </button>
                              )}
                            </div>
                          </div>

                          {/* 服务器列表 */}
                          {isExpanded && servers.length > 0 && (
                            <div style={{ padding: '4px 14px 8px' }}>
                              {servers.map((server) => {
                                const serverValue = form.allowedServers.includes(server.id)
                                  ? 'allowed' : form.blockedServers.includes(server.id)
                                  ? 'blocked' : 'default';
                                return (
                                  <div key={server.id} style={{
                                    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                                    padding: '7px 0', borderBottom: '1px solid var(--border)',
                                  }}>
                                    <div style={{ display: 'flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
                                      <span style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--text3)', flexShrink: 0 }} />
                                      <span style={{ fontSize: 13, fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                        {server.name}
                                      </span>
                                    </div>
                                    <AccessSelect
                                      value={serverValue}
                                      onChange={(v) => setServerAccess(server.id, v)}
                                      size="sm"
                                      options={[
                                        { value: 'default', label: '未设置' },
                                        { value: 'allowed', label: '强制允许' },
                                        { value: 'blocked', label: '强制禁止' },
                                      ]}
                                    />
                                  </div>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>

              {/* ── 效果摘要 ── */}
              <div style={{
                padding: '10px 14px', borderRadius: 'var(--r-md)',
                background: 'var(--surface2)', fontSize: 12.5, color: 'var(--text2)',
                lineHeight: 1.6, border: '1px solid var(--border)',
              }}>
                <span style={{ fontWeight: 600, color: 'var(--text)' }}>效果预览：</span>
                {ruleSummary}
              </div>

              {formError && <div style={{ color: 'var(--accent)', marginTop: 10, fontSize: 13 }}>{formError}</div>}
            </div>

            <div className="modal-footer">
              <button className="btn btn-outline" onClick={() => setModalOpen(false)}>取消</button>
              <button className="btn btn-primary" onClick={handleSubmit} disabled={submitting}>
                {submitting ? '保存中...' : '保存权限配置'}
              </button>
            </div>
          </div>
        </div>
      )}

      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}
