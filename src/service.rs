use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_NAME: &str = "HalconLicenseFetcher";
const NSSM_BYTES: &[u8] = include_bytes!("../assets/nssm.exe");

// ── service install ──────────────────────────────────────────

/// 注册 Windows 服务
pub fn install(exe_path: &Path, nssm_path: Option<&Path>, interval_days: u64) -> Result<()> {
    let nssm = ensure_nssm(nssm_path)?;
    println!("[*] NSSM: {}", nssm.display());

    // 检查管理员权限
    if !is_admin() {
        anyhow::bail!("安装服务需要管理员权限，请以管理员身份运行");
    }

    // 如果服务已存在，先询问
    if service_exists() {
        anyhow::bail!(
            "服务 {SERVICE_NAME} 已存在。请先运行 'service remove' 移除旧服务，或使用 sc 命令手动管理"
        );
    }

    let exe_str = exe_path.to_string_lossy();
    let exe_dir = exe_path.parent().unwrap_or(Path::new("."));

    // 安装服务
    nssm_run(&nssm, &["install", SERVICE_NAME, &exe_str])?;
    nssm_set(&nssm, "AppParameters", &format!("service run --interval {interval_days}"))?;
    nssm_set(&nssm, "DisplayName", "Halcon License Fetcher")?;
    nssm_set(&nssm, "Description", "自动更新 MVTec 产品许可证 - 每 7 天检查一次")?;
    nssm_set(&nssm, "Start", "SERVICE_AUTO_START")?;

    // 日志输出到 exe 同目录
    let stdout_log = exe_dir.join("service_stdout.log");
    let stderr_log = exe_dir.join("service_stderr.log");
    nssm_set(&nssm, "AppStdout", &stdout_log.to_string_lossy())?;
    nssm_set(&nssm, "AppStderr", &stderr_log.to_string_lossy())?;

    // 启动服务
    nssm_run(&nssm, &["start", SERVICE_NAME])?;

    println!("[✓] 服务已安装并启动");
    println!("    服务名: {SERVICE_NAME}");
    println!("    启动类型: 自动");
    println!("    检查间隔: 每 {interval_days} 天");
    println!("    日志: {}", stdout_log.display());
    println!("");
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

    // 停止服务
    println!("[*] 正在停止服务...");
    let _ = nssm_run(&nssm, &["stop", SERVICE_NAME]);

    // 移除服务
    println!("[*] 正在移除服务...");
    nssm_run(&nssm, &["remove", SERVICE_NAME, "confirm"])?;

    println!("[✓] 服务已移除");

    Ok(())
}

/// 服务主循环（由 NSSM 调用）
pub async fn run_loop<F, Fut>(interval_days: u64, check_fn: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let interval = chrono::Duration::days(interval_days as i64);

    loop {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        eprintln!("[{now}] 开始检查 license...");

        match check_fn().await {
            Ok(()) => {
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                eprintln!("[{now}] 检查完成，下次检查: {} 天后", interval_days);
            }
            Err(e) => {
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                eprintln!("[{now}] 检查失败: {e}");
                eprintln!("[{now}] 将在 {interval_days} 天后重试");
            }
        }

        tokio::time::sleep(interval.to_std().unwrap()).await;
    }
}

// ── NSSM 内部辅助 ──────────────────────────────────────────

/// 确保 nssm.exe 可用：优先使用指定路径，否则从嵌入资源提取
fn ensure_nssm(nssm_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = nssm_path {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        anyhow::bail!("指定的 NSSM 路径不存在: {}", path.display());
    }

    // 优先同目录已有的 nssm.exe
    let exe_dir = std::env::current_exe()
        .context("获取 exe 路径失败")?
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let local_nssm = exe_dir.join("nssm.exe");
    if local_nssm.exists() {
        return Ok(local_nssm);
    }

    // 从嵌入资源提取
    println!("[*] 正在提取 NSSM...");
    std::fs::write(&local_nssm, NSSM_BYTES)
        .context(format!("提取 NSSM 失败: {}", local_nssm.display()))?;
    Ok(local_nssm)
}

/// 执行 nssm 命令（不需要输出）
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

/// 执行 nssm set 命令
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

/// 检查服务是否已存在
fn service_exists() -> bool {
    let output = Command::new("sc")
        .args(["query", SERVICE_NAME])
        .output();
    output.map(|o| o.status.success()).unwrap_or(false)
}

/// 检查是否以管理员权限运行
fn is_admin() -> bool {
    // 简单检测: 尝试读取一个需要管理员权限的注册表键
    use winreg::enums::*;
    winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion", KEY_READ)
        .is_ok()
}
