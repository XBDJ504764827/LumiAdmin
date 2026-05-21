import test from 'node:test';
import assert from 'node:assert/strict';
import { banModalSubmitText, banModalTitle, banRecordAction, buildBanFormFromRecord, buildCreateBanPayload, emptyBanForm, validateBanForm } from './banForm.js';

test('validateBanForm requires steamid64', () => {
  assert.equal(validateBanForm({ ...emptyBanForm, steam_id: '   ', ban_type: 'steam', reason: '作弊' }), '请输入 SteamID64。');
});

test('validateBanForm requires ban type', () => {
  assert.equal(validateBanForm({ ...emptyBanForm, steam_id: '76561198000000000', ban_type: '', reason: '作弊' }), '请选择封禁属性。');
});

test('validateBanForm rejects unsupported ban type', () => {
  assert.equal(validateBanForm({ ...emptyBanForm, steam_id: '76561198000000000', ban_type: 'hardware', reason: '作弊' }), '封禁属性无效。');
});

test('validateBanForm requires reason', () => {
  assert.equal(validateBanForm({ ...emptyBanForm, steam_id: '76561198000000000', ban_type: 'ip', reason: '   ' }), '请输入封禁理由。');
});

test('buildCreateBanPayload converts optional empty fields to null', () => {
  const payload = buildCreateBanPayload({
    player: '   ',
    steam_id: ' 76561198000000000 ',
    ban_type: 'ip',
    ip_address: '   ',
    reason: ' 重复违规 ',
  });

  assert.deepEqual(payload, {
    player: null,
    steam_id: '76561198000000000',
    ban_type: 'ip',
    ip_address: null,
    reason: '重复违规',
  });
});

test('buildCreateBanPayload keeps optional player and ip when provided', () => {
  const payload = buildCreateBanPayload({
    player: 'Alex',
    steam_id: '76561198000000001',
    ban_type: 'steam',
    ip_address: '192.168.1.5',
    reason: '恶意行为',
  });

  assert.equal(payload.player, 'Alex');
  assert.equal(payload.ip_address, '192.168.1.5');
});


test('inactive ban records expose reban action and reuse record fields', () => {
  const record = {
    id: 'ban-1',
    status: 'inactive',
    player: 'Alex',
    steam_id: '76561198000000000',
    ban_type: 'steam',
    ip_address: '192.168.1.20',
    reason: '作弊',
  };

  assert.equal(banRecordAction(record), 'reban');
  assert.deepEqual(buildBanFormFromRecord(record), {
    player: 'Alex',
    steam_id: '76561198000000000',
    ban_type: 'steam',
    ip_address: '192.168.1.20',
    reason: '作弊',
  });
  assert.equal(banModalTitle('reban'), '重新封禁玩家');
  assert.equal(banModalSubmitText('reban', false), '确认重新封禁');
});

test('active ban records expose unban action', () => {
  assert.equal(banRecordAction({ status: 'active' }), 'unban');
});
