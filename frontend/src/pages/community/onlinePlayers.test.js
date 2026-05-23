import assert from 'node:assert/strict';
import test from 'node:test';
import { onlinePlayerFields, onlinePlayerKey, buildKickCommand, buildBanCommand } from './onlinePlayers.js';

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

test('buildKickCommand generates sm_kick with player name', () => {
  assert.equal(buildKickCommand('Player1'), 'sm_kick "Player1"');
  assert.equal(buildKickCommand('Player1', '作弊'), 'sm_kick "Player1" "作弊"');
  assert.equal(buildKickCommand('别逗你彩姐笑了', 'test'), 'sm_kick "别逗你彩姐笑了" "test"');
});

test('buildKickCommand escapes double quotes in player name', () => {
  assert.equal(buildKickCommand('He said "hi"'), 'sm_kick "He said \'hi\'"');
});

test('buildBanCommand generates correct RCON command', () => {
  assert.equal(buildBanCommand('76561198000000000', 0, '作弊'), 'sm_ban "76561198000000000" 0 "作弊"');
  assert.equal(buildBanCommand('76561198000000000', 60, '恶意行为'), 'sm_ban "76561198000000000" 60 "恶意行为"');
});
