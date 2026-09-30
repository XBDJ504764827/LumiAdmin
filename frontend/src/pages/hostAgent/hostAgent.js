const API_BASE = import.meta.env.VITE_API_BASE ?? '';

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
