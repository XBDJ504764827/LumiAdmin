import test from 'node:test';
import assert from 'node:assert/strict';
import {
  actorFieldLabel,
  categoryIconLabel,
  categoryKind,
  categoryLabel,
  categoryTone,
  latestWhitelistContact,
} from './playerDetailFormat.js';

test('时间线分类标签 / 配色 / 图标覆盖新增事件源', () => {
  assert.equal(categoryLabel('qq'), 'QQ');
  assert.equal(categoryLabel('internal'), '内部备注');
  assert.equal(categoryLabel('global_ban'), '全球封禁');
  assert.equal(categoryLabel('abnormal'), '异常成绩');
  assert.equal(categoryTone('ban'), 'danger');
  assert.equal(categoryTone('global_ban'), 'danger');
  assert.equal(categoryTone('qq'), 'info');
  assert.equal(categoryIconLabel('session'), '服');
  assert.equal(categoryIconLabel('unknown_category'), '事');
});

test('分类状态色：全球封禁与失败同为 danger', () => {
  assert.equal(categoryKind('global_ban', 'active'), 'danger');
  assert.equal(categoryKind('access', 'failed'), 'danger');
  assert.equal(categoryKind('access', 'success'), 'success');
  assert.equal(categoryKind('abnormal', 'pending'), 'warning');
});

test('actor 语义：玩家侧事件使用玩家名标签', () => {
  assert.equal(actorFieldLabel('session'), '玩家名');
  assert.equal(actorFieldLabel('abnormal'), '玩家名');
  assert.equal(actorFieldLabel('ban'), '操作人');
  assert.equal(actorFieldLabel('internal'), '操作人');
});

test('联系方式优先使用 profile.contact', () => {
  assert.equal(latestWhitelistContact({ profile: { contact: 'QQ 12345' } }), 'QQ 12345');
  assert.equal(latestWhitelistContact({ whitelist: [{ contact: 'wechat:abc' }] }), 'wechat:abc');
  assert.equal(latestWhitelistContact({}), null);
});
