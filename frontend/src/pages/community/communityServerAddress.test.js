import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildServerAddressCopyValue,
  buildServerAddressViewModel,
  copyServerAddress,
  createServerAddressCopyFeedback,
} from './communityServerAddress.js';

const sampleServer = {
  id: 'server-1',
  ip: '10.0.0.8',
  port: 27015,
};

test('buildServerAddressCopyValue joins ip and port', () => {
  assert.equal(buildServerAddressCopyValue(sampleServer), '10.0.0.8:27015');
});

test('buildServerAddressViewModel builds three-line address data without feedback', () => {
  assert.deepEqual(
    buildServerAddressViewModel(sampleServer, '限制：rating ≥ 1200，Steam 等级 ≥ 10'),
    {
      copyValue: '10.0.0.8:27015',
      ipText: '10.0.0.8',
      portText: 'Port 27015',
      accessSummary: '限制：rating ≥ 1200，Steam 等级 ≥ 10',
      feedbackMessage: '',
      feedbackTone: '',
    },
  );
});

test('createServerAddressCopyFeedback returns copied state for success', () => {
  assert.deepEqual(
    createServerAddressCopyFeedback('server-1', 'success'),
    {
      serverId: 'server-1',
      tone: 'success',
      message: '已复制',
    },
  );
});

test('createServerAddressCopyFeedback returns inline failure state for error', () => {
  assert.deepEqual(
    createServerAddressCopyFeedback('server-1', 'error'),
    {
      serverId: 'server-1',
      tone: 'error',
      message: '复制失败',
    },
  );
});

test('copyServerAddress writes ip and port to the provided clipboard writer', async () => {
  let copied = '';

  const result = await copyServerAddress(sampleServer, async (value) => {
    copied = value;
  });

  assert.equal(copied, '10.0.0.8:27015');
  assert.equal(result, '10.0.0.8:27015');
});

test('copyServerAddress forwards clipboard failures', async () => {
  await assert.rejects(
    () => copyServerAddress(sampleServer, async () => {
      throw new Error('clipboard blocked');
    }),
    /clipboard blocked/,
  );
});
