export function normalizeEndpointRows(items = []) {
  return items.map((item) => ({
    module: item.module,
    tone: item.tone,
    name: item.name,
    method: item.method,
    endpoint: item.endpoint,
    description: item.description,
    authRequired: item.auth_required,
    roles: item.roles ?? [],
  }));
}

export function formatEndpointRoles(roles) {
  return roles.includes('guest') ? '公开访问' : roles.join(' / ');
}

export function formatEndpointAuth(authRequired) {
  return authRequired ? '需要登录' : '公开访问';
}

export const defaultWebhookConfig = {
  publicPath: '',
  webhookUrl: '',
  secret: '',
  serverIds: [],
};

export const defaultPlayerApiConfig = {
  maxApiCount: 3,
  intervalSeconds: 30,
  items: [],
};

export function normalizePlayerApiRows(items = []) {
  return items.map((item) => ({
    player: item.player,
    steamId: item.steam_id64,
    ipAddress: item.ip_address,
    serverName: `${item.server_name}:${item.server_port}`,
    status: 'online',
    syncedText: item.reported_at,
  }));
}

export function normalizePlayerApiConfig(config = {}) {
  return {
    maxApiCount: config.max_api_count ?? defaultPlayerApiConfig.maxApiCount,
    intervalSeconds: config.interval_seconds ?? defaultPlayerApiConfig.intervalSeconds,
    items: (config.items ?? []).map((item) => ({
      id: item.id,
      publicPath: item.public_path ?? '',
      webhookUrl: item.webhook_url ?? '',
      secret: item.secret ?? '',
      serverIds: item.server_ids ?? [],
      lastStatus: item.last_status ?? null,
      lastError: item.last_error ?? null,
      lastDispatchedAt: item.last_dispatched_at ?? null,
    })),
  };
}

export function buildWebhookConfigPayload(config) {
  const secret = config.secret.trim();
  return {
    public_path: config.publicPath.trim(),
    webhook_url: config.webhookUrl.trim(),
    secret: secret || null,
    server_ids: config.serverIds,
  };
}

export function buildPlayerApiConfigPayload(config) {
  return {
    max_api_count: Number(config.maxApiCount),
    interval_seconds: Number(config.intervalSeconds),
    items: config.items.map(buildWebhookConfigPayload),
  };
}

export function flattenServerOptions(groups = []) {
  return groups.flatMap((group) => (group.servers ?? []).map((server) => ({
    id: server.id,
    groupId: group.id,
    groupName: group.name,
    label: server.name,
    port: server.port,
  })));
}
