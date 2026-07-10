import test from 'node:test';
import assert from 'node:assert/strict';
import { ALL_MAPS_RULE, buildRulePayload, ruleToForm, splitThreshold } from './abnormalRuleForm.js';

test('splitThreshold converts seconds to minutes and seconds', () => {
  assert.deepEqual(splitThreshold(125.5), { minutes: 2, seconds: 5.5 });
});

test('buildRulePayload creates an all-maps default rule', () => {
  const result = buildRulePayload({
    scope: 'all_maps',
    map_name: '',
    course: 0,
    mode: '',
    time_type: '',
    threshold_minutes: 2,
    threshold_seconds: 30.5,
    enabled: true,
    note: '',
  });

  assert.equal(result.error, '');
  assert.equal(result.payload.map_name, ALL_MAPS_RULE);
  assert.equal(result.payload.threshold_seconds, 150.5);
});

test('single-map rule overrides are restored into the editor', () => {
  const form = ruleToForm({
    map_name: 'kz_cargo',
    course: 1,
    mode: 'kzt',
    time_type: 'pro',
    threshold_seconds: 65,
    enabled: true,
  });

  assert.equal(form.scope, 'single_map');
  assert.equal(form.map_name, 'kz_cargo');
  assert.equal(form.threshold_minutes, 1);
  assert.equal(form.threshold_seconds, 5);
});

test('seconds part must be lower than 60', () => {
  const result = buildRulePayload({
    scope: 'all_maps',
    threshold_minutes: 1,
    threshold_seconds: 60,
    enabled: true,
  });
  assert.match(result.error, /60/);
});
