#!/usr/bin/env python3
"""LumiBot Mock 服务端 —— 用于本地测试 LumiAdmin 的事件上报集成。

按 LumiBot HTTP API 文档模拟：
  GET  /health           → 200 {"status":"ok"}
  POST /api/v1/events    → 202 {"success":true,"event_id":"<id>"}（默认）
                          → 可通过环境变量注入故障

用法：
  python3 mock_lumi_bot.py [端口]        # 默认 8080
  MOCK_FAIL=1 python3 mock_lumi_bot.py   # 所有事件上报返回 500（测重试/死信）
  MOCK_FAIL_RATE=50 ...                  # 50% 概率返回 500
  MOCK_API_KEY=key-admin ...             # 校验 X-API-Key，不匹配返回 401

收到的每个请求会打印到 stdout，并追加写入 ./mock_lumi_bot_requests.log
"""
import json
import os
import random
import sys
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
API_KEY = os.environ.get("MOCK_API_KEY", "")
FAIL = os.environ.get("MOCK_FAIL") == "1"
FAIL_RATE = int(os.environ.get("MOCK_FAIL_RATE", "0"))
LOG_FILE = os.environ.get("MOCK_LOG", "mock_lumi_bot_requests.log")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):  # 关闭默认访问日志（我们自行打印）
        pass

    def do_GET(self):
        if self.path == "/health":
            self._respond(200, {"status": "ok"})
        else:
            self._respond(404, {"success": False, "error": "not found"})

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length) if length else b""
        record = {
            "time": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "method": "POST",
            "path": self.path,
            "x_api_key": self.headers.get("X-API-Key", ""),
            "body": raw.decode("utf-8", errors="replace"),
        }
        print(json.dumps(record, ensure_ascii=False))
        with open(LOG_FILE, "a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")

        if self.path != "/api/v1/events":
            self._respond(404, {"success": False, "error": "not found"})
            return

        # 模拟鉴权失败
        if API_KEY and self.headers.get("X-API-Key", "") != API_KEY:
            self._respond(401, {"success": False, "error": "invalid api key"})
            return

        # 模拟限流
        if self.headers.get("X-API-Key", "") == "key-limited":
            self._respond(429, {"success": False, "error": "rate limit exceeded"})
            return

        # 模拟服务器内部错误 / 随机故障
        if FAIL or (FAIL_RATE > 0 and random.randint(1, 100) <= FAIL_RATE):
            self._respond(500, {"success": False, "error": "internal error"})
            return

        # 模拟参数校验（必填字段缺失）
        try:
            body = json.loads(raw or "{}")
        except json.JSONDecodeError:
            self._respond(400, {"success": False, "error": "JSON 解析失败"})
            return
        if not body.get("source") or not body.get("event_type"):
            self._respond(400, {"success": False, "error": "source/event_type 不能为空"})
            return

        event_id = body.get("id") or str(uuid.uuid4())
        self._respond(202, {"success": True, "event_id": event_id})

    def _respond(self, status: int, payload: dict):
        data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


if __name__ == "__main__":
    print(f"Mock LumiBot listening on 127.0.0.1:{PORT}  (日志写入 {LOG_FILE})")
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
