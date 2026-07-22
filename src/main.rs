mod detect;
mod github;
mod install;
mod license;
mod service;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "halcon-license-fetcher")]
#[command(version, about = "自动下载并安装 MVTec 产品许可证")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 手动指定 MVTec 安装目录（仅处理该目录，跳过自动扫描）
    #[arg(long, global = true)]
    halcon_root: Option<PathBuf>,

    /// 指定月份（默认：最新，格式 YYYY.MM）
    #[arg(long, global = true)]
    month: Option<String>,

    /// 仅显示操作，不实际写入文件
    #[arg(long, global = true)]
    dry_run: bool,

    /// 强制覆盖备份
    #[arg(long, global = true)]
    force: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 列出所有可用月份
    ListMonths,

    /// Windows 服务管理（需要管理员权限）
    #[command(name = "service")]
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand, Debug)]
enum ServiceAction {
    /// 注册为 Windows 服务（开机自启，每月固定日期执行）
    Install {
        /// 每月几号执行（1-28，逗号分隔，默认 1）
        #[arg(long, default_value = "1")]
        days: String,
        /// 手动指定 nssm.exe 路径
        #[arg(long)]
        nssm_path: Option<PathBuf>,
    },
    /// 移除 Windows 服务
    Remove {
        /// 手动指定 nssm.exe 路径
        #[arg(long)]
        nssm_path: Option<PathBuf>,
    },
    /// 服务入口（由 NSSM 调用，不直接使用）
    Run {
        /// 每月几号执行（1-28，逗号分隔）
        #[arg(long, default_value = "1")]
        days: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::ListMonths) => {
            let months = github::list_months().await?;
            println!("可用月份（共 {} 个）:", months.len());
            for m in &months {
                println!("  {m}");
            }
        }
        Some(Commands::Service { action }) => match action {
            ServiceAction::Install { days, nssm_path } => {
                let exe = std::env::current_exe()?;
                let d = service::parse_days(days)?;
                service::install(&exe, nssm_path.as_deref(), &d)?;
            }
            ServiceAction::Remove { nssm_path } => {
                service::remove(nssm_path.as_deref())?;
            }
            ServiceAction::Run { days } => {
                let d = service::parse_days(days)?;
                service::run_loop(d, || run_license_check(&cli, true)).await;
            }
        },
        None => {
            // 默认行为: 手动运行一次
            run_license_check(&cli, false).await?;
        }
    }

    Ok(())
}

/// 核心逻辑: 扫描产品 → 匹配 license → 下载 → 安装
///
/// `quiet` 模式下输出适合服务日志的简洁格式
pub(crate) async fn run_license_check(cli: &Cli, quiet: bool) -> Result<()> {
    // 1. 获取月份
    let months = github::list_months().await?;
    if months.is_empty() {
        anyhow::bail!("仓库中没有可用的月份数据");
    }

    let target_month = if let Some(ref m) = cli.month {
        if !months.contains(m) {
            anyhow::bail!("月份 {m} 不在可用列表中");
        }
        m.clone()
    } else {
        months[0].clone()
    };

    if !quiet {
        println!("[*] 使用最新月份: {target_month}");
    }

    // 2. 获取 license 文件列表
    if !quiet {
        println!("[*] 获取 {target_month} 的许可证文件列表...");
    }
    let entries = github::list_files(&target_month).await?;

    if !quiet {
        let dat_count = entries.iter().filter(|e| e.name.ends_with(".dat")).count();
        println!("    找到 {dat_count} 个 .dat 文件\n");
    }

    // 3. 检测产品
    let products: Vec<detect::MvtecProduct> = if let Some(ref root) = cli.halcon_root {
        let version = detect::extract_version_from_path(root)
            .ok_or_else(|| anyhow::anyhow!("无法从路径 {} 提取版本号", root.display()))?;
        let kind = detect::ProductKind::from_dir_name(
            &root.file_name().unwrap_or_default().to_string_lossy(),
        );
        vec![detect::MvtecProduct {
            root: root.clone(),
            version,
            kind,
        }]
    } else {
        detect::find_all_products()?
    };

    if !quiet {
        println!("[*] 发现 {} 个产品:", products.len());
        for p in &products {
            println!(
                "      {} {} ({})",
                p.version,
                p.kind.label(),
                p.root.display()
            );
        }
        println!();
    }

    // 4. 处理每个产品
    let mut success = 0;
    let mut download_cache: HashMap<String, Vec<u8>> = HashMap::new();

    for product in &products {
        if !quiet {
            println!(
                "--- {} {} ---",
                product.version,
                product.kind.label()
            );
            println!("  路径: {}", product.root.display());
        }

        // 检查当前 license 是否仍然有效
        if install::is_license_valid(&product.root) {
            if !quiet {
                println!("  [✓] 当前 license 有效，跳过");
            }
            success += 1;
            if !quiet {
                println!();
            }
            continue;
        }

        let matched = license::pick_licenses_for_product(&entries, product);
        if matched.is_empty() {
            if !quiet {
                println!("  [!] 未找到匹配的许可证，跳过\n");
            }
            continue;
        }

        if !quiet {
            let descs = license::describe_bundle(&matched);
            println!("  [*] 将合并 {} 个许可证:", matched.len());
            for (entry, desc) in matched.iter().zip(descs.iter()) {
                println!("      {} ({})", entry.name, desc);
            }
        }

        // 下载
        let mut datas: Vec<Vec<u8>> = Vec::new();
        let mut all_ok = true;
        for entry in &matched {
            if let Some(cached) = download_cache.get(&entry.name) {
                datas.push(cached.clone());
            } else {
                match github::download_file(&target_month, &entry.name).await {
                    Ok(data) => {
                        download_cache.insert(entry.name.clone(), data.clone());
                        datas.push(data);
                    }
                    Err(e) => {
                        if !quiet {
                            eprintln!("      ✗ 下载失败: {} — {e}", entry.name);
                        }
                        all_ok = false;
                    }
                }
            }
        }

        if !all_ok || datas.is_empty() {
            if !quiet {
                println!("  [!] 下载未完全成功，跳过安装\n");
            }
            continue;
        }

        // 安装
        match install::install_license_bundle(&datas, &product.root, cli.dry_run, cli.force) {
            Ok(()) => {
                success += 1;
            }
            Err(e) => {
                if !quiet {
                    eprintln!("  [!] 安装失败: {e}\n");
                }
            }
        }
    }

    if !quiet {
        let total = products.len();
        if cli.dry_run {
            println!("[DRY-RUN] 以上是将要执行的操作（{total} 个产品），实际文件未被修改");
        } else {
            println!("[✓] 完成! {success}/{total} 个产品已安装许可证");
        }
    }

    Ok(())
}
