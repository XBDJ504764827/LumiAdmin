use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone)]
struct Config {
    base_url: String,
    token: String,
    map_dir: PathBuf,
    maplist_path: Option<PathBuf>,
    mapcycle_path: Option<PathBuf>,
    sync_map_pool_files: bool,
    interval_secs: u64,
    limit: usize,
    once: bool,
}

#[derive(Debug, Serialize)]
struct InventoryBody {
    maps: Vec<InventoryMap>,
}

#[derive(Debug, Serialize)]
struct InventoryMap {
    file_name: String,
    size_bytes: i64,
    modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct TasksResponse {
    tasks: Vec<MapTask>,
}

#[derive(Debug, Deserialize)]
struct MapPoolResponse {
    map_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MapTask {
    id: String,
    file_name: String,
    source_url: String,
}

#[derive(Debug, Serialize)]
struct TaskReportBody {
    status: String,
    error: Option<String>,
    size_bytes: Option<i64>,
    modified_at: Option<DateTime<Utc>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let config = Config::from_env()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("LumiAdminMapSyncAgent/1.0")
        .build()
        .context("HTTP 客户端初始化失败")?;

    if config.once {
        run_once(&client, &config).await?;
        return Ok(());
    }

    loop {
        match run_once(&client, &config).await {
            Ok(count) => println!("cycle complete, tasks={count}"),
            Err(error) => eprintln!("agent error: {error:#}"),
        }
        tokio::time::sleep(Duration::from_secs(config.interval_secs.max(30))).await;
    }
}

impl Config {
    fn from_env() -> Result<Self> {
        reject_cli_args()?;

        let base_url = env::var("MAP_SYNC_AGENT_BASE_URL").ok();
        let token = env::var("MAP_SYNC_AGENT_TOKEN").ok();
        let map_dir = env::var("MAP_SYNC_AGENT_MAP_DIR").ok();
        let maplist_path = optional_path("MAP_SYNC_AGENT_MAPLIST_PATH")?;
        let mapcycle_path = optional_path("MAP_SYNC_AGENT_MAPCYCLE_PATH")?;
        let sync_map_pool_files = env_bool("MAP_SYNC_AGENT_SYNC_MAP_POOL_FILES", true);
        let interval_secs = env::var("MAP_SYNC_AGENT_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(300);
        let limit = env::var("MAP_SYNC_AGENT_TASK_LIMIT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(20usize);
        let once = env_bool("MAP_SYNC_AGENT_RUN_ONCE", false);

        let base_url = required(base_url, "MAP_SYNC_AGENT_BASE_URL")?;
        let token = required(token, "MAP_SYNC_AGENT_TOKEN")?;
        let map_dir = expand_path(&required(map_dir, "MAP_SYNC_AGENT_MAP_DIR")?)?;
        anyhow::ensure!(map_dir.exists(), "地图目录不存在: {}", map_dir.display());
        anyhow::ensure!(
            map_dir.is_dir(),
            "MAP_SYNC_AGENT_MAP_DIR 不是目录: {}",
            map_dir.display()
        );

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            map_dir,
            maplist_path,
            mapcycle_path,
            sync_map_pool_files,
            interval_secs,
            limit: limit.clamp(1, 50),
            once,
        })
    }
}

fn reject_cli_args() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "map_sync_agent 不再使用命令行参数，请在 .env 中配置 MAP_SYNC_AGENT_BASE_URL、MAP_SYNC_AGENT_TOKEN、MAP_SYNC_AGENT_MAP_DIR、MAP_SYNC_AGENT_MAPLIST_PATH、MAP_SYNC_AGENT_MAPCYCLE_PATH 等变量"
    )
}

async fn run_once(client: &reqwest::Client, config: &Config) -> Result<usize> {
    let inventory = scan_maps(&config.map_dir)?;
    post_json::<serde_json::Value>(
        client,
        config,
        "/api/map-sync/agent/inventory",
        &InventoryBody { maps: inventory },
    )
    .await
    .context("上报地图库存失败")?;

    let tasks: TasksResponse = get_json(
        client,
        config,
        &format!("/api/map-sync/agent/tasks?limit={}", config.limit),
    )
    .await
    .context("领取地图任务失败")?;

    let count = tasks.tasks.len();
    for task in tasks.tasks {
        match download_task(client, config, &task).await {
            Ok((size_bytes, modified_at)) => {
                report_task(
                    client,
                    config,
                    &task.id,
                    TaskReportBody {
                        status: "succeeded".to_string(),
                        error: None,
                        size_bytes: Some(size_bytes),
                        modified_at,
                    },
                )
                .await?;
                println!("updated {}", task.file_name);
            }
            Err(error) => {
                report_task(
                    client,
                    config,
                    &task.id,
                    TaskReportBody {
                        status: "failed".to_string(),
                        error: Some(error.to_string()),
                        size_bytes: None,
                        modified_at: None,
                    },
                )
                .await?;
                eprintln!("failed {}: {error:#}", task.file_name);
            }
        }
    }

    if config.sync_map_pool_files {
        match update_map_pool_files(client, config).await {
            Ok(()) => {}
            Err(error) => eprintln!("map pool update failed: {error:#}"),
        }
    }

    Ok(count)
}

fn scan_maps(map_dir: &Path) -> Result<Vec<InventoryMap>> {
    let mut maps = Vec::new();
    for entry in fs::read_dir(map_dir).context("读取地图目录失败")? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let lower = file_name.to_ascii_lowercase();
        if !lower.ends_with(".bsp") && !lower.ends_with(".bsp.bz2") {
            continue;
        }
        let metadata = path.metadata()?;
        maps.push(InventoryMap {
            file_name: file_name.to_string(),
            size_bytes: metadata.len() as i64,
            modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
        });
    }
    Ok(maps)
}

async fn download_task(
    client: &reqwest::Client,
    config: &Config,
    task: &MapTask,
) -> Result<(i64, Option<DateTime<Utc>>)> {
    let target = safe_target_path(&config.map_dir, &task.file_name)?;
    let tmp = target.with_file_name(format!("{}.tmp", task.file_name));
    let response = client.get(&task.source_url).send().await?;
    anyhow::ensure!(
        response.status().is_success(),
        "下载返回状态 {}",
        response.status()
    );
    let bytes = response.bytes().await?;
    fs::write(&tmp, &bytes).with_context(|| format!("写入临时文件失败: {}", tmp.display()))?;
    fs::rename(&tmp, &target).with_context(|| format!("替换地图文件失败: {}", target.display()))?;
    let metadata = target.metadata()?;
    Ok((
        metadata.len() as i64,
        metadata.modified().ok().map(DateTime::<Utc>::from),
    ))
}

async fn report_task(
    client: &reqwest::Client,
    config: &Config,
    task_id: &str,
    body: TaskReportBody,
) -> Result<()> {
    post_json::<serde_json::Value>(
        client,
        config,
        &format!("/api/map-sync/agent/tasks/{task_id}/report"),
        &body,
    )
    .await
    .context("回报任务结果失败")?;
    Ok(())
}

async fn update_map_pool_files(client: &reqwest::Client, config: &Config) -> Result<()> {
    if config.maplist_path.is_none() && config.mapcycle_path.is_none() {
        return Ok(());
    }

    let response: MapPoolResponse = get_json(client, config, "/api/map-sync/agent/map-pool")
        .await
        .context("读取地图池失败")?;
    let mut map_names = response
        .map_names
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains('/') && !value.contains('\\'))
        .collect::<Vec<_>>();
    map_names.sort();
    map_names.dedup();
    if map_names.is_empty() {
        println!("map pool cache is empty, skipped maplist/mapcycle update");
        return Ok(());
    }
    let content = format!("{}\n", map_names.join("\n"));

    if let Some(path) = &config.maplist_path {
        write_if_changed(path, &content)?;
    }
    if let Some(path) = &config.mapcycle_path {
        write_if_changed(path, &content)?;
    }
    Ok(())
}

async fn get_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    config: &Config,
    path: &str,
) -> Result<T> {
    let response = client
        .get(format!("{}{}", config.base_url, path))
        .headers(agent_headers(&config.token)?)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    anyhow::ensure!(status.is_success(), "请求失败 {status}: {text}");
    serde_json::from_str(&text).context("解析响应 JSON 失败")
}

async fn post_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    config: &Config,
    path: &str,
    body: &impl Serialize,
) -> Result<T> {
    let response = client
        .post(format!("{}{}", config.base_url, path))
        .headers(agent_headers(&config.token)?)
        .json(body)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    anyhow::ensure!(status.is_success(), "请求失败 {status}: {text}");
    serde_json::from_str(&text).context("解析响应 JSON 失败")
}

fn agent_headers(token: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert("x-map-agent-token", token.parse()?);
    Ok(headers)
}

fn safe_target_path(map_dir: &Path, file_name: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        !file_name.contains('/') && !file_name.contains('\\') && !file_name.contains(".."),
        "非法文件名: {file_name}"
    );
    let lower = file_name.to_ascii_lowercase();
    anyhow::ensure!(
        lower.ends_with(".bsp") || lower.ends_with(".bsp.bz2"),
        "非地图文件: {file_name}"
    );
    Ok(map_dir.join(file_name))
}

fn required(value: Option<String>, name: &str) -> Result<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("缺少 {name}"))
}

fn optional_path(key: &str) -> Result<Option<PathBuf>> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_path(&value))
        .transpose()
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn expand_path(value: &str) -> Result<PathBuf> {
    let trimmed = value.trim();
    if trimmed == "~" {
        return home_dir();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Ok(home_dir()?.join(rest));
    }
    Ok(PathBuf::from(trimmed))
}

fn home_dir() -> Result<PathBuf> {
    env::var("HOME")
        .map(PathBuf::from)
        .context("无法展开 ~：HOME 环境变量不存在")
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == content {
            return Ok(());
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("map_pool")
    ));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    println!("updated {}", path.display());
    Ok(())
}
