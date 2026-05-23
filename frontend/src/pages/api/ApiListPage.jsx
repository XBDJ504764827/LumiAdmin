import React from 'react';
import { useAuth } from '../../state/auth.jsx';
import { api } from '../../lib/api.js';
import { useAsync } from '../../shared/useAsync.js';
import { normalizeEndpointRows } from './apiPages.js';

const methodClass = {
  GET: 'method-get',
  POST: 'method-post',
  PUT: 'method-put',
  DELETE: 'method-delete',
};

const toneClass = {
  info: 'pill-info',
  online: 'pill-online',
  danger: 'pill-danger',
  warning: 'pill-warning',
  success: 'pill-online',
};

export function ApiListPage() {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const { data, error, loading } = useAsync(() => api.docsEndpoints(token), [token]);
  const endpointRows = normalizeEndpointRows(data?.items ?? []);

  return (
    <div id="docs-api" className="content-section active">
      <div className="breadcrumb"><span>系统功能</span><span className="sep">›</span><span className="current">API接口列表</span></div>
      <div className="page-header">
        <div>
          <div className="page-title">后端 API 接口文档</div>
          <div className="page-sub">详细说明网站后端提供的各个接口、请求方式及功能作用。</div>
        </div>
      </div>

      <div className="card">
        <div className="card-body" style={{ padding: 0 }}>
          <div className="table-responsive">
            <table className="data-table">
              <thead>
                <tr>
                  <th>所属模块</th>
                  <th>接口名称 / 功能</th>
                  <th>请求方式</th>
                  <th>路由地址 (Endpoint)</th>
                  <th>描述说明</th>
                </tr>
              </thead>
              <tbody>
                {loading && <tr><td colSpan="5" style={{ textAlign: 'center', color: 'var(--text2)' }}>正在加载 API 接口列表...</td></tr>}
                {error && <tr><td colSpan="5" style={{ textAlign: 'center', color: 'var(--accent)' }}>{error.message}</td></tr>}
                {!loading && !error && endpointRows.length === 0 && <tr><td colSpan="5" style={{ textAlign: 'center', color: 'var(--text2)' }}>暂无 API 接口数据</td></tr>}
                {!loading && !error && endpointRows.map((row) => (
                  <tr key={`${row.method}-${row.endpoint}`}>
                    <td><span className={`status-pill ${toneClass[row.tone] || 'pill-info'}`}>{row.module}</span></td>
                    <td style={{ fontWeight: 500 }}>{row.name}</td>
                    <td><span className={`method-badge ${methodClass[row.method]}`}>{row.method}</span></td>
                    <td className="steam-id">{row.endpoint}</td>
                    <td style={{ color: 'var(--text2)' }}>{row.description}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
}
