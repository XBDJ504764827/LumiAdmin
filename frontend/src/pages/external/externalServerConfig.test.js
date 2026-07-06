import test from 'node:test';
import assert from 'node:assert/strict';
import {
  EXTERNAL_SERVER_DEFAULT_POLL_INTERVAL,
  EXTERNAL_SERVER_MAX_POLL_INTERVAL,
  EXTERNAL_SERVER_MIN_POLL_INTERVAL,
  clampExternalServerPollInterval,
} from './externalServerConfig.js';

test('clampExternalServerPollInterval keeps configured bounds', () => {
  assert.equal(clampExternalServerPollInterval(5), EXTERNAL_SERVER_MIN_POLL_INTERVAL);
  assert.equal(clampExternalServerPollInterval(99999), EXTERNAL_SERVER_MAX_POLL_INTERVAL);
  assert.equal(clampExternalServerPollInterval(''), EXTERNAL_SERVER_DEFAULT_POLL_INTERVAL);
  assert.equal(clampExternalServerPollInterval(120), 120);
});
