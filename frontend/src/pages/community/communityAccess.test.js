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
  });
});

test('buildAccessSummary describes enabled modes', () => {
  assert.equal(buildAccessSummary({ access_restriction_enabled: false, whitelist_mode_enabled: false }), '无限制');
  assert.equal(buildAccessSummary({ access_restriction_enabled: true, min_rating: 1200, min_steam_level: 10, whitelist_mode_enabled: false }), '限制：rating ≥ 1200，Steam 等级 ≥ 10');
  assert.equal(buildAccessSummary({ access_restriction_enabled: false, whitelist_mode_enabled: true }), '白名单模式');
  assert.equal(buildAccessSummary({ access_restriction_enabled: true, min_rating: 1200, min_steam_level: 10, whitelist_mode_enabled: true }), '白名单优先；未通过白名单需 rating ≥ 1200，Steam 等级 ≥ 10');
});
