use anyhow::{Context, Result};
use serde::Deserialize;

const REPO_OWNER: &str = "lovelyyoshino";
const REPO_NAME: &str = "Halcon_licenses";
const API_BASE: &str = "https://api.github.com";

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

/// 获取所有可用的月份文件夹列表（降序，最新在前）
pub async fn list_months() -> Result<Vec<String>> {
    let url = format!(
        "{}/repos/{}/{}/contents/",
        API_BASE, REPO_OWNER, REPO_NAME
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

    let mut months: Vec<String> = entries
        .iter()
        .filter(|e| e.entry_type == "dir" && is_month_folder(&e.name))
        .map(|e| e.name.clone())
        .collect();

    // 降序排列：最新月份在前
    months.sort_by(|a, b| b.cmp(a));
    Ok(months)
}

/// 获取指定月份的文件夹内容
pub async fn list_files(month: &str) -> Result<Vec<GitHubEntry>> {
    let url = format!(
        "{}/repos/{}/{}/contents/{}",
        API_BASE, REPO_OWNER, REPO_NAME, month
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

/// 通过 GitHub Contents API 下载文件内容（使用 api.github.com，避免 raw.githubusercontent.com 被墙）
pub async fn download_file(month: &str, filename: &str) -> Result<Vec<u8>> {
    let path = format!("{}/{}", month, filename);
    let url = format!(
        "{}/repos/{}/{}/contents/{}",
        API_BASE, REPO_OWNER, REPO_NAME, path
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

    // GitHub API 返回 base64 编码的内容（可能包含换行符需要清理）
    let cleaned = file_content.content.replace('\n', "").replace('\r', "");
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .context(format!("base64 解码失败: {filename}"))?;

    Ok(decoded)
}

/// 判断是否为 YYYY.MM 格式的文件夹
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

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("halcon-license-fetcher/0.1")
        .build()
        .context("创建 HTTP 客户端失败")
}

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
}
