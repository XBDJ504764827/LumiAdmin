import React, { useCallback, useEffect, useState } from 'react';
import { api } from '../../lib/api.js';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import {
  buildDeleteGroupConfirmMessage,
  buildDeleteGroupFailureMessage,
  buildDeleteGroupSuccessMessage,
} from './communityDelete.js';
import {
  buildResetReportTokenConfirmMessage,
  buildResetReportTokenSuccessMessage,
  canManageServerReportToken,
  normalizeReportTokenResponse,
} from './communityToken.js';
import {
  buildServerPayloadWithAccess,
  buildCommunityAccessPayload,
  emptyAccessConfig,
  emptyCommunityAccessConfig,
  fillAccessConfigFromServer,
  fillCommunityAccessConfig,
  buildAccessSummary,
} from './communityAccess.js';
import { onlinePlayerKey, buildKickCommand, buildBanCommand, BAN_DURATION_OPTIONS, BAN_REASON_OPTIONS } from './onlinePlayers.js';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';

const emptyGroupForm = { name: '' };
const emptyServerForm = {
  name: '',
  ip: '',
  port: '27015',
  rcon_password: '',
  report_token: '',
  note: '',
  max_players: '0',
  ...emptyAccessConfig,
};

function ToggleSwitch({ checked, onChange, disabled = false }) {
  return (
    <label className="toggle-switch">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} disabled={disabled} />
      <span className="toggle-slider" />
    </label>
  );
}

function FormSectionCard({ icon, title, children }) {
  return (
    <div className="form-section-card">
      <div className="form-section-header">
        {icon}
        <span>{title}</span>
      </div>
      {children}
    </div>
  );
}

function ServerRconFeedback({ feedback }) {
  if (feedback.testing) {
    return (
      <div className="alert alert-info">
        <span className="alert-icon">⟳</span>
        <div className="alert-content"><div className="alert-text">正在测试 RCON 连接...</div></div>
      </div>
    );
  }
  if (!feedback.tested || !feedback.message) return null;

  const alertClass = feedback.ok ? 'alert-success' : 'alert-error';
  const iconText = feedback.ok ? '✓' : '✕';
  const titleText = feedback.ok ? '连接成功' : '连接失败';

  return (
    <div className={`alert ${alertClass}`}>
      <span className="alert-icon">{iconText}</span>
      <div className="alert-content">
        <div className="alert-title">{titleText}</div>
        <div className="alert-text">
          {feedback.message}
          {feedback.ok && feedback.players.length > 0 ? ` — 检测到 ${feedback.players.length} 名在线玩家` : ''}
        </div>
      </div>
    </div>
  );
}

export function CommunityPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const token = session?.token ?? null;
  const canMutate = session?.role === 'developer' || session?.role === 'admin';
  const canManageToken = canManageServerReportToken(session?.role);

  const [groups, setGroups] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [groupModalOpen, setGroupModalOpen] = useState(false);
  const [serverModalOpen, setServerModalOpen] = useState(false);
  const [communityAccessModal, setCommunityAccessModal] = useState({ open: false, groupId: null, groupName: '' });
  const [communityAccessForm, setCommunityAccessForm] = useState(emptyCommunityAccessConfig);
  const [submittingCommunityAccess, setSubmittingCommunityAccess] = useState(false);
  const [playersModal, setPlayersModal] = useState({ open: false, serverId: null, serverName: '', players: [], loading: false, error: '' });
  const [groupForm, setGroupForm] = useState(emptyGroupForm);
  const [serverForm, setServerForm] = useState(emptyServerForm);
  const [selectedGroupId, setSelectedGroupId] = useState(null);
  const [editingServerId, setEditingServerId] = useState(null);
  const [serverFeedback, setServerFeedback] = useState({ testing: false, tested: false, ok: false, message: '', players: [] });
  const [submittingGroup, setSubmittingGroup] = useState(false);
  const [submittingServer, setSubmittingServer] = useState(false);
  const [groupError, setGroupError] = useState('');
  const [tokenPanel, setTokenPanel] = useState({ serverId: null, token: '', loading: false, error: '' });

  const loadGroups = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await api.servers(token);
      setGroups(response?.groups ?? []);
    } catch (loadError) {
      setError(loadError);
      setGroups([]);
    } finally {
      setLoading(false);
    }
  }, [token]);

  useEffect(() => { loadGroups(); }, [loadGroups]);

  function openCreateGroupModal() {
    if (!canMutate) return;
    setGroupForm(emptyGroupForm);
    setGroupError('');
    setGroupModalOpen(true);
  }

  function openCreateServerModal(groupId) {
    if (!canMutate) return;
    setSelectedGroupId(groupId);
    setEditingServerId(null);
    setServerForm(emptyServerForm);
    setServerFeedback({ testing: false, tested: false, ok: false, message: '', players: [] });
    setServerModalOpen(true);
  }

  function openEditServerModal(groupId, server) {
    if (!canMutate) return;
    setSelectedGroupId(groupId);
    setEditingServerId(server.id);
    setServerForm({
      name: server.name,
      ip: server.ip,
      port: String(server.port),
      rcon_password: '',
      report_token: server.report_token ?? '',
      note: server.note ?? '',
      max_players: String(server.max_players ?? 0),
      ...fillAccessConfigFromServer(server),
    });
    setServerFeedback({
      testing: false,
      tested: true,
      ok: true,
      message: '修改非密码项无需重新测试 RCON。',
      players: server.players ?? [],
    });
    setServerModalOpen(true);
  }

  function openCommunityAccessModal(group) {
    if (!canMutate) return;
    setCommunityAccessForm(fillCommunityAccessConfig(group));
    setCommunityAccessModal({ open: true, groupId: group.id, groupName: group.name });
  }

  async function handleSaveCommunityAccess() {
    if (!communityAccessModal.groupId) return;
    try {
      setSubmittingCommunityAccess(true);
      const payload = buildCommunityAccessPayload(communityAccessForm);
      await api.updateCommunityAccess(token, communityAccessModal.groupId, payload);
      await loadGroups();
      setCommunityAccessModal({ open: false, groupId: null, groupName: '' });
      toast({ title: '保存成功', message: `社区「${communityAccessModal.groupName}」的访问限制已更新，将应用于所有使用社区统一设置的服务器。` });
    } catch (submitError) {
      toast({ title: '保存失败', message: submitError.message, tone: 'danger' });
    } finally {
      setSubmittingCommunityAccess(false);
    }
  }

  async function handleCreateGroup() {
    const name = groupForm.name.trim();
    if (!name) { setGroupError('请输入社区名称。'); return; }
    try {
      setSubmittingGroup(true);
      setGroupError('');
      await api.createCommunityGroup(token, { name });
      await loadGroups();
      setGroupModalOpen(false);
      setGroupForm(emptyGroupForm);
      toast({ title: '创建成功', message: `社区组「${name}」已成功创建。` });
    } catch (submitError) {
      setGroupError('');
      toast({ title: '创建失败', message: submitError.message, tone: 'danger' });
    } finally {
      setSubmittingGroup(false);
    }
  }

  async function handleTestRcon() {
    let payload;
    try {
      payload = buildServerPayloadWithAccess(serverForm);
    } catch (validationError) {
      setServerFeedback({ testing: false, tested: true, ok: false, message: validationError.message, players: [] });
      return;
    }
    setServerFeedback({ testing: true, tested: false, ok: false, message: '', players: [] });
    try {
      const response = await api.testCommunityServerRcon(token, payload);
      const result = response.result;
      setServerFeedback({ testing: false, tested: true, ok: result.ok, message: result.message, players: result.players ?? [] });
    } catch (testError) {
      setServerFeedback({ testing: false, tested: true, ok: false, message: testError.message, players: [] });
    }
  }

  async function handleSaveServer() {
    if (!selectedGroupId) return;
    const passwordChanged = serverForm.rcon_password.trim() !== '';
    if (passwordChanged || !editingServerId) {
      if (!serverFeedback.tested || !serverFeedback.ok) {
        setServerFeedback((prev) => ({ ...prev, message: '请先完成 RCON 测试并确保通过后再保存。' }));
        return;
      }
    }
    try {
      setSubmittingServer(true);
      const payload = buildServerPayloadWithAccess(serverForm);
      if (editingServerId) {
        await api.updateCommunityServer(token, editingServerId, payload);
      } else {
        await api.createCommunityServer(token, selectedGroupId, payload);
      }
      await loadGroups();
      closeServerModal();
    } catch (submitError) {
      setServerFeedback((prev) => ({ ...prev, ok: false, message: submitError.message }));
    } finally {
      setSubmittingServer(false);
    }
  }

  async function handleDeleteGroup(group) {
    const confirmed = await confirm({ title: '删除社区组', message: buildDeleteGroupConfirmMessage(group) });
    if (!confirmed) return;
    try {
      await api.deleteCommunityGroup(token, group.id);
      await loadGroups();
      toast({ title: '删除成功', message: buildDeleteGroupSuccessMessage(group.name) });
    } catch (deleteError) {
      toast({ title: '删除失败', message: buildDeleteGroupFailureMessage(deleteError.message), tone: 'danger' });
    }
  }

  async function handleDeleteServer(serverId) {
    const confirmed = await confirm({ title: '删除游戏服务器', message: '确定删除这个游戏服务器吗？' });
    if (!confirmed) return;
    try {
      await api.deleteCommunityServer(token, serverId);
      await loadGroups();
      toast({ title: '删除成功', message: '游戏服务器已删除。' });
    } catch (deleteError) {
      toast({ title: '删除失败', message: deleteError.message, tone: 'danger' });
    }
  }

  async function handleViewPlayers(server) {
    setPlayersModal({ open: true, serverId: server.id, serverName: server.name, players: [], loading: true, error: '' });
    try {
      const response = await api.communityServerPlayers(token, server.id);
      setPlayersModal({ open: true, serverId: server.id, serverName: server.name, players: response.details ?? [], loading: false, error: '' });
      await loadGroups();
    } catch (playersError) {
      setPlayersModal({ open: true, serverId: server.id, serverName: server.name, players: [], loading: false, error: playersError.message });
    }
  }

  async function handleViewReportToken(server) {
    if (!canManageToken) return;
    if (tokenPanel.serverId === server.id && tokenPanel.token) {
      setTokenPanel({ serverId: null, token: '', loading: false, error: '' });
      return;
    }
    setTokenPanel({ serverId: server.id, token: '', loading: true, error: '' });
    try {
      const response = await api.serverReportToken(token, server.id);
      setTokenPanel({ serverId: server.id, token: normalizeReportTokenResponse(response), loading: false, error: '' });
    } catch (tokenError) {
      setTokenPanel({ serverId: server.id, token: '', loading: false, error: tokenError.message });
    }
  }

  async function handleCopyReportToken() {
    if (!tokenPanel.token) return;
    try {
      await navigator.clipboard.writeText(tokenPanel.token);
      toast({ title: '复制成功', message: '服务器上报 Token 已复制。' });
    } catch (_copyError) {
      toast({ title: '复制失败', message: '复制失败，请手动复制 Token。', tone: 'danger' });
    }
  }

  async function handleResetReportToken(server) {
    if (!canManageToken) return;
    const confirmed = await confirm({ title: '重置上报 Token', message: buildResetReportTokenConfirmMessage(server.name), confirmText: '确认重置' });
    if (!confirmed) return;
    setTokenPanel({ serverId: server.id, token: '', loading: true, error: '' });
    try {
      const response = await api.resetServerReportToken(token, server.id);
      const reportToken = normalizeReportTokenResponse(response);
      setTokenPanel({ serverId: server.id, token: reportToken, loading: false, error: '' });
      toast({ title: '重置成功', message: buildResetReportTokenSuccessMessage(server.name) });
    } catch (resetError) {
      setTokenPanel({ serverId: server.id, token: '', loading: false, error: resetError.message });
    }
  }

  async function handleKickPlayer(player, reason) {
    if (!playersModal.serverId) return;
    try {
      const command = buildKickCommand(player.steam_id64, reason);
      await api.executeRcon(token, playersModal.serverId, { command });
      toast({ title: '踢出成功', message: `玩家 ${player.name} 已被踢出。` });
      const response = await api.communityServerPlayers(token, playersModal.serverId);
      setPlayersModal((prev) => ({ ...prev, players: response.details ?? [] }));
    } catch (kickError) {
      toast({ title: '踢出失败', message: kickError.message, tone: 'danger' });
    }
  }

  async function handleBanPlayer(player, duration, reason) {
    if (!playersModal.serverId) return;
    const confirmed = await confirm({ title: '封禁玩家', message: `确定要封禁玩家 ${player.name} 吗？时长：${duration === 0 ? '永久' : `${duration} 分钟`}，理由：${reason}`, confirmText: '确认封禁' });
    if (!confirmed) return;
    try {
      await api.createBan(token, { player: player.name, steam_id: player.steam_id64, ban_type: 'steam', ip_address: player.ip, reason, duration_minutes: duration });
      const command = buildKickCommand(player.steam_id64, `被封禁：${reason}`);
      await api.executeRcon(token, playersModal.serverId, { command });
      toast({ title: '封禁成功', message: `玩家 ${player.name} 已被封禁并添加到封禁列表。` });
      const response = await api.communityServerPlayers(token, playersModal.serverId);
      setPlayersModal((prev) => ({ ...prev, players: response.details ?? [] }));
    } catch (banError) {
      toast({ title: '封禁失败', message: banError.message, tone: 'danger' });
    }
  }

  function closeServerModal() {
    setServerModalOpen(false);
    setSelectedGroupId(null);
    setEditingServerId(null);
    setServerForm(emptyServerForm);
    setServerFeedback({ testing: false, tested: false, ok: false, message: '', players: [] });
  }

  function handleServerFieldChange(field, value) {
    setServerForm((prev) => ({ ...prev, [field]: value }));
    if (editingServerId && field !== 'rcon_password') return;
    setServerFeedback((prev) => ({
      ...prev,
      tested: false,
      ok: false,
      message: prev.message ? '配置已变更，请重新测试 RCON。' : '',
    }));
  }

  // 找到服务器所属社区
  function findGroupForServer(serverId) {
    for (const group of groups) {
      if (group.servers.some((s) => s.id === serverId)) return group;
    }
    return null;
  }

  // ── 渲染：社区访问限制设置弹窗 ──
  function renderCommunityAccessModal() {
    return (
      <Modal
        open={communityAccessModal.open}
        title={`社区统一访问限制 — ${communityAccessModal.groupName}`}
        onClose={() => setCommunityAccessModal({ open: false, groupId: null, groupName: '' })}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setCommunityAccessModal({ open: false, groupId: null, groupName: '' })}>取消</button>
            <button className="btn btn-primary" onClick={handleSaveCommunityAccess} disabled={submittingCommunityAccess}>{submittingCommunityAccess ? '保存中...' : '保存'}</button>
          </>
        )}
      >
        <FormSectionCard
          icon={<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2" /><path d="M7 11V7a5 5 0 0 1 10 0v4" /></svg>}
          title="访问限制"
        >
          <div className="form-hint" style={{ marginBottom: 12 }}>
            设置社区统一的访问限制。所有未开启「自定义设置」的服务器将自动使用此配置。
          </div>
          <div className="toggle-row">
            <div>
              <div className="toggle-label">白名单模式</div>
              <div className="toggle-desc">开启后，玩家必须通过白名单审核才能进入服务器</div>
            </div>
            <ToggleSwitch checked={communityAccessForm.whitelist_mode_enabled} onChange={(v) => setCommunityAccessForm((prev) => ({ ...prev, whitelist_mode_enabled: v }))} />
          </div>
          <div className="toggle-row">
            <div>
              <div className="toggle-label">进入限制</div>
              <div className="toggle-desc">按 Rating 和 Steam 等级限制玩家进入</div>
            </div>
            <ToggleSwitch
              checked={communityAccessForm.min_rating !== '0' || communityAccessForm.min_steam_level !== '0'}
              onChange={(v) => setCommunityAccessForm((prev) => ({ ...prev, min_rating: v ? prev.min_rating === '0' ? '1000' : prev.min_rating : '0', min_steam_level: v ? prev.min_steam_level === '0' ? '5' : prev.min_steam_level : '0' }))}
            />
          </div>
          {(communityAccessForm.min_rating !== '0' || communityAccessForm.min_steam_level !== '0') ? (
            <div className="form-row" style={{ marginTop: 8 }}>
              <div className="form-group">
                <label>最低进入 Rating</label>
                <input type="number" min="0" className="form-control" placeholder="0" value={communityAccessForm.min_rating} onChange={(e) => setCommunityAccessForm((prev) => ({ ...prev, min_rating: e.target.value }))} />
              </div>
              <div className="form-group">
                <label>最低 Steam 等级</label>
                <input type="number" min="0" className="form-control" placeholder="0" value={communityAccessForm.min_steam_level} onChange={(e) => setCommunityAccessForm((prev) => ({ ...prev, min_steam_level: e.target.value }))} />
              </div>
            </div>
          ) : null}
        </FormSectionCard>
      </Modal>
    );
  }

  // ── 渲染：服务器编辑弹窗 ──
  function renderServerModal() {
    const currentGroup = selectedGroupId ? groups.find((g) => g.id === selectedGroupId) : null;
    return (
      <Modal
        open={serverModalOpen}
        title={editingServerId ? '编辑游戏服务器' : '添加游戏服务器'}
        onClose={closeServerModal}
        wide
        footer={(
          <>
            <button className="btn btn-outline" onClick={closeServerModal}>取消</button>
            {canMutate ? <button className="btn btn-primary" onClick={handleSaveServer} disabled={submittingServer || !serverFeedback.ok}>{submittingServer ? '保存中...' : '保存'}</button> : null}
          </>
        )}
      >
        {/* 基本信息 */}
        <FormSectionCard
          icon={<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg>}
          title="基本信息"
        >
          <div className="form-group">
            <label>服务器名称</label>
            <input type="text" className="form-control" placeholder="例如：一号服（竞技）" value={serverForm.name} onChange={(e) => handleServerFieldChange('name', e.target.value)} />
          </div>
          <div className="form-row">
            <div className="form-group" style={{ flex: 3 }}>
              <label>服务器 IP</label>
              <input type="text" className="form-control" placeholder="例如：192.168.1.100" value={serverForm.ip} onChange={(e) => handleServerFieldChange('ip', e.target.value)} />
            </div>
            <div className="form-group" style={{ flex: 1 }}>
              <label>端口</label>
              <input type="number" className="form-control" placeholder="27015" value={serverForm.port} onChange={(e) => handleServerFieldChange('port', e.target.value)} />
            </div>
          </div>
          <div className="form-group">
            <label>RCON 密码</label>
            <div style={{ display: 'flex', gap: 8 }}>
              <input type="password" className="form-control" placeholder={editingServerId ? '留空则不修改密码' : 'RCON 密码'} value={serverForm.rcon_password} onChange={(e) => handleServerFieldChange('rcon_password', e.target.value)} />
              {canMutate ? (
                <button className="btn btn-outline" style={{ flexShrink: 0, whiteSpace: 'nowrap' }} onClick={handleTestRcon} disabled={serverFeedback.testing}>
                  {serverFeedback.testing ? '测试中...' : '测试连接'}
                </button>
              ) : null}
            </div>
            {editingServerId ? <div className="form-hint">修改其他配置项无需重新输入密码</div> : null}
          </div>
          <ServerRconFeedback feedback={serverFeedback} />
          <div className="form-group">
            <label>备注</label>
            <input type="text" className="form-control" placeholder="备注（非必填）" value={serverForm.note} onChange={(e) => handleServerFieldChange('note', e.target.value)} />
          </div>
        </FormSectionCard>

        {/* 访问限制 */}
        <FormSectionCard
          icon={<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2" /><path d="M7 11V7a5 5 0 0 1 10 0v4" /></svg>}
          title="访问限制"
        >
          <div className="toggle-row">
            <div>
              <div className="toggle-label">自定义设置</div>
              <div className="toggle-desc">开启后使用此服务器独立的限制值，关闭则继承社区统一设置{currentGroup ? `（Rating ≥ ${currentGroup.min_rating}，Steam 等级 ≥ ${currentGroup.min_steam_level}）` : ''}</div>
            </div>
            <ToggleSwitch checked={serverForm.use_custom_access} onChange={(v) => handleServerFieldChange('use_custom_access', v)} />
          </div>
          {serverForm.use_custom_access ? (
            <>
              <div className="toggle-row">
                <div>
                  <div className="toggle-label">进入限制</div>
                  <div className="toggle-desc">开启后将按条件限制玩家进入</div>
                </div>
                <ToggleSwitch checked={serverForm.access_restriction_enabled} onChange={(v) => handleServerFieldChange('access_restriction_enabled', v)} />
              </div>
              {serverForm.access_restriction_enabled ? (
                <div className="form-row" style={{ marginTop: 8 }}>
                  <div className="form-group">
                    <label>最低进入 Rating</label>
                    <input type="number" min="0" className="form-control" placeholder="0" value={serverForm.min_rating} onChange={(e) => handleServerFieldChange('min_rating', e.target.value)} />
                  </div>
                  <div className="form-group">
                    <label>最低 Steam 等级</label>
                    <input type="number" min="0" className="form-control" placeholder="0" value={serverForm.min_steam_level} onChange={(e) => handleServerFieldChange('min_steam_level', e.target.value)} />
                  </div>
                </div>
              ) : null}
            </>
          ) : null}
          <div className="toggle-row">
            <div>
              <div className="toggle-label">白名单模式</div>
              <div className="toggle-desc">仅白名单玩家可进入此服务器</div>
            </div>
            <ToggleSwitch checked={serverForm.whitelist_mode_enabled} onChange={(v) => handleServerFieldChange('whitelist_mode_enabled', v)} />
          </div>
        </FormSectionCard>
      </Modal>
    );
  }

  // ── 渲染：Token 列 ──
  function renderTokenCell(server) {
    const isViewing = tokenPanel.serverId === server.id && !tokenPanel.loading;
    return (
      <div className="token-display">
        <span className={`token-text ${isViewing && tokenPanel.token ? 'revealed' : ''}`}>
          {isViewing && tokenPanel.token ? tokenPanel.token : '••••••••••••'}
        </span>
        {canManageToken ? (
          <>
            <button className="token-btn" onClick={() => handleViewReportToken(server)}>
              {isViewing ? '隐藏' : '查看'}
            </button>
            {isViewing && tokenPanel.token ? (
              <button className="token-btn token-btn-copy" onClick={handleCopyReportToken}>复制</button>
            ) : null}
          </>
        ) : null}
      </div>
    );
  }

  // ── 渲染：服务器行操作 ──
  function renderServerActions(server) {
    const isOnline = server.status === 'online';
    return (
      <div className="action-btn-group">
        <button className="action-btn" onClick={() => handleViewPlayers(server)} disabled={!isOnline} title={!isOnline ? '服务器离线，无法查看' : ''}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" /></svg>
          玩家
        </button>
        {canMutate ? (
          <>
            <button className="action-btn action-btn-accent" onClick={() => openEditServerModal(server.id, server)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" /><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" /></svg>
              编辑
            </button>
            <button className="action-btn action-btn-danger" onClick={() => handleDeleteServer(server.id)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="3 6 5 6 21 6" /><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" /></svg>
              删除
            </button>
          </>
        ) : null}
      </div>
    );
  }

  return (
    <div id="community" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">社区组管理</span></div>
      <div className="page-header">
        <div><div className="page-title">社区与服务器管理</div><div className="page-sub">管理您旗下各个游戏社区的服务器节点、Token 令牌与配置信息。</div></div>
        {canMutate ? <button className="btn btn-primary" onClick={openCreateGroupModal}>创建社区组</button> : null}
      </div>

      {loading ? (
        <div className="card"><div className="card-body">正在加载社区组数据...</div></div>
      ) : null}

      {error ? (
        <div className="card"><div className="card-body" style={{ color: 'var(--accent)' }}>{error.message}</div></div>
      ) : null}

      {!loading && !error && groups.length === 0 ? (
        <div className="card"><div className="card-body" style={{ color: 'var(--text2)' }}>暂无社区组。</div></div>
      ) : null}

      {groups.map((group) => (
        <div className="card" key={group.id}>
          <div className="card-header" style={{ borderBottom: '1px solid var(--border)', paddingBottom: 16 }}>
            <div>
              <div className="card-title" style={{ fontSize: 16 }}>{group.name}</div>
              <div className="card-sub">
                包含 {group.servers.length} 个游戏服务器节点
                {group.min_rating > 0 || group.min_steam_level > 0 ? (
                  <span style={{ marginLeft: 8, opacity: 0.7 }}>统一限制：Rating ≥ {group.min_rating}，Steam 等级 ≥ {group.min_steam_level}</span>
                ) : null}
              </div>
            </div>
            {canMutate ? (
              <div className="action-btn-group">
                <button className="action-btn" onClick={() => openCommunityAccessModal(group)}>访问限制</button>
                <button className="action-btn" onClick={() => openCreateServerModal(group.id)}>+ 添加服务器</button>
              </div>
            ) : null}
          </div>
          <div className="card-body" style={{ padding: 0 }}>
            <div className="table-responsive">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>服务器名称</th>
                    <th>地址 / 端口</th>
                    <th>Token 令牌</th>
                    <th>状态</th>
                    <th>访问限制</th>
                    <th>当前人数</th>
                    <th style={{ textAlign: 'right' }}>操作</th>
                  </tr>
                </thead>
                <tbody>
                  {group.servers.length === 0 ? (
                    <tr><td colSpan={7} style={{ padding: 20, color: 'var(--text3)' }}>暂无服务器。</td></tr>
                  ) : (
                    group.servers.map((server) => {
                      const isOnline = server.status === 'online';
                      const playerCount = server.online_player_count ?? server.players?.length ?? 0;
                      const maxPlayers = server.max_players ?? 0;
                      return (
                        <tr key={server.id}>
                          <td style={{ fontWeight: 600 }}>{server.name}</td>
                          <td className="steam-id">{server.ip}:{server.port}</td>
                          <td>{renderTokenCell(server)}</td>
                          <td>
                            <span className={`status-pill ${isOnline ? 'pill-online' : 'pill-offline'}`}>
                              {isOnline ? '在线' : '离线'}
                            </span>
                          </td>
                          <td style={{ fontSize: 12, color: 'var(--text3)' }}>{buildAccessSummary(server, group)}</td>
                          <td>
                            {isOnline ? `${playerCount} / ${maxPlayers}` : <span style={{ color: 'var(--text3)' }}>0 / 0</span>}
                          </td>
                          <td style={{ textAlign: 'right' }}>{renderServerActions(server)}</td>
                        </tr>
                      );
                    })
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      ))}

      {/* 创建社区组弹窗 */}
      <Modal
        open={groupModalOpen}
        title="创建新社区组"
        onClose={() => setGroupModalOpen(false)}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setGroupModalOpen(false)}>取消</button>
            <button className="btn btn-primary" onClick={handleCreateGroup} disabled={submittingGroup}>{submittingGroup ? '创建中...' : '立即创建'}</button>
          </>
        )}
      >
        <div className="form-group"><label>社区名称</label><input type="text" className="form-control" placeholder="社区名称" value={groupForm.name} onChange={(e) => setGroupForm({ name: e.target.value })} /></div>
        {groupError ? <div style={{ color: 'var(--accent)' }}>{groupError}</div> : null}
      </Modal>

      {/* 社区访问限制设置弹窗 */}
      {renderCommunityAccessModal()}

      {/* 服务器编辑弹窗 */}
      {renderServerModal()}

      {/* 在线玩家弹窗 */}
      <Modal
        open={playersModal.open}
        title={`在线玩家 — ${playersModal.serverName}`}
        onClose={() => setPlayersModal({ open: false, serverId: null, serverName: '', players: [], loading: false, error: '' })}
        footer={<button className="btn btn-primary" onClick={() => setPlayersModal({ open: false, serverId: null, serverName: '', players: [], loading: false, error: '' })}>关闭</button>}
      >
        {playersModal.loading ? (
          <div className="online-player-loading">
            <div>⟳</div>
            <div>正在获取在线玩家...</div>
          </div>
        ) : null}
        {!playersModal.loading && playersModal.error ? (
          <div className="alert alert-error">
            <span className="alert-icon">✕</span>
            <div className="alert-content"><div className="alert-text">{playersModal.error}</div></div>
          </div>
        ) : null}
        {!playersModal.loading && !playersModal.error && playersModal.players.length === 0 ? (
          <div className="online-player-empty">
            <div className="online-player-empty-icon">👥</div>
            <div className="online-player-empty-text">当前没有在线玩家</div>
          </div>
        ) : null}
        {!playersModal.loading && !playersModal.error && playersModal.players.length > 0 ? (
          <>
            <div className="online-player-header">
              <div className="online-player-header-left">
                <svg className="online-player-header-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" /></svg>
                <span style={{ fontSize: 13, fontWeight: 500, color: 'var(--text2)' }}>玩家列表</span>
              </div>
              <span className="online-player-count">{playersModal.players.length} 人在线</span>
            </div>
            <div className="online-player-list">
              {playersModal.players.map((player) => (
                <OnlinePlayerCard
                  key={onlinePlayerKey(player)}
                  player={player}
                  canOperate={canMutate}
                  onKick={(reason) => handleKickPlayer(player, reason)}
                  onBan={(duration, reason) => handleBanPlayer(player, duration, reason)}
                />
              ))}
            </div>
          </>
        ) : null}
      </Modal>
      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

function OnlinePlayerCard({ player, canOperate, onKick, onBan }) {
  const [showKickForm, setShowKickForm] = React.useState(false);
  const [kickReason, setKickReason] = React.useState('');
  const [showBanForm, setShowBanForm] = React.useState(false);
  const [banDuration, setBanDuration] = React.useState(0);
  const [banReason, setBanReason] = React.useState(BAN_REASON_OPTIONS[0]);

  const initial = (player.name || '?')[0].toUpperCase();

  function handleKick() {
    if (!kickReason.trim()) return;
    onKick(kickReason.trim());
    setShowKickForm(false);
    setKickReason('');
  }

  function handleBan() {
    onBan(banDuration, banReason);
    setShowBanForm(false);
  }

  return (
    <div className="online-player-card">
      <div className="online-player-row">
        <div className="online-player-info">
          <div className="online-player-avatar">{initial}</div>
          <div className="online-player-detail">
            <span className="online-player-name">{player.name}</span>
            <div className="online-player-meta">
              <span className="online-player-tag">{player.steam_id64}</span>
              <span className="online-player-tag">{player.ip}</span>
              <span className="online-player-tag online-player-tag-tag-ping">{player.ping}ms</span>
            </div>
          </div>
        </div>
        {canOperate ? (
          <div className="online-player-actions">
            <button className="btn btn-outline btn-sm" onClick={() => setShowKickForm(!showKickForm)}>{showKickForm ? '取消' : '踢出'}</button>
            <button className="btn btn-outline btn-sm" style={{ color: 'var(--accent)' }} onClick={() => setShowBanForm(!showBanForm)}>{showBanForm ? '取消' : '封禁'}</button>
          </div>
        ) : null}
      </div>
      {showKickForm ? (
        <div className="online-player-ban-form">
          <div className="ban-form-row">
            <div className="ban-form-field" style={{ flex: 1 }}><label>踢出理由</label><input type="text" placeholder="请输入踢出理由" value={kickReason} onChange={(e) => setKickReason(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') handleKick(); }} /></div>
            <button className="btn btn-danger btn-sm" onClick={handleKick} disabled={!kickReason.trim()}>确认踢出</button>
          </div>
        </div>
      ) : null}
      {showBanForm ? (
        <div className="online-player-ban-form">
          <div className="ban-form-row">
            <div className="ban-form-field"><label>时长</label><select value={banDuration} onChange={(e) => setBanDuration(Number(e.target.value))}>{BAN_DURATION_OPTIONS.map((opt) => (<option key={opt.value} value={opt.value}>{opt.label}</option>))}</select></div>
            <div className="ban-form-field"><label>理由</label><select value={banReason} onChange={(e) => setBanReason(e.target.value)}>{BAN_REASON_OPTIONS.map((reason) => (<option key={reason} value={reason}>{reason}</option>))}</select></div>
            <button className="btn btn-danger btn-sm" onClick={handleBan}>确认封禁</button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
