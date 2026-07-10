# CNGOKZ Replay Worker

同一个 Worker 和 R2 Bucket 同时存储录像与网站证据文件：

- `wr/{mode}/{map}/{route}.replay`：全球 WR 录像，可公开下载。
- `audit/{recordId}/{idempotencyKey}.replay`：异常审核录像，Worker 下载接口要求 API Key。
- `appeals/`、`player-reports/`、`bans/`、`abnormal-records/`：网站证据文件，使用短时 HMAC 签名下载。

部署时将 R2 binding 命名为 `REPLAY_BUCKET`。为了兼容现有部署，Worker 也会识别
`errorplayer` 和 `cngokz-replay` binding。`bucket_name` 可以填写现有 WR Bucket 或
`errorplayer`，但 WR、异常插件和网站后端必须统一指向这个 Worker。

复制配置并设置密钥：

```bash
cp wrangler.toml.example wrangler.toml
npx wrangler secret put API_KEY
npx wrangler secret put DOWNLOAD_SIGNING_KEY
npx wrangler deploy
```

游戏服务器的 `gokz-r2upload.cfg` 只需要配置一次 Worker `/upload` URL 和 API Key。
`cngokz-recordguard` 默认复用这套配置，并通过 `X-CNGOKZ-Replay-Category: abnormal`
和 `X-CNGOKZ-Object-Key` 将异常录像写入 `audit/` 前缀。

生产环境网站后端配置 `R2_WORKER_URL`、`R2_WORKER_API_KEY` 和
`R2_WORKER_SIGNING_KEY`。后端通过 `/internal/upload` 上传文件，并生成 `/files/`
短时签名下载地址，无需保存 R2 S3 Access Key。
