export function buildServerAddressCopyValue(server) {
  return `${server.ip}:${server.port}`;
}

export function buildServerAddressViewModel(server, accessSummary, copyFeedback = null) {
  const feedbackMatches = copyFeedback?.serverId === server.id;

  return {
    copyValue: buildServerAddressCopyValue(server),
    ipText: server.ip,
    portText: `Port ${server.port}`,
    accessSummary,
    feedbackMessage: feedbackMatches ? copyFeedback.message : '',
    feedbackTone: feedbackMatches ? copyFeedback.tone : '',
  };
}

export function createServerAddressCopyFeedback(serverId, tone) {
  return {
    serverId,
    tone,
    message: tone === 'success' ? '已复制' : '复制失败',
  };
}

export async function copyServerAddress(server, writeText) {
  const copyValue = buildServerAddressCopyValue(server);
  await writeText(copyValue);
  return copyValue;
}
