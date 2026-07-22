mod detect;
mod github;
mod install;
mod license;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// 自动从 GitHub 下载并安装 Halcon 许可证（支持多产品）
#[derive(Parser, Debug)]
#[command(name = "halcon-license-fetcher")]
#[command(version, about, long_about = None)]
struct Cli {
    /// 手动指定 Halcon 安装目录（仅处理该目录，跳过自动扫描）
    #[arg(long)]
    halcon_root: Option<PathBuf>,

    /// 指定月份（默认：使用最新可用月份，格式 YYYY.MM）
    #[arg(long)]
    month: Option<String>,

    /// 仅显示操作，不实际写入文件
    #[arg(long)]
    dry_run: bool,

    /// 强制覆盖，不询问
    #[arg(long)]
    force: bool,

    /// 列出所有可用月份
    #[arg(long)]
    list_months: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // 特殊命令：列出可用月份
    if cli.list_months {
        println!("[*] 获取可用月份列表...");
        let months = github::list_months().await?;
        println!("可用月份（共 {} 个）:", months.len());
        for m in &months {
            println!("  {}", m);
        }
        return Ok(());
    }

    // Step 1: 获取可用月份，确定目标月份
    let months = github::list_months().await?;
    if months.is_empty() {
        anyhow::bail!("仓库中没有可用的月份数据");
    }

    let target_month = if let Some(ref m) = cli.month {
        if !months.contains(m) {
            anyhow::bail!("月份 {m} 不在可用列表中，使用 --list-months 查看所有可用月份");
        }
        m.clone()
    } else {
        let latest = months[0].clone();
        println!("[*] 使用最新月份: {latest}");
        latest
    };

    // Step 2: 获取该月份的所有 license 文件
    println!("[*] 获取 {target_month} 的许可证文件列表...");
    let entries = github::list_files(&target_month).await?;
    let dat_count = entries.iter().filter(|e| e.name.ends_with(".dat")).count();
    println!("    找到 {dat_count} 个 .dat 文件\n");

    // Step 3: 检测所有 MVTec 产品（或使用手动指定的路径）
    let products: Vec<detect::MvtecProduct> = if let Some(ref root) = cli.halcon_root {
        println!("[*] 使用指定路径: {}", root.display());
        let version = detect::extract_version_from_path(root)
            .ok_or_else(|| anyhow::anyhow!("无法从路径 {} 提取版本号", root.display()))?;
        let kind = detect::ProductKind::from_dir_name(
            &root.file_name().unwrap_or_default().to_string_lossy(),
        );
        let product = detect::MvtecProduct {
            root: root.clone(),
            version,
            kind,
        };
        println!("    类型: {}\n", product.kind.label());
        vec![product]
    } else {
        println!("[*] 正在扫描 MVTec 产品...");
        let products = detect::find_all_products()?;
        println!("    发现 {} 个产品:", products.len());
        for p in &products {
            println!("      {} {} ({})", p.version, p.kind.label(), p.root.display());
        }
        println!();
        products
    };

    // Step 4: 为每个产品下载并安装 license
    let mut success = 0;
    let mut skipped = 0;

    for product in &products {
        println!(
            "--- {} {} ---",
            product.version,
            product.kind.label()
        );
        println!("  路径: {}", product.root.display());

        match license::pick_license_for_product(&entries, product) {
            Some(entry) => {
                let desc = license::license_description(&entry.name);

                if license::is_exact_match(&entry.name, &product.version) {
                    println!("  [*] 精确匹配: {}", entry.name);
                } else {
                    println!("  [*] 兼容匹配: {} ({})", entry.name, desc);
                }

                // 下载
                match github::download_file(&target_month, &entry.name).await {
                    Ok(data) => {
                        println!("  [*] 已下载 {} 字节", data.len());

                        // 安装
                        match install::install_license(
                            &data,
                            &product.root,
                            &entry.name,
                            cli.dry_run,
                            cli.force,
                        ) {
                            Ok(()) => {
                                println!("  [✓] 已安装\n");
                                success += 1;
                            }
                            Err(e) => {
                                eprintln!("  [!] 安装失败: {e}\n");
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("  [!] 下载失败: {e}\n");
                    }
                }
            }
            None => {
                println!("  [!] 未找到匹配的许可证，跳过\n");
                skipped += 1;
            }
        }
    }

    // Step 5: 总结
    let total = products.len();
    if cli.dry_run {
        println!("[DRY-RUN] 以上是将要执行的操作（{total} 个产品），实际文件未被修改");
    } else {
        println!(
            "[✓] 完成! {success}/{total} 个产品已安装许可证{}",
            if skipped > 0 {
                format!("（{} 个跳过）", skipped)
            } else {
                String::new()
            }
        );
    }

    Ok(())
}
