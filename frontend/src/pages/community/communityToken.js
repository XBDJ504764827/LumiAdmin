export function buildReportTokenConfigLine(reportToken) {
  return `manger_report_token "${reportToken}"`;
}

export function canManageServerReportToken(role) {
  return role === 'admin' || role === 'developer';
}

export function normalizeReportTokenResponse(response) {
  const reportToken = response?.token?.report_token;
  if (!reportToken) {
    throw new Error('服务器上报 Token 返回为空。');
  }
  return reportToken;
}

export function buildResetReportTokenConfirmMessage(serverName) {
  return `确定重置“${serverName}”的上报 Token 吗？旧 Token 会立即失效，需要更新插件配置文件。`;
}

export function buildResetReportTokenSuccessMessage(serverName) {
  return `${serverName} 的上报 Token 已重置，请复制新 Token 并写入插件配置文件。`;
}
