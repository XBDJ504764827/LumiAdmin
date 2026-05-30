use crate::{config::Config, http_client};
use anyhow::Context;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;

const STEAM_ID64_BASE: u64 = 76561197960265728;
const STEAM_API_MAX_RETRIES: u32 = 3;

static RE_STEAM2: OnceLock<Regex> = OnceLock::new();
static RE_STEAM3: OnceLock<Regex> = OnceLock::new();
static RE_PROFILE_URL: OnceLock<Regex> = OnceLock::new();
static RE_VANITY_URL: OnceLock<Regex> = OnceLock::new();

fn re_steam2() -> &'static Regex {
    RE_STEAM2.get_or_init(|| Regex::new(r"(?i)^STEAM_[0-5]:([0-1]):(\d+)$").unwrap())
}
fn re_steam3() -> &'static Regex {
    RE_STEAM3.get_or_init(|| Regex::new(r"^\[U:1:(\d+)\]$").unwrap())
}
fn re_profile_url() -> &'static Regex {
    RE_PROFILE_URL
        .get_or_init(|| Regex::new(r"^https?://steamcommunity\.com/profiles/(\d{17})/?$").unwrap())
}
fn re_vanity_url() -> &'static Regex {
    RE_VANITY_URL
        .get_or_init(|| Regex::new(r"^https?://steamcommunity\.com/id/([^/?#]+)/?$").unwrap())
}
const CACHE_TTL_SECS: u64 = 300; // 5分钟缓存
const CACHE_MAX_SIZE: usize = 10000; // 最大缓存条目数

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSteamIdentity {
    pub steamid64: String,
    pub steamid: Option<String>,
    pub steamid3: Option<String>,
    pub profile_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SteamProfile {
    pub persona_name: String,
}

struct CacheEntry<T> {
    value: T,
    created_at: Instant,
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    fn new(value: T, ttl_secs: u64) -> Self {
        let now = Instant::now();
        Self {
            value,
            created_at: now,
            expires_at: now + Duration::from_secs(ttl_secs),
        }
    }

    fn is_valid(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

/// 从缓存中驱逐最旧的条目，保持缓存大小在限制内
fn evict_oldest_if_needed<T>(cache: &mut HashMap<String, CacheEntry<T>>, max_size: usize) {
    if cache.len() <= max_size {
        return;
    }

    // 需要删除的条目数
    let to_remove = cache.len() - max_size;

    // 收集所有条目的创建时间和键
    let mut entries: Vec<(String, Instant)> = cache
        .iter()
        .map(|(k, v)| (k.clone(), v.created_at))
        .collect();

    // 按创建时间排序（最旧的在前）
    entries.sort_by(|a, b| a.1.cmp(&b.1));

    // 删除最旧的条目
    for (key, _) in entries.into_iter().take(to_remove) {
        cache.remove(&key);
    }
}

#[derive(Clone)]
pub struct SteamResolver {
    steam_api_key: Option<String>,
    steamchina_profile_key: Option<String>,
    profile_cache: Arc<RwLock<HashMap<String, CacheEntry<Option<SteamProfile>>>>>,
}

impl SteamResolver {
    pub fn new(config: &Config) -> Self {
        Self {
            steam_api_key: config.steam_api_key.clone(),
            steamchina_profile_key: config.steamchina_profile_key.clone(),
            profile_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            steam_api_key: None,
            steamchina_profile_key: None,
            profile_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn parse_local(&self, input: &str) -> anyhow::Result<ParsedSteamIdentity> {
        let input = input.trim();
        anyhow::ensure!(!input.is_empty(), "请输入 Steam 标识符");

        if is_steamid64(input) {
            let steamid64 = input.to_string();
            return Ok(build_identity(
                steamid64.clone(),
                Some(steamid2_from_steamid64(&steamid64)?),
                Some(steamid3_from_steamid64(&steamid64)?),
                Some(profile_url_from_steamid64(&steamid64)),
            ));
        }

        if let Some(captures) = re_steam2().captures(input) {
            let parity: u64 = captures[1].parse()?;
            let account_number: u64 = captures[2].parse()?;
            let steamid64 = (STEAM_ID64_BASE + account_number * 2 + parity).to_string();
            return Ok(build_identity(
                steamid64.clone(),
                Some(format!("STEAM_0:{parity}:{account_number}")),
                Some(steamid3_from_steamid64(&steamid64)?),
                Some(profile_url_from_steamid64(&steamid64)),
            ));
        }

        if let Some(captures) = re_steam3().captures(input) {
            let account_id: u64 = captures[1].parse()?;
            let steamid64 = (STEAM_ID64_BASE + account_id).to_string();
            return Ok(build_identity(
                steamid64.clone(),
                Some(steamid2_from_steamid64(&steamid64)?),
                Some(format!("[U:1:{account_id}]")),
                Some(profile_url_from_steamid64(&steamid64)),
            ));
        }

        if let Some(captures) = re_profile_url().captures(input) {
            let steamid64 = captures[1].to_string();
            return Ok(build_identity(
                steamid64.clone(),
                Some(steamid2_from_steamid64(&steamid64)?),
                Some(steamid3_from_steamid64(&steamid64)?),
                Some(profile_url_from_steamid64(&steamid64)),
            ));
        }

        anyhow::bail!("无法识别 Steam 标识符格式")
    }

    pub async fn resolve(&self, input: &str) -> anyhow::Result<ParsedSteamIdentity> {
        if let Ok(parsed) = self.parse_local(input) {
            return Ok(parsed);
        }

        let input = input.trim();
        let vanity = extract_vanity(input)?;
        let api_key = self
            .steam_api_key
            .as_deref()
            .context("缺少 STEAM_API_KEY，无法解析自定义 Steam 主页链接")?;

        let response = http_client::http_client()
            .get("https://api.steampowered.com/ISteamUser/ResolveVanityURL/v1/")
            .query(&[("key", api_key), ("vanityurl", vanity.as_str())])
            .send()
            .await?
            .error_for_status()?;

        let payload: ResolveVanityUrlEnvelope = response.json().await?;
        anyhow::ensure!(payload.response.success == 1, "无法解析 Steam 个人主页链接");
        let steamid64 = payload
            .response
            .steamid
            .context("Steam 接口未返回 steamid64")?;

        Ok(build_identity(
            steamid64.clone(),
            Some(steamid2_from_steamid64(&steamid64)?),
            Some(steamid3_from_steamid64(&steamid64)?),
            Some(format!("https://steamcommunity.com/id/{vanity}")),
        ))
    }

    pub async fn fetch_profile(&self, steamid64: &str) -> anyhow::Result<Option<SteamProfile>> {
        // 检查缓存
        {
            let cache = self.profile_cache.read().await;
            if let Some(entry) = cache.get(steamid64) {
                if entry.is_valid() {
                    return Ok(entry.value.clone());
                }
            }
        }

        let api_key = match self.steam_api_key.as_deref() {
            Some(key) => key,
            None => return Ok(None),
        };

        // 带重试的请求
        let mut last_error = None;
        for attempt in 0..STEAM_API_MAX_RETRIES {
            match self.fetch_profile_internal(steamid64, api_key).await {
                Ok(profile) => {
                    // 更新缓存
                    {
                        let mut cache = self.profile_cache.write().await;
                        cache.insert(
                            steamid64.to_string(),
                            CacheEntry::new(profile.clone(), CACHE_TTL_SECS),
                        );
                        evict_oldest_if_needed(&mut cache, CACHE_MAX_SIZE);
                    }
                    return Ok(profile);
                }
                Err(e) => {
                    last_error = Some(e);
                    // 指数退避
                    if attempt < STEAM_API_MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                    }
                }
            }
        }

        // 失败时缓存 None，避免频繁重试
        {
            let mut cache = self.profile_cache.write().await;
            cache.insert(steamid64.to_string(), CacheEntry::new(None, 60)); // 失败缓存1分钟
            evict_oldest_if_needed(&mut cache, CACHE_MAX_SIZE);
        }

        last_error.map_or(Ok(None), Err)
    }

    async fn fetch_profile_internal(
        &self,
        steamid64: &str,
        api_key: &str,
    ) -> anyhow::Result<Option<SteamProfile>> {
        // 优先使用 steamchina（国内更快）
        if let Some(ref china_key) = self.steamchina_profile_key {
            if let Ok(Some(profile)) = Self::fetch_profile_from(
                "https://api.steamchina.com/ISteamUser/GetPlayerSummaries/v2/",
                steamid64,
                china_key,
            )
            .await
            {
                return Ok(Some(profile));
            }
        }

        // 备用：steampowered
        Self::fetch_profile_from(
            "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/",
            steamid64,
            api_key,
        )
        .await
    }

    async fn fetch_profile_from(
        base_url: &str,
        steamid64: &str,
        api_key: &str,
    ) -> anyhow::Result<Option<SteamProfile>> {
        let response = http_client::http_client()
            .get(base_url)
            .query(&[("key", api_key), ("steamids", steamid64)])
            .send()
            .await?
            .error_for_status()?;

        let payload: GetPlayerSummariesEnvelope = response.json().await?;

        if let Some(player) = payload.response.players.into_iter().next() {
            Ok(Some(SteamProfile {
                persona_name: player.personaname,
            }))
        } else {
            Ok(None)
        }
    }

    /// 批量查询 Steam Profile（最多100个）
    pub async fn fetch_profiles_batch(
        &self,
        steamids: &[String],
    ) -> anyhow::Result<HashMap<String, SteamProfile>> {
        if steamids.is_empty() {
            return Ok(HashMap::new());
        }

        let api_key = match self.steam_api_key.as_deref() {
            Some(key) => key,
            None => return Ok(HashMap::new()),
        };

        // 检查缓存，收集需要查询的ID
        let mut result = HashMap::new();
        let mut to_fetch = Vec::new();

        {
            let cache = self.profile_cache.read().await;
            for steamid in steamids {
                if let Some(entry) = cache.get(steamid) {
                    if entry.is_valid() {
                        if let Some(profile) = &entry.value {
                            result.insert(steamid.clone(), profile.clone());
                        }
                        continue;
                    }
                }
                to_fetch.push(steamid.clone());
            }
        }

        if to_fetch.is_empty() {
            return Ok(result);
        }

        // Steam API 支持一次查询多个ID（逗号分隔）
        // 分批查询，每批最多100个
        for chunk in to_fetch.chunks(100) {
            let ids_str = chunk.join(",");

            match self.fetch_profiles_batch_internal(&ids_str, api_key).await {
                Ok(profiles) => {
                    // 更新缓存和结果
                    let mut cache = self.profile_cache.write().await;
                    for steamid in chunk {
                        let profile = profiles.get(steamid).cloned();
                        cache.insert(
                            steamid.clone(),
                            CacheEntry::new(profile.clone(), CACHE_TTL_SECS),
                        );
                        if let Some(p) = profile {
                            result.insert(steamid.clone(), p);
                        }
                    }
                    evict_oldest_if_needed(&mut cache, CACHE_MAX_SIZE);
                }
                Err(e) => {
                    // 批量失败时，缓存空值避免频繁重试
                    let mut cache = self.profile_cache.write().await;
                    for steamid in chunk {
                        cache.insert(steamid.clone(), CacheEntry::new(None, 60));
                    }
                    evict_oldest_if_needed(&mut cache, CACHE_MAX_SIZE);
                    tracing::warn!("批量查询Steam Profile失败: {}", e);
                }
            }
        }

        Ok(result)
    }

    async fn fetch_profiles_batch_internal(
        &self,
        steamids: &str,
        api_key: &str,
    ) -> anyhow::Result<HashMap<String, SteamProfile>> {
        // 优先使用 steamchina
        if let Some(ref china_key) = self.steamchina_profile_key {
            if let Ok(result) = Self::fetch_profiles_batch_from(
                "https://api.steamchina.com/ISteamUser/GetPlayerSummaries/v2/",
                steamids,
                china_key,
            )
            .await
            {
                return Ok(result);
            }
        }

        // 备用：steampowered
        Self::fetch_profiles_batch_from(
            "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/",
            steamids,
            api_key,
        )
        .await
    }

    async fn fetch_profiles_batch_from(
        base_url: &str,
        steamids: &str,
        api_key: &str,
    ) -> anyhow::Result<HashMap<String, SteamProfile>> {
        let response = http_client::http_client()
            .get(base_url)
            .query(&[("key", api_key), ("steamids", steamids)])
            .send()
            .await?
            .error_for_status()?;

        let payload: GetPlayerSummariesEnvelope = response.json().await?;

        let mut result = HashMap::new();
        for player in payload.response.players {
            result.insert(
                player.steamid,
                SteamProfile {
                    persona_name: player.personaname,
                },
            );
        }

        Ok(result)
    }
}

#[derive(Deserialize)]
struct ResolveVanityUrlEnvelope {
    response: ResolveVanityUrlResponse,
}

#[derive(Deserialize)]
struct ResolveVanityUrlResponse {
    success: i32,
    steamid: Option<String>,
}

#[derive(Deserialize)]
struct GetPlayerSummariesEnvelope {
    response: GetPlayerSummariesResponse,
}

#[derive(Deserialize)]
struct GetPlayerSummariesResponse {
    players: Vec<PlayerSummary>,
}

#[derive(Deserialize)]
struct PlayerSummary {
    steamid: String,
    personaname: String,
}

fn is_steamid64(value: &str) -> bool {
    value.len() == 17 && value.chars().all(|char| char.is_ascii_digit())
}

fn build_identity(
    steamid64: String,
    steamid: Option<String>,
    steamid3: Option<String>,
    profile_url: Option<String>,
) -> ParsedSteamIdentity {
    ParsedSteamIdentity {
        steamid64,
        steamid,
        steamid3,
        profile_url,
    }
}

fn steamid2_from_steamid64(steamid64: &str) -> anyhow::Result<String> {
    let z = steamid64
        .parse::<u64>()?
        .checked_sub(STEAM_ID64_BASE)
        .context("无效的 SteamID64")?;
    let y = z / 2;
    let x = z % 2;
    Ok(format!("STEAM_0:{x}:{y}"))
}

/// 将 STEAM_X:Y:Z 格式转换为 SteamID64
pub fn steam2_to_steamid64(steam_id: &str) -> anyhow::Result<String> {
    let mut parts = steam_id.split(':');
    let prefix = parts.next().unwrap_or_default();
    let auth_server = parts.next().unwrap_or_default();
    let account = parts.next().unwrap_or_default();

    anyhow::ensure!(
        prefix == "STEAM_0" || prefix == "STEAM_1",
        "旧 SteamID 格式无效"
    );
    let auth_server = auth_server.parse::<u64>()?;
    let account = account.parse::<u64>()?;
    anyhow::ensure!(auth_server <= 1, "旧 SteamID 格式无效");

    Ok((STEAM_ID64_BASE + account * 2 + auth_server).to_string())
}

fn steamid3_from_steamid64(steamid64: &str) -> anyhow::Result<String> {
    let z = steamid64
        .parse::<u64>()?
        .checked_sub(STEAM_ID64_BASE)
        .context("无效的 SteamID64")?;
    Ok(format!("[U:1:{z}]"))
}

fn profile_url_from_steamid64(steamid64: &str) -> String {
    format!("https://steamcommunity.com/profiles/{steamid64}")
}

fn extract_vanity(input: &str) -> anyhow::Result<String> {
    let captures = re_vanity_url()
        .captures(input)
        .context("无法识别 Steam 标识符格式")?;
    Ok(captures[1].to_string())
}

#[cfg(test)]
mod tests {
    use super::SteamResolver;

    #[test]
    fn parse_steam2_to_steamid64() {
        let resolver = SteamResolver::for_tests();
        let parsed = resolver.parse_local("STEAM_0:1:12345").unwrap();
        assert_eq!(parsed.steamid64, "76561197960290419");
        assert_eq!(parsed.steamid.as_deref(), Some("STEAM_0:1:12345"));
        assert_eq!(parsed.steamid3.as_deref(), Some("[U:1:24691]"));
    }

    #[test]
    fn parse_steam3_to_steamid64() {
        let resolver = SteamResolver::for_tests();
        let parsed = resolver.parse_local("[U:1:24691]").unwrap();
        assert_eq!(parsed.steamid64, "76561197960290419");
        assert_eq!(parsed.steamid.as_deref(), Some("STEAM_0:1:12345"));
        assert_eq!(parsed.steamid3.as_deref(), Some("[U:1:24691]"));
    }

    #[test]
    fn parse_profiles_url_to_steamid64() {
        let resolver = SteamResolver::for_tests();
        let parsed = resolver
            .parse_local("https://steamcommunity.com/profiles/76561198000000001")
            .unwrap();
        assert_eq!(parsed.steamid64, "76561198000000001");
        assert_eq!(parsed.steamid.as_deref(), Some("STEAM_0:1:19867136"));
        assert_eq!(parsed.steamid3.as_deref(), Some("[U:1:39734273]"));
    }

    #[test]
    fn parse_steamid64_to_steamid2_without_network() {
        let resolver = SteamResolver::for_tests();
        let parsed = resolver.parse_local("76561197960290419").unwrap();
        assert_eq!(parsed.steamid64, "76561197960290419");
        assert_eq!(parsed.steamid.as_deref(), Some("STEAM_0:1:12345"));
        assert_eq!(parsed.steamid3.as_deref(), Some("[U:1:24691]"));
    }
}
