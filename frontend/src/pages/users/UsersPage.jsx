import React, { useState } from 'react';
import { api } from '../../lib/api.js';
import { useConfirmDialog } from '../../shared/ConfirmModal.jsx';
import { Modal } from '../../shared/Modal.jsx';
import { useAuth } from '../../state/auth.jsx';
import { useToast, ToastContainer } from '../../shared/Toast.jsx';
import { useApiQuery, useApiMutation } from '../../shared/useApiQuery.js';
import { buildCreateUserPayload, buildUpdateUserPayload, validateCreateUserForm } from './userForm.js';
import { SearchBar } from '../../shared/SearchBar.jsx';
import { Pagination } from '../../shared/Pagination.jsx';
import { formatChinaDateTime } from '../../shared/time.js';

const emptyCreateForm = { username: '', password: '', role: 'normal', steam_id: '', remark: '' };
const emptyEditForm = { id: '', username: '', role: 'normal', steam_id: '', remark: '' };
const emptyPasswordForm = { id: '', password: '', confirmPassword: '', username: '' };

function getRoleLabel(role) {
  if (role === 'developer') return '开发管理员';
  if (role === 'admin') return '系统管理员';
  return '普通管理员';
}

function getAvatarText(displayName, username) {
  return (displayName || username || '?').slice(0, 2).toUpperCase();
}

function roleClass(role) {
  if (role === 'developer') return 'developer';
  if (role === 'admin') return 'admin';
  return 'normal';
}

export function UsersPage() {
  const { session } = useAuth();
  const { confirm, dialog } = useConfirmDialog();
  const { toast, toasts, dismiss: dismissToast } = useToast();
  const token = session?.token ?? null;
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);
  const [createOpen, setCreateOpen] = useState(false);
  const [editOpen, setEditOpen] = useState(false);
  const [passwordOpen, setPasswordOpen] = useState(false);
  const [createForm, setCreateForm] = useState(emptyCreateForm);
  const [editForm, setEditForm] = useState(emptyEditForm);
  const [passwordForm, setPasswordForm] = useState(emptyPasswordForm);
  const [submitting, setSubmitting] = useState(false);

  const isDeveloper = session?.role === 'developer';
  const isAdmin = session?.role === 'admin';
  const isNormal = session?.role === 'normal';
  const canCreate = isDeveloper || isAdmin;

  // 使用 React Query 获取用户列表
  const { data, isLoading, error, refetch } = useApiQuery(
    ['users', { page, search }],
    (token) => api.users(token, { page, page_size: 20, ...(search ? { search } : {}) })
  );

  // 创建用户变更
  const createUserMutation = useApiMutation(
    ({ token, ...body }) => api.createUser(token, body),
    { invalidateQueries: ['users'] }
  );

  // 更新用户变更
  const updateUserMutation = useApiMutation(
    ({ token, id, ...body }) => api.updateUser(token, id, body),
    { invalidateQueries: ['users'] }
  );

  // 删除用户变更
  const deleteUserMutation = useApiMutation(
    ({ token, id }) => api.deleteUser(token, id),
    { invalidateQueries: ['users'] }
  );

  // 更新密码变更
  const updatePasswordMutation = useApiMutation(
    ({ token, id, ...body }) => api.updateUserPassword(token, id, body),
    { invalidateQueries: ['users'] }
  );

  // 切换启用状态变更
  const toggleEnabledMutation = useApiMutation(
    ({ token, id, enabled }) => api.toggleUserEnabled(token, id, { enabled }),
    { invalidateQueries: ['users'] }
  );

  // 撤销会话变更
  const revokeSessionsMutation = useApiMutation(
    ({ token, id }) => api.revokeUserSessions(token, id),
    { invalidateQueries: ['users'] }
  );

  const items = data?.items ?? [];
  const total = data?.total ?? 0;

  function canEditUser(item) {
    if (isDeveloper) return true;
    if (isAdmin) return item.role !== 'developer';
    return isNormal && item.id === session?.userId;
  }

  function canDeleteUser(item) {
    if (item.id === session?.userId) return false;
    if (isDeveloper) return true;
    if (isAdmin) return item.role !== 'developer';
    return false;
  }

  function canChangeRole(item) {
    if (isDeveloper) return true;
    if (isAdmin) return item.role !== 'developer' && item.id !== session?.userId;
    return false;
  }

  function canToggleEnabled(item) {
    if (item.id === session?.userId) return false;
    if (isDeveloper) return true;
    if (isAdmin) return item.role !== 'developer';
    return false;
  }

  function canRevokeSessions(item) {
    if (item.id === session?.userId) return false;
    if (isDeveloper) return true;
    if (isAdmin) return item.role !== 'developer';
    return false;
  }

  function openEditModal(item) {
    setEditForm({
      id: item.id,
      username: item.username,
      role: item.role,
      steam_id: item.steam_id ?? '',
      remark: item.remark ?? '',
    });
    setEditOpen(true);
  }

  function openPasswordModal(item) {
    setPasswordForm({ id: item.id, password: '', confirmPassword: '', username: item.username });
    setPasswordOpen(true);
  }

  async function handleCreate() {
    const validationError = validateCreateUserForm(createForm);
    if (validationError) {
      toast({ title: '创建失败', message: validationError, tone: 'danger' });
      return;
    }

    try {
      setSubmitting(true);
      await createUserMutation.mutateAsync(buildCreateUserPayload(createForm));
      setCreateOpen(false);
      setCreateForm(emptyCreateForm);
      toast({ title: '创建成功', message: `管理员 ${createForm.username} 已创建。` });
    } catch (actionError) {
      toast({ title: '创建失败', message: actionError.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleUpdate() {
    if (!editForm.username.trim()) {
      toast({ title: '保存失败', message: '请输入用户名。', tone: 'danger' });
      return;
    }

    try {
      setSubmitting(true);
      await updateUserMutation.mutateAsync({ 
        id: editForm.id, 
        ...buildUpdateUserPayload(editForm, canChangeRole(editForm)) 
      });
      setEditOpen(false);
      toast({ title: '保存成功', message: `管理员 ${editForm.username} 信息已更新。` });
    } catch (actionError) {
      toast({ title: '保存失败', message: actionError.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleUpdatePassword() {
    if (!passwordForm.password.trim()) {
      toast({ title: '修改失败', message: '请输入新密码。', tone: 'danger' });
      return;
    }
    if (passwordForm.password !== passwordForm.confirmPassword) {
      toast({ title: '修改失败', message: '两次密码输入不一致。', tone: 'danger' });
      return;
    }

    try {
      setSubmitting(true);
      await updatePasswordMutation.mutateAsync({ 
        id: passwordForm.id, 
        password: passwordForm.password.trim() 
      });
      setPasswordOpen(false);
      toast({ title: '修改成功', message: `${passwordForm.username} 的密码已更新。` });
    } catch (actionError) {
      toast({ title: '修改失败', message: actionError.message, tone: 'danger' });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDelete(item) {
    const confirmed = await confirm({
      title: '删除管理员',
      message: `确定删除管理员 ${item.username} 吗？`,
    });
    if (!confirmed) return;

    try {
      await deleteUserMutation.mutateAsync({ id: item.id });
      toast({ title: '删除成功', message: `管理员 ${item.username} 已删除。` });
    } catch (actionError) {
      toast({ title: '删除失败', message: actionError.message, tone: 'danger' });
    }
  }

  async function handleToggleEnabled(item) {
    const action = item.enabled ? '禁用' : '启用';
    const confirmed = await confirm({
      title: `${action}账号`,
      message: `确定${action}管理员 ${item.username} 吗？${item.enabled ? '禁用后该账号将无法登录。' : ''}`,
    });
    if (!confirmed) return;

    try {
      await toggleEnabledMutation.mutateAsync({ id: item.id, enabled: !item.enabled });
      toast({ title: `${action}成功`, message: `管理员 ${item.username} 已${action}。` });
    } catch (actionError) {
      toast({ title: `${action}失败`, message: actionError.message, tone: 'danger' });
    }
  }

  async function handleRevokeSessions(item) {
    const confirmed = await confirm({
      title: '强制登出',
      message: `确定强制登出管理员 ${item.username} 的所有设备吗？该用户需要重新登录才能继续使用系统。`,
    });
    if (!confirmed) return;

    try {
      const result = await revokeSessionsMutation.mutateAsync({ id: item.id });
      toast({ title: '登出成功', message: `已强制登出 ${item.username} 的 ${result.revoked_count} 个会话。` });
    } catch (actionError) {
      toast({ title: '登出失败', message: actionError.message, tone: 'danger' });
    }
  }

  return (
    <div id="users" className="content-section active">
      <div className="breadcrumb"><span>核心管理</span><span className="sep">›</span><span className="current">网站用户管理</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">网站管理员列表</div>
          <div className="page-sub">管理后台系统的登录账号及分配操作权限。</div>
        </div>
        {canCreate ? <button className="btn btn-primary" onClick={() => setCreateOpen(true)}>新增管理员</button> : null}
      </div>

      <SearchBar
        value={search}
        onChange={(v) => { setSearch(v); setPage(1); }}
        placeholder="搜索用户名..."
      />

      <div className="card">
        <div className="card-body p-0">
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr><th>用户名</th><th>权限</th><th>状态</th><th>备注</th><th>steamid</th><th>创建时间</th><th className="text-right">操作</th></tr>
              </thead>
              <tbody>
                {isLoading ? (
                  <tr><td colSpan={7} className="text-muted">正在加载用户列表...</td></tr>
                ) : null}
                {!isLoading && error ? (
                  <tr><td colSpan={7} className="text-accent">{error.message}</td></tr>
                ) : null}
                {!isLoading && !error && items.length === 0 ? (
                  <tr><td colSpan={7} className="text-muted">暂无管理员账号。</td></tr>
                ) : null}
                {!isLoading && !error && items.map((item) => (
                  <tr key={item.id}>
                    <td>
                      <div className="user-cell">
                        <div className="avatar avatar-info">{getAvatarText(item.display_name, item.username)}</div>
                        <div>{item.display_name || item.username}{item.id === session?.userId ? ' (您)' : ''}</div>
                      </div>
                    </td>
                    <td><span className={`role-badge ${roleClass(item.role)}`}>{getRoleLabel(item.role)}</span></td>
                    <td><span className={`status-pill ${item.enabled ? 'pill-online' : 'pill-offline'}`}>{item.enabled ? '已启用' : '已禁用'}</span></td>
                    <td className="text-muted-light">{item.remark ?? '-'}</td>
                    <td className="steam-id">{item.steam_id ?? '-'}</td>
                    <td className="text-muted-light">{formatChinaDateTime(item.created_at)}</td>
                    <td className="text-right">
                      <div className="action-btn-group">
                        {canToggleEnabled(item) ? <button className={`action-btn ${item.enabled ? 'action-btn-danger' : 'action-btn-success'}`} onClick={() => handleToggleEnabled(item)}>{item.enabled ? '禁用' : '启用'}</button> : null}
                        {canRevokeSessions(item) ? <button className="action-btn" onClick={() => handleRevokeSessions(item)}>强制登出</button> : null}
                        {canEditUser(item) ? <button className="action-btn action-btn-accent" onClick={() => openEditModal(item)}>{isNormal ? '编辑我的信息' : '编辑'}</button> : null}
                        {canEditUser(item) ? <button className="action-btn" onClick={() => openPasswordModal(item)}>修改密码</button> : null}
                        {canDeleteUser(item) ? <button className="action-btn action-btn-danger" onClick={() => handleDelete(item)}>删除</button> : null}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <Pagination page={page} pageSize={20} total={total} onChange={setPage} />

      <Modal
        open={createOpen}
        title="新增管理员"
        onClose={() => { setCreateOpen(false); setCreateForm(emptyCreateForm); }}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setCreateOpen(false)}>取消</button>
            <button className="btn btn-primary" onClick={handleCreate} disabled={submitting}>创建</button>
          </>
        )}
      >
        <div className="form-group"><label>用户名</label><input className="form-control" value={createForm.username} onChange={(event) => setCreateForm((prev) => ({ ...prev, username: event.target.value }))} /></div>
        <div className="form-group"><label>密码</label><input type="password" className="form-control" value={createForm.password} onChange={(event) => setCreateForm((prev) => ({ ...prev, password: event.target.value }))} /></div>
        <div className="form-group"><label>权限等级</label><select className="form-control" value={createForm.role} onChange={(event) => setCreateForm((prev) => ({ ...prev, role: event.target.value }))}><option value="admin">系统管理员</option><option value="normal">普通管理员</option></select></div>
        <div className="form-group"><label>steamid</label><input className="form-control" value={createForm.steam_id} onChange={(event) => setCreateForm((prev) => ({ ...prev, steam_id: event.target.value }))} /></div>
        <div className="form-group"><label>备注</label><input className="form-control" value={createForm.remark} onChange={(event) => setCreateForm((prev) => ({ ...prev, remark: event.target.value }))} /></div>
      </Modal>

      <Modal
        open={editOpen}
        title="编辑管理员信息"
        onClose={() => { setEditOpen(false); setEditForm(emptyEditForm); }}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setEditOpen(false)}>取消</button>
            <button className="btn btn-primary" onClick={handleUpdate} disabled={submitting}>保存</button>
          </>
        )}
      >
        <div className="form-group"><label>用户名</label><input className="form-control" value={editForm.username} onChange={(event) => setEditForm((prev) => ({ ...prev, username: event.target.value }))} /></div>
        {canChangeRole(editForm) ? <div className="form-group"><label>权限等级</label><select className="form-control" value={editForm.role} onChange={(event) => setEditForm((prev) => ({ ...prev, role: event.target.value }))}><option value="admin">系统管理员</option><option value="normal">普通管理员</option><option value="developer">开发管理员</option></select></div> : null}
        <div className="form-group"><label>steamid</label><input className="form-control" value={editForm.steam_id} onChange={(event) => setEditForm((prev) => ({ ...prev, steam_id: event.target.value }))} /></div>
        <div className="form-group"><label>备注</label><input className="form-control" value={editForm.remark} onChange={(event) => setEditForm((prev) => ({ ...prev, remark: event.target.value }))} /></div>
      </Modal>

      <Modal
        open={passwordOpen}
        title="修改密码"
        onClose={() => { setPasswordOpen(false); setPasswordForm(emptyPasswordForm); }}
        footer={(
          <>
            <button className="btn btn-outline" onClick={() => setPasswordOpen(false)}>取消</button>
            <button className="btn btn-primary" onClick={handleUpdatePassword} disabled={submitting}>保存</button>
          </>
        )}
      >
        <div className="form-group"><label>新密码</label><input type="password" className="form-control" value={passwordForm.password} onChange={(event) => setPasswordForm((prev) => ({ ...prev, password: event.target.value }))} /></div>
        <div className="form-group"><label>确认密码</label><input type="password" className="form-control" value={passwordForm.confirmPassword} onChange={(event) => setPasswordForm((prev) => ({ ...prev, confirmPassword: event.target.value }))} /></div>
      </Modal>
      {dialog}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}
