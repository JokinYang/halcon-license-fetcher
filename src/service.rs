use anyhow::{Context, Result};
use chrono::Datelike;
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_NAME: &str = "HalconLicenseFetcher";
const NSSM_BYTES: &[u8] = include_bytes!("../assets/nssm.exe");

// ── service install ──────────────────────────────────────────

/// 注册 Windows 服务
pub fn install(exe_path: &Path, nssm_path: Option<&Path>, days: &[u32]) -> Result<()> {
    let nssm = ensure_nssm(nssm_path)?;
    println!("[*] NSSM: {}", nssm.display());

    if !is_admin() {
        anyhow::bail!("安装服务需要管理员权限，请以管理员身份运行");
    }

    if service_exists() {
        anyhow::bail!(
            "服务 {SERVICE_NAME} 已存在。请先运行 'service remove' 移除旧服务，或使用 sc 命令手动管理"
        );
    }

    let exe_str = exe_path.to_string_lossy();
    let exe_dir = exe_path.parent().unwrap_or(Path::new("."));

    let days_str = days.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",");
    let desc = days_desc(days);

    // 安装服务
    nssm_run(&nssm, &["install", SERVICE_NAME, &exe_str])?;
    nssm_set(&nssm, "AppParameters", &format!("service run --days {days_str}"))?;
    nssm_set(&nssm, "DisplayName", "Halcon License Fetcher")?;
    nssm_set(&nssm, "Description", &format!("自动更新 MVTec 产品许可证 - {desc}"))?;
    nssm_set(&nssm, "Start", "SERVICE_AUTO_START")?;

    let stdout_log = exe_dir.join("service_stdout.log");
    let stderr_log = exe_dir.join("service_stderr.log");
    nssm_set(&nssm, "AppStdout", &stdout_log.to_string_lossy())?;
    nssm_set(&nssm, "AppStderr", &stderr_log.to_string_lossy())?;

    nssm_run(&nssm, &["start", SERVICE_NAME])?;

    println!("[✓] 服务已安装并启动");
    println!("    服务名: {SERVICE_NAME}");
    println!("    启动类型: 自动");
    println!("    执行计划: {desc}");
    println!("    日志: {}", stdout_log.display());
    println!();
    println!("    可以使用以下命令管理:");
    println!("      sc stop {SERVICE_NAME}");
    println!("      sc start {SERVICE_NAME}");
    println!("      halcon-license-fetcher service remove");

    Ok(())
}

/// 移除 Windows 服务
pub fn remove(nssm_path: Option<&Path>) -> Result<()> {
    let nssm = ensure_nssm(nssm_path)?;

    if !is_admin() {
        anyhow::bail!("移除服务需要管理员权限，请以管理员身份运行");
    }

    if !service_exists() {
        println!("[*] 服务 {SERVICE_NAME} 不存在，无需移除");
        return Ok(());
    }

    println!("[*] 正在停止服务...");
    let _ = nssm_run(&nssm, &["stop", SERVICE_NAME]);

    println!("[*] 正在移除服务...");
    nssm_run(&nssm, &["remove", SERVICE_NAME, "confirm"])?;

    println!("[✓] 服务已移除");
    Ok(())
}

/// 服务主循环（由 NSSM 调用）
pub async fn run_loop<F, Fut>(days: Vec<u32>, check_fn: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    loop {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        eprintln!("[{now}] 开始检查 license...");

        match check_fn().await {
            Ok(()) => {
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                let next = next_run_desc(&days);
                eprintln!("[{now}] 检查完成，{next}");
            }
            Err(e) => {
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                let next = next_run_desc(&days);
                eprintln!("[{now}] 检查失败: {e}");
                eprintln!("[{now}] {next} 重试");
            }
        }

        let secs = seconds_until_next_run(&days);
        tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await;
    }
}

// ── 调度计算 ────────────────────────────────────────────────

/// 计算距离下次执行还剩多少秒
fn seconds_until_next_run(days: &[u32]) -> i64 {
    let now = chrono::Local::now();
    let today = now.date_naive();
    let mut sorted: Vec<u32> = days.to_vec();
    sorted.sort();

    // 查找本月还未过的最近日期
    for day in &sorted {
        if *day > today.day() {
            if let Some(target) = today.with_day(*day) {
                if let Some(target_dt) = target.and_hms_opt(0, 0, 0) {
                    let delta = target_dt - now.naive_local();
                    return delta.num_seconds().max(60);
                }
            }
        }
    }

    // 本月所有目标日已过，取下月第一个
    let first = sorted.first().copied().unwrap_or(1);
    let (next_year, next_month) = if today.month() == 12 {
        (today.year() + 1, 1)
    } else {
        (today.year(), today.month() + 1)
    };

    if let Some(next) = chrono::NaiveDate::from_ymd_opt(next_year, next_month, first) {
        if let Some(target_dt) = next.and_hms_opt(0, 0, 0) {
            let delta = target_dt - now.naive_local();
            return delta.num_seconds().max(60);
        }
    }

    86400 // fallback: 1 天
}

fn next_run_desc(days: &[u32]) -> String {
    let now = chrono::Local::now();
    let today = now.date_naive();
    let mut sorted: Vec<u32> = days.to_vec();
    sorted.sort();

    for day in &sorted {
        if *day > today.day() {
            if let Some(target) = today.with_day(*day) {
                return format!("下次执行: {}", target.format("%m月%d日"));
            }
        }
    }

    let first = sorted.first().copied().unwrap_or(1);
    format!("下次执行: 下月{}日", first)
}

fn days_desc(days: &[u32]) -> String {
    let mut sorted: Vec<u32> = days.to_vec();
    sorted.sort();
    let list: Vec<String> = sorted.iter().map(|d| format!("{}日", d)).collect();
    format!("每月 {}", list.join("、"))
}

/// 解析 --days 参数
pub fn parse_days(s: &str) -> Result<Vec<u32>> {
    let mut days = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        let d: u32 = part
            .parse()
            .with_context(|| format!("无效日期: {part}"))?;
        if d < 1 || d > 28 {
            anyhow::bail!("日期必须在 1-28 之间（避免月末缺失）: {d}");
        }
        days.push(d);
    }
    if days.is_empty() {
        anyhow::bail!("至少指定一个日期");
    }
    Ok(days)
}

// ── NSSM 内部辅助 ──────────────────────────────────────────

fn ensure_nssm(nssm_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = nssm_path {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        anyhow::bail!("指定的 NSSM 路径不存在: {}", path.display());
    }

    let exe_dir = std::env::current_exe()
        .context("获取 exe 路径失败")?
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let local_nssm = exe_dir.join("nssm.exe");
    if local_nssm.exists() {
        return Ok(local_nssm);
    }

    println!("[*] 正在提取 NSSM...");
    std::fs::write(&local_nssm, NSSM_BYTES)
        .context(format!("提取 NSSM 失败: {}", local_nssm.display()))?;
    Ok(local_nssm)
}

fn nssm_run(nssm: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new(nssm)
        .args(args)
        .output()
        .context(format!("执行 nssm {} 失败", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("nssm {} 失败:\n{stderr}", args.join(" "));
    }
    Ok(())
}

fn nssm_set(nssm: &Path, param: &str, value: &str) -> Result<()> {
    let output = Command::new(nssm)
        .args(["set", SERVICE_NAME, param, value])
        .output()
        .context(format!("nssm set {param} 失败"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("nssm set {param} 失败:\n{stderr}");
    }
    Ok(())
}

fn service_exists() -> bool {
    Command::new("sc")
        .args(["query", SERVICE_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn is_admin() -> bool {
    use winreg::enums::*;
    winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion", KEY_READ)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_days() {
        assert_eq!(parse_days("1").unwrap(), vec![1]);
        assert_eq!(parse_days("1,15").unwrap(), vec![1, 15]);
        assert!(parse_days("0").is_err());
        assert!(parse_days("29").is_err());
        assert!(parse_days("abc").is_err());
    }

    #[test]
    fn test_seconds_until_next() {
        // 至少应返回正值
        let s = seconds_until_next_run(&[1, 15]);
        assert!(s > 0);
    }
}
