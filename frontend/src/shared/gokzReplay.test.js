import test from 'node:test';
import assert from 'node:assert/strict';
import { parseGokzReplay } from './gokzReplay.js';

function replayFixture() {
  const bytes = [];
  const pushU8 = (value) => bytes.push(value & 0xff);
  const pushI32 = (value) => {
    const data = new Uint8Array(4);
    new DataView(data.buffer).setInt32(0, value, true);
    bytes.push(...data);
  };
  const pushF32 = (value) => {
    const data = new Uint8Array(4);
    new DataView(data.buffer).setFloat32(0, value, true);
    bytes.push(...data);
  };
  const pushString = (value) => {
    const data = new TextEncoder().encode(value);
    pushU8(data.length);
    bytes.push(...data);
  };
  const floatBits = (value) => {
    const data = new Uint8Array(4);
    const view = new DataView(data.buffer);
    view.setFloat32(0, value, true);
    return view.getInt32(0, true);
  };

  pushI32(0x676f6b7a); pushU8(2); pushU8(0);
  pushString('test'); pushString('kz_test');
  pushI32(123); pushI32(0); pushI32(1); pushString('player'); pushI32(42);
  pushU8(2); pushU8(0); pushF32(2.5); pushF32(0.022); pushF32(128); pushI32(2);
  pushI32(1); pushI32(2); pushF32(10.5); pushU8(0); pushI32(1);

  const fields = new Int32Array(20);
  fields[7] = floatBits(100); fields[8] = floatBits(200); fields[9] = floatBits(300);
  fields[10] = floatBits(5); fields[11] = floatBits(90);
  fields[13] = floatBits(250); fields[14] = floatBits(50); fields[15] = floatBits(0);
  pushI32((1 << 20) - 1);
  for (let index = 1; index < 20; index += 1) pushI32(fields[index]);

  pushI32(1 << 7); pushI32(floatBits(110));
  return Uint8Array.from(bytes).buffer;
}

test('parses GOKZ v2 run replay headers and delta-compressed ticks', () => {
  const replay = parseGokzReplay(replayFixture());
  assert.equal(replay.header.mapName, 'kz_test');
  assert.equal(replay.header.playerAlias, 'player');
  assert.equal(replay.header.tickrate, 128);
  assert.equal(replay.header.time, 10.5);
  assert.equal(replay.header.teleports, 1);
  assert.deepEqual(replay.ticks[0].origin, [100, 200, 300]);
  assert.deepEqual(replay.ticks[1].origin, [110, 200, 300]);
  assert.equal(replay.ticks[1].angles[1], 90);
});

test('rejects non-GOKZ data', () => {
  assert.throws(() => parseGokzReplay(new Uint8Array(64).buffer), /不是有效/);
});

