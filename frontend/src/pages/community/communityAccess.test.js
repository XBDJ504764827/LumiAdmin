import assert from 'node:assert/strict';
import test from 'node:test';
import {
  buildAccessSummary,
  buildServerPayloadWithAccess,
  emptyAccessConfig,
  fillAccessConfigFromServer,
  validateAccessConfig,
} from './communityAccess.js';

test('buildServerPayloadWithAccess trims base fields and converts access values', () => {
  const payload = buildServerPayloadWithAccess({
    name: ' 准入服 ',
    ip: ' 127.0.0.1 ',
    port: '27015',
    rcon_password: ' secret ',
    report_token: ' token ',
    note: ' note ',
    access_restriction_enabled: true,
    min_rating: '1500',
    min_steam_level: '12',
    whitelist_mode_enabled: true,
    cs_prime_enabled: true,
    max_players: '32',
    use_custom_access: true,
  });

  assert.deepEqual(payload, {
    name: '准入服',
    ip: '127.0.0.1',
    port: 27015,
    rcon_password: 'secret',
    report_token: 'token',
    note: 'note',
    access_restriction_enabled: true,
    min_rating: 1500,
    min_steam_level: 12,
    whitelist_mode_enabled: true,
    cs_prime_enabled: true,
    max_players: 32,
    use_custom_access: true,
  });
});

test('validateAccessConfig rejects negative values', () => {
  assert.equal(validateAccessConfig({ ...emptyAccessConfig, min_rating: '-1' }), '最低进入 rating 不能为负数。');
  assert.equal(validateAccessConfig({ ...emptyAccessConfig, min_rating: '0', min_steam_level: '-1' }), '最低 Steam 等级不能为负数。');
});

test('fillAccessConfigFromServer maps missing values to defaults', () => {
  assert.deepEqual(fillAccessConfigFromServer({ name: 'A', ip: '1.1.1.1', port: 27015, rcon_password: 'x' }), {
    access_restriction_enabled: false,
    min_rating: '0',
    min_steam_level: '0',
    whitelist_mode_enabled: false,
    cs_prime_enabled: false,
    use_custom_access: false,
  });
});

test('buildAccessSummary describes enabled modes', () => {
  assert.equal(buildAccessSummary({ use_custom_access: true, access_restriction_enabled: false, whitelist_mode_enabled: false, cs_prime_enabled: false }), '无限制');
  assert.equal(buildAccessSummary({ use_custom_access: true, access_restriction_enabled: true, min_rating: 1200, min_steam_level: 10, whitelist_mode_enabled: false, cs_prime_enabled: false }), '限制：rating ≥ 1200，Steam 等级 ≥ 10');
  assert.equal(buildAccessSummary({ use_custom_access: true, access_restriction_enabled: false, whitelist_mode_enabled: true, cs_prime_enabled: false }), '白名单模式');
  assert.equal(buildAccessSummary({ use_custom_access: true, access_restriction_enabled: false, whitelist_mode_enabled: false, cs_prime_enabled: true }), 'CS优先账户');
  assert.equal(buildAccessSummary({ use_custom_access: true, access_restriction_enabled: true, min_rating: 1200, min_steam_level: 10, whitelist_mode_enabled: true, cs_prime_enabled: false }), '满足限制即可进；不满足需通过白名单（rating ≥ 1200，Steam 等级 ≥ 10）');
  assert.equal(buildAccessSummary({ use_custom_access: true, access_restriction_enabled: false, whitelist_mode_enabled: true, cs_prime_enabled: true }), 'CS优先账户 或 白名单');
  assert.equal(buildAccessSummary({ use_custom_access: false }, { min_rating: 1200, min_steam_level: 10 }), '限制：rating ≥ 1200，Steam 等级 ≥ 10（社区）');
  assert.equal(buildAccessSummary({ use_custom_access: false }, { cs_prime_enabled: true }), 'CS优先账户（社区）');
});
