import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildPlayerApiConfigPayload,
  buildWebhookConfigPayload,
  defaultPlayerApiConfig,
  defaultWebhookConfig,
  flattenServerOptions,
  formatEndpointAuth,
  formatEndpointRoles,
  normalizeEndpointRows,
  normalizePlayerApiConfig,
  normalizePlayerApiRows,
} from './apiPages.js';

test('normalizeEndpointRows preserves backend endpoint metadata', () => {
  const rows = normalizeEndpointRows([
    { module: 'API 文档', tone: 'info', name: '接口元数据列表', method: 'GET', endpoint: '/api/docs/endpoints', description: '获取接口元数据', auth_required: true, roles: ['admin', 'developer'] },
    { module: '公共展示', tone: 'online', name: '白名单公示', method: 'GET', endpoint: '/api/public/whitelist', description: '公共页面获取白名单', auth_required: false, roles: ['guest'] },
  ]);

  assert.deepEqual(rows[0], {
    module: 'API 文档',
    tone: 'info',
    name: '接口元数据列表',
    method: 'GET',
    endpoint: '/api/docs/endpoints',
    description: '获取接口元数据',
    authRequired: true,
    roles: ['admin', 'developer'],
  });
  assert.equal(rows[1].authRequired, false);
});

test('formatEndpointAuth displays whether login is required', () => {
  assert.equal(formatEndpointAuth(true), '需要登录');
  assert.equal(formatEndpointAuth(false), '公开访问');
});

test('formatEndpointRoles displays guest and admin roles', () => {
  assert.equal(formatEndpointRoles(['guest']), '公开访问');
  assert.equal(formatEndpointRoles(['admin', 'developer']), 'admin / developer');
});

test('buildWebhookConfigPayload trims publicPath and converts empty secret to null', () => {
  const payload = buildWebhookConfigPayload({
    ...defaultWebhookConfig,
    publicPath: ' my-hook ',
    webhookUrl: ' https://api.example.com/webhook ',
    secret: '   ',
    serverIds: ['server-1', 'server-2'],
  });

  assert.deepEqual(payload, {
    public_path: 'my-hook',
    webhook_url: 'https://api.example.com/webhook',
    secret: null,
    server_ids: ['server-1', 'server-2'],
    external_server_ids: [],
    enabled: true,
    public_access: true,
  });
});

test('normalizePlayerApiRows maps real backend player rows and does not use mock rows', () => {
  const rows = normalizePlayerApiRows([
    {
      player: 'Alice',
      steam_id64: '76561198000000001',
      ip_address: '203.0.113.10',
      server_name: '一号服',
      server_port: 25575,
      reported_at: '2026-04-27T13:00:00Z',
    },
  ]);

  assert.deepEqual(rows, [{
    player: 'Alice',
    steamId: '76561198000000001',
    ipAddress: '203.0.113.10',
    serverName: '一号服:25575',
    status: 'online',
    syncedText: '2026-04-27T13:00:00Z',
  }]);
});

test('normalizePlayerApiConfig maps backend config and max api count', () => {
  assert.deepEqual(normalizePlayerApiConfig({
    max_api_count: 3,
    interval_seconds: 45,
    items: [{
      id: 'webhook-1',
      public_path: 'my-hook',
      webhook_url: 'https://a.test',
      secret: null,
      server_ids: [],
      last_status: 'success',
      last_error: null,
      last_dispatched_at: '2026-04-27T13:00:00Z',
    }],
  }), {
    maxApiCount: 3,
    intervalSeconds: 45,
    items: [{
      id: 'webhook-1',
      publicPath: 'my-hook',
      webhookUrl: 'https://a.test',
      secret: '',
      serverIds: [],
      externalServerIds: [],
      enabled: true,
      publicAccess: true,
      lastStatus: 'success',
      lastError: null,
      lastDispatchedAt: '2026-04-27T13:00:00Z',
    }],
  });
});

test('buildPlayerApiConfigPayload builds full replacement backend payload', () => {
  const payload = buildPlayerApiConfigPayload({
    maxApiCount: 2,
    intervalSeconds: 60,
    items: [{
      publicPath: ' my-hook ',
      webhookUrl: ' https://api.example.com/a ',
      secret: ' secret ',
      serverIds: ['server-1'],
    }],
  });

  assert.deepEqual(payload, {
    max_api_count: 2,
    interval_seconds: 60,
    items: [{
      public_path: 'my-hook',
      webhook_url: 'https://api.example.com/a',
      secret: 'secret',
      server_ids: ['server-1'],
      external_server_ids: [],
      enabled: true,
      public_access: true,
    }],
  });
});

test('flattenServerOptions maps community groups to checkbox options', () => {
  assert.deepEqual(flattenServerOptions([
    { id: 'group-1', name: '社区A', servers: [{ id: 'server-1', name: '一号服', port: 27015 }, { id: 'server-2', name: '二号服', port: 27016 }] },
  ]), [
    { id: 'server-1', groupId: 'group-1', groupName: '社区A', label: '一号服', port: 27015 },
    { id: 'server-2', groupId: 'group-1', groupName: '社区A', label: '二号服', port: 27016 },
  ]);
});

test('defaultPlayerApiConfig exposes safe defaults', () => {
  assert.equal(defaultPlayerApiConfig.maxApiCount, 5);
  assert.equal(defaultPlayerApiConfig.intervalSeconds, 30);
  assert.deepEqual(defaultPlayerApiConfig.items, []);
});
