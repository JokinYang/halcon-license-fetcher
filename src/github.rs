use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::{LazyLock, Mutex};

const API_BASE: &str = "https://api.github.com";

/// Cache for downloaded zip archives keyed by month string.
/// Populated by `list_files` for zip-based sources, consumed by `download_file`.
static ZIP_CACHE: LazyLock<Mutex<HashMap<String, Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// ── License Source ──────────────────────────────────────────────────

/// License source repository
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LicenseSource {
    /// starain2000/HalconEvalLicenses (zip 格式，默认)
    #[value(name = "eval")]
    Eval,
    /// lovelyyoshino/Halcon_licenses (目录格式)
    #[value(name = "community")]
    Community,
}

impl LicenseSource {
    pub fn repo_owner(self) -> &'static str {
        match self {
            LicenseSource::Eval => "starain2000",
            LicenseSource::Community => "lovelyyoshino",
        }
    }

    pub fn repo_name(self) -> &'static str {
        match self {
            LicenseSource::Eval => "HalconEvalLicenses",
            LicenseSource::Community => "Halcon_licenses",
        }
    }

    /// Whether this source stores licenses inside zip archives
    pub fn is_zip_based(self) -> bool {
        matches!(self, LicenseSource::Eval)
    }

    /// CLI argument fragment for NSSM service `AppParameters`
    pub fn as_cli_arg(self) -> &'static str {
        match self {
            LicenseSource::Eval => "--source eval",
            LicenseSource::Community => "--source community",
        }
    }
}

// ── Data types ──────────────────────────────────────────────────────

/// GitHub API 返回的文件/目录条目（列表用）
#[derive(Debug, Deserialize, Clone)]
pub struct GitHubEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub download_url: Option<String>,
}

/// GitHub API 返回的单个文件内容
#[derive(Debug, Deserialize)]
struct GitHubFileContent {
    pub content: String,
    #[allow(dead_code)]
    pub encoding: String,
}

// ── Public API ──────────────────────────────────────────────────────

/// 获取所有可用的月份文件夹列表（降序，最新在前）
pub async fn list_months(source: LicenseSource) -> Result<Vec<String>> {
    let url = format!(
        "{}/repos/{}/{}/contents/",
        API_BASE,
        source.repo_owner(),
        source.repo_name()
    );

    let client = build_client()?;
    let entries: Vec<GitHubEntry> = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("获取仓库目录失败")?
        .json()
        .await
        .context("解析仓库目录失败")?;

    let mut months: Vec<String> = if source.is_zip_based() {
        entries
            .iter()
            .filter_map(|e| extract_month_from_zip_name(&e.name))
            .collect()
    } else {
        entries
            .iter()
            .filter(|e| e.entry_type == "dir" && is_month_folder(&e.name))
            .map(|e| e.name.clone())
            .collect()
    };

    // 降序 + 去重（zip 源可能同一月份有多个 zip 文件）
    months.sort_by(|a, b| b.cmp(a));
    months.dedup();
    Ok(months)
}

/// 获取指定月份的 license 文件列表
///
/// - 目录源：直接列出文件夹内 .dat 文件
/// - zip 源：下载对应 zip 并列出其中 .dat 文件，同时缓存 zip 内容
pub async fn list_files(source: LicenseSource, month: &str) -> Result<Vec<GitHubEntry>> {
    if source.is_zip_based() {
        return list_files_from_zip(source, month).await;
    }

    let url = format!(
        "{}/repos/{}/{}/contents/{}",
        API_BASE,
        source.repo_owner(),
        source.repo_name(),
        month
    );

    let client = build_client()?;
    let entries: Vec<GitHubEntry> = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context(format!("获取 {month} 目录失败"))?
        .json()
        .await
        .context(format!("解析 {month} 目录失败"))?;

    Ok(entries)
}

/// 下载单个 license 文件
///
/// - 目录源：通过 GitHub Contents API 逐文件下载
/// - zip 源：从缓存的 zip 中提取指定文件
pub async fn download_file(source: LicenseSource, month: &str, filename: &str) -> Result<Vec<u8>> {
    if source.is_zip_based() {
        return extract_from_cached_zip(month, filename);
    }

    let path = format!("{}/{}", month, filename);
    let url = format!(
        "{}/repos/{}/{}/contents/{}",
        API_BASE,
        source.repo_owner(),
        source.repo_name(),
        path
    );

    let client = build_client()?;
    let file_content: GitHubFileContent = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context(format!("下载文件失败: {filename}"))?
        .json()
        .await
        .context(format!("解析文件内容失败: {filename}"))?;

    decode_base64_content(&file_content.content, filename)
}

// ── Zip-based source helpers ────────────────────────────────────────

/// 列出 zip 归档中的 .dat 文件，并缓存 zip 原始字节
async fn list_files_from_zip(source: LicenseSource, month: &str) -> Result<Vec<GitHubEntry>> {
    let zip_data = download_zip_from_source(source, month).await?;
    let cursor = Cursor::new(&zip_data);
    let mut archive =
        zip::ZipArchive::new(cursor).context(format!("解析 {month} 的 zip 文件失败"))?;

    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .context("读取 zip 条目失败")?;
        let full_name = file.name().to_string();
        if full_name.ends_with(".dat") {
            let basename = std::path::Path::new(&full_name)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(full_name);
            entries.push(GitHubEntry {
                name: basename,
                entry_type: "file".to_string(),
                download_url: None,
            });
        }
    }

    // 缓存 zip 数据供后续 download_file 使用
    {
        let mut cache = ZIP_CACHE.lock().unwrap();
        cache.insert(month.to_string(), zip_data);
    }

    Ok(entries)
}

/// 下载指定月份对应的 zip 文件（通过 GitHub Contents API）
async fn download_zip_from_source(source: LicenseSource, month: &str) -> Result<Vec<u8>> {
    // 先列出仓库根目录，找到匹配的 zip 文件名
    let url = format!(
        "{}/repos/{}/{}/contents/",
        API_BASE,
        source.repo_owner(),
        source.repo_name()
    );

    let client = build_client()?;
    let entries: Vec<GitHubEntry> = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("获取仓库目录失败")?
        .json()
        .await
        .context("解析仓库目录失败")?;

    let zip_name = entries
        .iter()
        .find(|e| e.name.starts_with(&format!("{month}_")) && e.name.ends_with(".zip"))
        .map(|e| e.name.clone())
        .ok_or_else(|| anyhow::anyhow!("未找到 {month} 对应的 zip 文件"))?;

    // 通过 Contents API 下载 zip 内容
    let download_url = format!(
        "{}/repos/{}/{}/contents/{}",
        API_BASE,
        source.repo_owner(),
        source.repo_name(),
        zip_name
    );

    let file_content: GitHubFileContent = client
        .get(&download_url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context(format!("下载 zip 失败: {zip_name}"))?
        .json()
        .await
        .context(format!("解析 zip 内容失败: {zip_name}"))?;

    decode_base64_content(&file_content.content, &zip_name)
}

/// 从缓存的 zip 中提取指定 .dat 文件
fn extract_from_cached_zip(month: &str, filename: &str) -> Result<Vec<u8>> {
    let cache = ZIP_CACHE.lock().unwrap();
    let zip_data = cache
        .get(month)
        .ok_or_else(|| anyhow::anyhow!("zip 缓存未命中: {month}（请先调用 list_files）"))?;

    let cursor = Cursor::new(zip_data);
    let mut archive =
        zip::ZipArchive::new(cursor).context(format!("解析缓存的 {month} zip 失败"))?;

    // 按 basename 匹配（zip 内的路径可能带有前缀目录）
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).context("读取 zip 条目失败")?;
        let basename = std::path::Path::new(file.name())
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        if basename == filename {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .context(format!("从 zip 读取文件失败: {filename}"))?;
            return Ok(buf);
        }
    }

    anyhow::bail!("zip 中未找到文件: {filename}")
}

// ── Shared helpers ───────────────────────────────────────────────────

/// 解码 GitHub Contents API 返回的 base64 内容
fn decode_base64_content(content: &str, label: &str) -> Result<Vec<u8>> {
    let cleaned = content.replace('\n', "").replace('\r', "");
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .context(format!("base64 解码失败: {label}"))
}

/// 判断是否为 YYYY.MM 格式的文件夹名
fn is_month_folder(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() != 2 {
        return false;
    }
    if let (Ok(year), Ok(month)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
        year >= 2018 && month >= 1 && month <= 12
    } else {
        false
    }
}

/// 从 zip 文件名提取 YYYY.MM 月份（如 "2026.07_evaluation_support_licenses.zip" → Some("2026.07")）
fn extract_month_from_zip_name(name: &str) -> Option<String> {
    // 取第一个 _ 之前的部分作为候选月份
    let candidate = name.split('_').next()?;
    let dot_parts: Vec<&str> = candidate.split('.').collect();
    if dot_parts.len() != 2 {
        return None;
    }
    if let (Ok(year), Ok(month)) = (dot_parts[0].parse::<u32>(), dot_parts[1].parse::<u32>()) {
        if year >= 2018 && month >= 1 && month <= 12 {
            return Some(candidate.to_string());
        }
    }
    None
}

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("halcon-license-fetcher/0.1")
        .build()
        .context("创建 HTTP 客户端失败")
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_month_folder() {
        assert!(is_month_folder("2026.07"));
        assert!(is_month_folder("2018.02"));
        assert!(!is_month_folder("README.md"));
        assert!(!is_month_folder("2026JulyLicenceByLovelyyoshino"));
    }

    #[test]
    fn test_extract_month_from_zip_name() {
        assert_eq!(
            extract_month_from_zip_name("2026.07_evaluation_support_licenses.zip"),
            Some("2026.07".to_string())
        );
        assert_eq!(
            extract_month_from_zip_name("2026.06_evaluation_support_licenses.zip"),
            Some("2026.06".to_string())
        );
        assert_eq!(extract_month_from_zip_name("README.md"), None);
        assert_eq!(extract_month_from_zip_name("abc_2026.07_test.zip"), None);
    }

    #[test]
    fn test_source_owner_mapping() {
        assert_eq!(LicenseSource::Eval.repo_owner(), "starain2000");
        assert_eq!(LicenseSource::Eval.repo_name(), "HalconEvalLicenses");
        assert_eq!(LicenseSource::Community.repo_owner(), "lovelyyoshino");
        assert_eq!(LicenseSource::Community.repo_name(), "Halcon_licenses");
    }

    #[test]
    fn test_is_zip_based() {
        assert!(LicenseSource::Eval.is_zip_based());
        assert!(!LicenseSource::Community.is_zip_based());
    }

    #[test]
    fn test_source_is_copy() {
        let a = LicenseSource::Eval;
        let b = a;
        assert_eq!(a, b);
    }
}
