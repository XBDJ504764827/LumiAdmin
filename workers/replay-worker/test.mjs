import assert from "node:assert/strict";
import worker from "./src/index.js";

class MemoryBucket {
  constructor() {
    this.objects = new Map();
  }

  async put(key, data, options = {}) {
    const body = data instanceof ReadableStream
      ? new Uint8Array(await new Response(data).arrayBuffer())
      : new Uint8Array(data);
    this.objects.set(key, {
      body,
      size: body.byteLength,
      customMetadata: options.customMetadata || {},
      httpMetadata: options.httpMetadata || {},
      httpEtag: `"test-${body.byteLength}"`
    });
  }

  async get(key) {
    return this.wrap(this.objects.get(key));
  }

  async head(key) {
    return this.wrap(this.objects.get(key), false);
  }

  async delete(key) {
    this.objects.delete(key);
  }

  wrap(value, includeBody = true) {
    if (!value) return null;
    return {
      ...value,
      body: includeBody ? value.body : undefined,
      writeHttpMetadata(headers) {
        if (value.httpMetadata.contentType) headers.set("Content-Type", value.httpMetadata.contentType);
        if (value.httpMetadata.cacheControl) headers.set("Cache-Control", value.httpMetadata.cacheControl);
      }
    };
  }
}

const bucket = new MemoryBucket();
const env = {
  REPLAY_BUCKET: bucket,
  API_KEY: "test-secret",
  DOWNLOAD_SIGNING_KEY: "test-download-signing-secret"
};

async function call(path, init = {}) {
  return worker.fetch(new Request(`https://replay.example${path}`, init), env);
}

const wrUpload = await call("/upload", {
  method: "POST",
  headers: {
    "x-api-key": env.API_KEY,
    "x-gokz-mode": "kzt",
    "x-map": "kz_test",
    "x-route": "main"
  },
  body: "WR_REPLAY"
});
assert.equal(wrUpload.status, 201);
assert.equal((await wrUpload.json()).object_key, "wr/kzt/kz_test/main.replay");

const publicWr = await call("/wr/kzt/kz_test/main.replay");
assert.equal(publicWr.status, 200);
assert.equal(await publicWr.text(), "WR_REPLAY");

const recordId = "8d69604c-e3f3-4555-ba07-bc3dfd6d0a9a";
const auditKey = `audit/${recordId}/audit-test-123.replay`;
const auditUpload = await call("/upload", {
  method: "POST",
  headers: {
    "x-api-key": env.API_KEY,
    "x-cngokz-replay-category": "abnormal",
    "x-cngokz-abnormal-record-id": recordId,
    "x-cngokz-object-key": auditKey,
    "x-gokz-mode": "kzt",
    "x-map": "kz_test"
  },
  body: "AUDIT_REPLAY"
});
assert.equal(auditUpload.status, 201);
assert.equal((await auditUpload.json()).object_key, auditKey);

assert.equal((await call(`/${auditKey}`)).status, 401);

const auditMeta = await call(`/${auditKey}?meta=1`, {
  headers: { "x-api-key": env.API_KEY }
});
assert.equal(auditMeta.status, 200);
assert.equal((await auditMeta.json()).record_id, recordId);

const auditDownload = await call(`/${auditKey}`, {
  headers: { "x-api-key": env.API_KEY }
});
assert.equal(auditDownload.status, 200);
assert.equal(await auditDownload.text(), "AUDIT_REPLAY");

const auditDelete = await call(`/${auditKey}`, {
  method: "DELETE",
  headers: { "x-api-key": env.API_KEY }
});
assert.equal(auditDelete.status, 200);
assert.equal(await bucket.head(auditKey), null);

const evidenceKey = "player-reports/8d69604c-e3f3-4555-ba07-bc3dfd6d0a9a/11111111-2222-4333-8444-555555555555-proof.png";
const evidenceUpload = await call("/internal/upload", {
  method: "POST",
  headers: {
    "x-api-key": env.API_KEY,
    "x-object-key": evidenceKey,
    "content-type": "image/png"
  },
  body: "PNG_TEST"
});
assert.equal(evidenceUpload.status, 201);

const unsignedEvidence = await call(`/files/${evidenceKey}`);
assert.equal(unsignedEvidence.status, 401);

const expires = Math.floor(Date.now() / 1000) + 60;
const signingKey = await crypto.subtle.importKey(
  "raw",
  new TextEncoder().encode(env.DOWNLOAD_SIGNING_KEY),
  { name: "HMAC", hash: "SHA-256" },
  false,
  ["sign"]
);
const signed = await crypto.subtle.sign(
  "HMAC",
  signingKey,
  new TextEncoder().encode(`${evidenceKey}\n${expires}`)
);
const signature = Array.from(new Uint8Array(signed))
  .map((byte) => byte.toString(16).padStart(2, "0"))
  .join("");
const evidenceDownload = await call(
  `/files/${evidenceKey}?expires=${expires}&signature=${signature}`
);
assert.equal(evidenceDownload.status, 200);
assert.equal(await evidenceDownload.text(), "PNG_TEST");

console.log("replay-worker integration tests passed");
