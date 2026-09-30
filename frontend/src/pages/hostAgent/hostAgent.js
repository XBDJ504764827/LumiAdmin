const API_BASE = import.meta.env.VITE_API_BASE ?? '';

export const HOST_AGENT_FILES = [
  { name: 'install.sh', description: '一键安装脚本' },
  { name: 'lumi-host-agent.service', description: 'systemd 单元模板' },
  { name: 'lumi-server-agent-x86_64', description: '预编译二进制（待发布）' },
];

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

export function formatBytes(bytes = 0) {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

export function buildInstallCommand({ backendUrl, lgsmDir, instances }) {
  const url = (backendUrl || '').trim().replace(/\/+$/, '');
  return [
    'sudo bash install.sh',
    `--url ${url || '<后端地址>'}`,
    '--token <Agent口令>',
    `--lgsm-dir ${lgsmDir?.trim() || '/home/steam/lgsm'}`,
    `--instances ${instances?.trim() || 'csgoserver,csgo2server'}`,
  ].join(' \\\n  ');
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
