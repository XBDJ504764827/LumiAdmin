const MAX_REPLAY_SIZE = 128 * 1024 * 1024;

export default {
  async fetch(request, env) {
    try {
      const url = new URL(request.url);

      if (request.method === "GET" && url.pathname === "/health") {
        return json({ success: true, service: "cngokz-replay-worker" });
      }

      const bucket = env.REPLAY_BUCKET || env.errorplayer || env["cngokz-replay"];
      if (!bucket) {
        return json({ error: "R2 replay bucket binding is not configured" }, 500);
      }

      // LumiAdmin 后端通过此内部接口上传举报、封禁和申诉证据。
      if (request.method === "POST" && url.pathname === "/internal/upload") {
        if (!isAuthorized(request, env)) {
          return unauthorized();
        }
        return await uploadWebsiteFile(request, bucket);
      }

      // 网站文件使用短时 HMAC 签名地址下载，不暴露 Worker API Key。
      if (["GET", "HEAD"].includes(request.method) && url.pathname.startsWith("/files/")) {
        const key = websiteObjectKeyFromPath(url.pathname);
        if (!key || !isPrivateObjectKey(key)) {
          return json({ error: "File not found" }, 404);
        }
        if (!await isValidSignedDownload(url, key, env)) {
          return unauthorized();
        }
        return request.method === "HEAD"
          ? await headReplay(bucket, key)
          : await downloadReplay(bucket, key);
      }

      if (request.method === "POST" && (url.pathname === "/" || url.pathname === "/upload")) {
        if (!isAuthorized(request, env)) {
          return unauthorized();
        }
        return await uploadReplay(request, bucket, url);
      }

      if (["GET", "HEAD", "DELETE"].includes(request.method)) {
        const key = objectKeyFromPath(url.pathname);
        if (!key || !isSupportedObjectKey(key)) {
          return json({ error: "Replay not found" }, 404);
        }

        // WR 录像可以公开下载；审核录像必须经过服务端鉴权。
        if ((key.startsWith("audit/") || request.method === "DELETE") && !isAuthorized(request, env)) {
          return unauthorized();
        }

        if (request.method === "DELETE") {
          return await deleteReplay(bucket, key);
        }
        if (request.method === "HEAD") {
          return await headReplay(bucket, key);
        }
        if (url.searchParams.get("meta") === "1") {
          return await replayMetadata(bucket, key);
        }
        return await downloadReplay(bucket, key);
      }

      return json({ error: "Method not allowed" }, 405, { Allow: "GET, HEAD, POST, DELETE" });
    } catch (error) {
      console.error("Replay Worker error", error);
      return json({ error: "Internal server error" }, 500);
    }
  }
};

async function uploadWebsiteFile(request, bucket) {
  const key = (request.headers.get("x-object-key") || "").trim();
  if (!isWebsiteEvidenceKey(key)) {
    return json({ error: "Invalid website file object key" }, 400);
  }

  const declaredLength = Number(request.headers.get("content-length") || 0);
  if (declaredLength > MAX_REPLAY_SIZE) {
    return json({ error: "File is too large", max_size: MAX_REPLAY_SIZE }, 413);
  }
  if (!request.body) {
    return json({ error: "File is empty" }, 400);
  }

  const contentType = cleanMetadata(
    request.headers.get("content-type") || "application/octet-stream",
    120
  );
  const uploadedAt = String(Date.now());

  await bucket.put(key, request.body, {
    httpMetadata: {
      contentType,
      cacheControl: "private, no-store"
    },
    customMetadata: {
      category: "website-evidence",
      uploaded_at: uploadedAt
    }
  });

  return json({
    success: true,
    object_key: key,
    uploaded_at: uploadedAt,
    size: declaredLength || null
  }, 201);
}

async function uploadReplay(request, bucket, url) {
  const declaredLength = Number(request.headers.get("content-length") || 0);
  if (declaredLength > MAX_REPLAY_SIZE) {
    return json({ error: "Replay file is too large", max_size: MAX_REPLAY_SIZE }, 413);
  }

  const categoryHeader = (request.headers.get("x-cngokz-replay-category") || "").trim().toLowerCase();
  const abnormalRecordId = (
    request.headers.get("x-cngokz-abnormal-record-id") ||
    request.headers.get("x-record-id") ||
    request.headers.get("x-error-id") ||
    ""
  ).trim().toLowerCase();
  const isAudit = categoryHeader === "abnormal" || categoryHeader === "audit" || abnormalRecordId !== "";

  let upload;
  if (isAudit) {
    upload = auditUploadDescriptor(request, abnormalRecordId);
  } else {
    upload = wrUploadDescriptor(request);
  }
  if (!upload.ok) {
    return json({ error: upload.error }, 400);
  }

  const fileData = await request.arrayBuffer();
  if (fileData.byteLength === 0) {
    return json({ error: "Replay file is empty" }, 400);
  }
  if (fileData.byteLength > MAX_REPLAY_SIZE) {
    return json({ error: "Replay file is too large", max_size: MAX_REPLAY_SIZE }, 413);
  }

  const sha256 = await calculateSha256(fileData);
  const uploadedAt = cleanMetadata(request.headers.get("x-timestamp") || String(Date.now()), 32);
  const timeMs = cleanMetadata(request.headers.get("x-time-ms") || "", 32);
  const customMetadata = {
    category: upload.category,
    uploaded_at: uploadedAt,
    sha256
  };

  if (timeMs) customMetadata.time_ms = timeMs;
  if (upload.recordId) customMetadata.record_id = upload.recordId;
  if (upload.mode) customMetadata.gokz_mode = upload.mode;
  if (upload.map) customMetadata.map = upload.map;
  if (upload.route) customMetadata.route = upload.route;

  await bucket.put(upload.key, fileData, {
    httpMetadata: {
      contentType: "application/octet-stream",
      cacheControl: upload.category === "wr" ? "public, max-age=3600" : "private, no-store"
    },
    customMetadata
  });

  const replayPath = `/${encodeObjectKey(upload.key)}`;
  return json({
    success: true,
    category: upload.category,
    object_key: upload.key,
    path: upload.key,
    replay_url: new URL(replayPath, url.origin).toString(),
    time_ms: timeMs || null,
    uploaded_at: uploadedAt,
    sha256,
    size: fileData.byteLength
  }, 201);
}

function auditUploadDescriptor(request, recordId) {
  if (!isUuid(recordId)) {
    return { ok: false, error: "Missing or invalid abnormal record ID" };
  }

  const requestedKey = (request.headers.get("x-cngokz-object-key") || "").trim();
  const key = requestedKey || `audit/${recordId}/${recordId}.replay`;
  const match = key.match(/^audit\/([0-9a-f-]{36})\/([A-Za-z0-9_-]{1,120})\.replay$/i);

  if (!match || match[1].toLowerCase() !== recordId) {
    return { ok: false, error: "Invalid audit replay object key" };
  }

  return {
    ok: true,
    category: "audit",
    key,
    recordId,
    mode: normalizeMode(request.headers.get("x-gokz-mode")),
    map: sanitizeSegment(request.headers.get("x-map"), 100),
    route: ""
  };
}

function wrUploadDescriptor(request) {
  const mode = normalizeMode(request.headers.get("x-gokz-mode"));
  const map = sanitizeSegment(request.headers.get("x-map"), 100);
  const route = sanitizeSegment(request.headers.get("x-route"), 120);

  if (!mode || !map || !route) {
    return { ok: false, error: "Missing or invalid x-gokz-mode, x-map or x-route" };
  }

  return {
    ok: true,
    category: "wr",
    key: `wr/${mode}/${map}/${route}.replay`,
    recordId: "",
    mode,
    map,
    route
  };
}

async function replayMetadata(bucket, key) {
  const object = await bucket.head(key);
  if (!object) {
    return json({ exists: false }, 404);
  }

  const metadata = object.customMetadata || {};
  return json({
    exists: true,
    object_key: key,
    category: metadata.category || (key.startsWith("audit/") ? "audit" : "wr"),
    record_id: metadata.record_id || null,
    gokz_mode: metadata.gokz_mode || null,
    map: metadata.map || null,
    route: metadata.route || null,
    time_ms: metadata.time_ms || null,
    uploaded_at: metadata.uploaded_at || null,
    sha256: metadata.sha256 || null,
    size: object.size,
    etag: object.httpEtag || null
  });
}

async function downloadReplay(bucket, key) {
  const object = await bucket.get(key);
  if (!object) {
    return json({ error: "Replay not found" }, 404);
  }

  const metadata = object.customMetadata || {};
  const headers = replayHeaders(object, key, metadata);
  const fileName = key.split("/").at(-1) || "run.replay";
  const forceDownload = /\.(replay|dem)$/i.test(fileName);
  headers.set(
    "Content-Disposition",
    `${forceDownload ? "attachment" : "inline"}; filename="${fileName}"`
  );
  return new Response(object.body, { status: 200, headers });
}

async function headReplay(bucket, key) {
  const object = await bucket.head(key);
  if (!object) {
    return new Response(null, { status: 404 });
  }
  return new Response(null, {
    status: 200,
    headers: replayHeaders(object, key, object.customMetadata || {})
  });
}

async function deleteReplay(bucket, key) {
  const object = await bucket.head(key);
  if (!object) {
    return json({ error: "Replay not found" }, 404);
  }
  await bucket.delete(key);
  return json({ success: true, deleted: true, object_key: key });
}

function replayHeaders(object, key, metadata) {
  const headers = new Headers();
  object.writeHttpMetadata(headers);
  headers.set("Content-Type", "application/octet-stream");
  headers.set("Content-Length", String(object.size));
  headers.set("Cache-Control", key.startsWith("wr/") ? "public, max-age=3600" : "private, no-store");
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set("x-size", String(object.size));
  if (object.httpEtag) headers.set("ETag", object.httpEtag);
  if (metadata.record_id) headers.set("x-record-id", metadata.record_id);
  if (metadata.uploaded_at) headers.set("x-uploaded-at", metadata.uploaded_at);
  if (metadata.sha256) headers.set("x-sha256", metadata.sha256);
  if (metadata.time_ms) headers.set("x-time-ms", metadata.time_ms);
  return headers;
}

function objectKeyFromPath(pathname) {
  let encoded = pathname.replace(/^\/+/, "");
  if (encoded.startsWith("replay/")) encoded = encoded.slice("replay/".length);
  if (!encoded) return "";
  try {
    return decodeURIComponent(encoded);
  } catch {
    return "";
  }
}

function websiteObjectKeyFromPath(pathname) {
  const encoded = pathname.slice("/files/".length);
  if (!encoded) return "";
  try {
    return decodeURIComponent(encoded);
  } catch {
    return "";
  }
}

function isSupportedObjectKey(key) {
  return /^wr\/(vnl|skz|kzt)\/[a-z0-9_-]{1,100}\/[a-z0-9_-]{1,120}\.replay$/.test(key)
    || /^audit\/[0-9a-f-]{36}\/[A-Za-z0-9_-]{1,120}\.replay$/i.test(key);
}

function isWebsiteEvidenceKey(key) {
  return /^(appeals|player-reports|bans|abnormal-records)\/[0-9a-f-]{36}\/[A-Za-z0-9._-]{1,512}$/i.test(key);
}

function isPrivateObjectKey(key) {
  return isWebsiteEvidenceKey(key)
    || /^audit\/[0-9a-f-]{36}\/[A-Za-z0-9_-]{1,120}\.replay$/i.test(key);
}

async function isValidSignedDownload(url, key, env) {
  if (!env.DOWNLOAD_SIGNING_KEY) return false;

  const expiresText = url.searchParams.get("expires") || "";
  const signature = (url.searchParams.get("signature") || "").toLowerCase();
  if (!/^\d{10,13}$/.test(expiresText) || !/^[0-9a-f]{64}$/.test(signature)) {
    return false;
  }

  const expires = Number(expiresText);
  const now = Math.floor(Date.now() / 1000);
  if (!Number.isSafeInteger(expires) || expires < now || expires > now + 604800) {
    return false;
  }

  const expected = await hmacSha256Hex(
    env.DOWNLOAD_SIGNING_KEY,
    `${key}\n${expiresText}`
  );
  return constantTimeEqual(signature, expected);
}

async function hmacSha256Hex(secret, message) {
  const encoder = new TextEncoder();
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );
  const signature = await crypto.subtle.sign("HMAC", key, encoder.encode(message));
  return Array.from(new Uint8Array(signature))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

function constantTimeEqual(left, right) {
  if (left.length !== right.length) return false;
  let result = 0;
  for (let i = 0; i < left.length; i++) {
    result |= left.charCodeAt(i) ^ right.charCodeAt(i);
  }
  return result === 0;
}

function isAuthorized(request, env) {
  if (!env.API_KEY) return false;
  const direct = request.headers.get("x-api-key");
  const bearer = request.headers.get("authorization")?.replace(/^Bearer\s+/i, "");
  return (direct || bearer || "") === env.API_KEY;
}

function unauthorized() {
  return json({ error: "Unauthorized" }, 401, { "WWW-Authenticate": "Bearer" });
}

function normalizeMode(value) {
  const mode = String(value || "").trim().toLowerCase();
  if (mode === "0") return "vnl";
  if (mode === "1") return "skz";
  if (mode === "2") return "kzt";
  return ["vnl", "skz", "kzt"].includes(mode) ? mode : "";
}

function sanitizeSegment(value, maxLength) {
  return String(value || "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]/g, "")
    .slice(0, maxLength);
}

function isUuid(value) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function cleanMetadata(value, maxLength) {
  return String(value || "")
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .trim()
    .slice(0, maxLength);
}

async function calculateSha256(data) {
  const digest = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

function encodeObjectKey(key) {
  return key.split("/").map(encodeURIComponent).join("/");
}

function json(body, status = 200, extraHeaders = {}) {
  const headers = new Headers(extraHeaders);
  headers.set("Content-Type", "application/json; charset=utf-8");
  headers.set("Cache-Control", "no-store");
  headers.set("X-Content-Type-Options", "nosniff");
  return new Response(JSON.stringify(body), { status, headers });
}
