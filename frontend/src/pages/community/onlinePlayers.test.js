import assert from 'node:assert/strict';
import test from 'node:test';
import { onlinePlayerFields, onlinePlayerKey, buildKickCommand, buildBanCommand, steamId64ToSteam2 } from './onlinePlayers.js';

test('onlinePlayerFields exposes structured player details for styled display', () => {
  const player = {
    name: 'BadPlayer',
    steam_id64: '76561198000000000',
    ip: '192.168.1.20',
    ping: 42,
    server_port: 27015,
  };

  assert.equal(onlinePlayerKey(player), '76561198000000000-192.168.1.20');
  assert.deepEqual(onlinePlayerFields(player), [
    { label: 'SteamID64', value: '76561198000000000' },
    { label: 'IP 地址', value: '192.168.1.20' },
    { label: '延迟', value: '42ms' },
    { label: '服务器端口', value: 27015 },
  ]);
});

test('steamId64ToSteam2 converts SteamID64 to STEAM_X:Y:Z format', () => {
  assert.equal(steamId64ToSteam2('76561198000000000'), 'STEAM_0:0:19867136');
  assert.equal(steamId64ToSteam2('76561198298405388'), 'STEAM_0:0:169069830');
  assert.equal(steamId64ToSteam2('76561197960265729'), 'STEAM_0:1:0');
  assert.equal(steamId64ToSteam2('76561197960265728'), 'STEAM_0:0:0');
});

test('buildKickCommand generates correct sm_kick command with SteamID2', () => {
  assert.equal(buildKickCommand('76561198000000000'), 'sm_kick "STEAM_0:0:19867136"');
  assert.equal(buildKickCommand('76561198000000000', '作弊'), 'sm_kick "STEAM_0:0:19867136" "作弊"');
});

test('buildBanCommand generates correct RCON command', () => {
  assert.equal(buildBanCommand('76561198000000000', 0, '作弊'), 'sm_ban "76561198000000000" 0 "作弊"');
  assert.equal(buildBanCommand('76561198000000000', 60, '恶意行为'), 'sm_ban "76561198000000000" 60 "恶意行为"');
});
