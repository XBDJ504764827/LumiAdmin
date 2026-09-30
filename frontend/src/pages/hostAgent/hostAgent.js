// vite 下为配置的 API 前缀；纯 node 单测时 import.meta.env 不存在，兜底空串。
const API_BASE = import.meta.env?.VITE_API_BASE ?? '';

// 演示数据：电源操作记录接口未上线前用于预览设计，接入真实日志后删除。
// 字段对齐操作日志页：操作人 / 模块 / 操作动作 / 目标详情 / 操作IP / 操作时间。
export const MOCK_POWER_LOGS = [
  { id: 'log-1', operator: 'devadmin', module: '服务器控制', action: '重启服务器', target: '1服（csgoserver）', result: 'success', ip: '192.168.0.10', time: '2 分钟前' },
  { id: 'log-2', operator: 'admin', module: '服务器控制', action: '强制重启服务器', target: '2服（csgo2server）', result: 'success', ip: '192.168.0.11', time: '15 分钟前' },
  { id: 'log-3', operator: 'admin', module: '服务器控制', action: '关闭服务器', target: '3服（csgo3server）', result: 'success', ip: '192.168.0.11', time: '1 小时前' },
  { id: 'log-4', operator: 'normal', module: '服务器控制', action: '开启服务器', target: '2服（csgo2server）', result: 'success', ip: '192.168.0.12', time: '3 小时前' },
  { id: 'log-5', operator: 'admin', module: '服务器控制', action: '重启服务器', target: '1服（csgoserver）', result: 'blocked', ip: '192.168.0.11', time: '昨天 21:04' },
];

export const POWER_LOG_RESULT_META = {
  success: { label: '已执行', className: 'pill-online' },
  blocked: { label: '已拦截', className: 'pill-warning' },
  failed: { label: '失败', className: 'pill-danger' },
};

export function normalizeSetupResponse(payload) {
  const files = Array.isArray(payload?.files) ? payload.files : [];
  return {
    version: payload?.version ?? '',
    files: files.map((item) => ({
      name: item.name ?? '',
      sizeBytes: item.size_bytes ?? 0,
      available: item.available === true,
      description: item.description ?? '',
    })),
  };
}

export function downloadUrl(filename) {
  return `${API_BASE}/api/host-agent/download/${encodeURIComponent(filename)}`;
}

export async function downloadHostAgentFile(token, filename) {
  const response = await fetch(downloadUrl(filename), {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (!response.ok) {
    const payload = await response.json().catch(() => ({}));
    throw new Error(payload.error || `下载失败（${response.status}）`);
  }
  const blob = await response.blob();
  const url = window.URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  window.URL.revokeObjectURL(url);
}
