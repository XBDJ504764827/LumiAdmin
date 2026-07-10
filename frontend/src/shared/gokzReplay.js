const MAGIC = 0x676f6b7a;
const FORMAT_VERSION = 2;
const RUN_REPLAY = 0;
const TICK_FIELDS = 20;

class ReplayReader {
  constructor(buffer) {
    this.view = new DataView(buffer);
    this.offset = 0;
    this.decoder = new TextDecoder('utf-8');
  }

  ensure(length) {
    if (this.offset + length > this.view.byteLength) {
      throw new Error('Replay 文件不完整');
    }
  }

  u8() {
    this.ensure(1);
    return this.view.getUint8(this.offset++);
  }

  i32() {
    this.ensure(4);
    const value = this.view.getInt32(this.offset, true);
    this.offset += 4;
    return value;
  }

  f32() {
    this.ensure(4);
    const value = this.view.getFloat32(this.offset, true);
    this.offset += 4;
    return value;
  }

  string() {
    const length = this.u8();
    this.ensure(length);
    const bytes = new Uint8Array(this.view.buffer, this.offset, length);
    this.offset += length;
    return this.decoder.decode(bytes);
  }
}

function bitsToFloat(value) {
  const view = new DataView(new ArrayBuffer(4));
  view.setInt32(0, value, true);
  return view.getFloat32(0, true);
}

export function parseGokzReplay(buffer) {
  const reader = new ReplayReader(buffer);
  if (reader.i32() !== MAGIC) throw new Error('不是有效的 GOKZ Replay 文件');

  const formatVersion = reader.u8();
  if (formatVersion !== FORMAT_VERSION) throw new Error(`暂不支持 Replay v${formatVersion}`);

  const replayType = reader.u8();
  if (replayType !== RUN_REPLAY) throw new Error('当前播放器只支持跑图 Replay');

  const gokzVersion = reader.string();
  const mapName = reader.string();
  const mapFileSize = reader.i32();
  const serverIp = reader.i32();
  const timestamp = reader.i32();
  const playerAlias = reader.string();
  const playerSteamId = reader.i32();
  const mode = reader.u8();
  const style = reader.u8();
  const sensitivity = reader.f32();
  const mYaw = reader.f32();
  const tickrate = reader.f32();
  const tickCount = reader.i32();
  const equippedWeapon = reader.i32();
  const equippedKnife = reader.i32();
  const time = reader.f32();
  const course = reader.u8();
  const teleports = reader.i32();

  if (!Number.isFinite(tickrate) || tickrate <= 0 || tickrate > 1024) {
    throw new Error('Replay Tickrate 无效');
  }
  if (tickCount <= 0 || tickCount > 5_000_000) {
    throw new Error('Replay Tick 数量无效');
  }

  const fields = new Int32Array(TICK_FIELDS);
  const ticks = new Array(tickCount);
  for (let tick = 0; tick < tickCount; tick += 1) {
    const changed = reader.i32();
    for (let index = 1; index < TICK_FIELDS; index += 1) {
      if ((changed & (1 << index)) !== 0) fields[index] = reader.i32();
    }
    ticks[tick] = {
      origin: [bitsToFloat(fields[7]), bitsToFloat(fields[8]), bitsToFloat(fields[9])],
      angles: [bitsToFloat(fields[10]), bitsToFloat(fields[11]), bitsToFloat(fields[12])],
      velocity: [bitsToFloat(fields[13]), bitsToFloat(fields[14]), bitsToFloat(fields[15])],
      flags: fields[16],
    };
  }

  return {
    header: {
      formatVersion,
      replayType,
      gokzVersion,
      mapName,
      mapFileSize,
      serverIp,
      timestamp,
      playerAlias,
      playerSteamId,
      mode,
      style,
      sensitivity,
      mYaw,
      tickrate,
      tickCount,
      equippedWeapon,
      equippedKnife,
      time,
      course,
      teleports,
    },
    ticks,
  };
}

