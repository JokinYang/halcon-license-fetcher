use anyhow::{Context, Result};
use std::path::Path;

/// 将多个 license 文件合并安装到单个 license.dat
///
/// 流程：
/// 1. 确保 license 目录存在
/// 2. 如果已有 license.dat，自动轮转备份
/// 3. 合并所有 license 内容，写入 license.dat
pub fn install_license_bundle(
    datas: &[Vec<u8>],
    product_root: &Path,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    let license_dir = product_root.join("license");

    if !license_dir.exists() {
        if dry_run {
            println!("  [DRY-RUN] 将创建目录: {}", license_dir.display());
        } else {
            std::fs::create_dir_all(&license_dir)
                .context(format!("创建 license 目录失败: {}", license_dir.display()))?;
            println!("  [+] 已创建目录: {}", license_dir.display());
        }
    }

    let target_path = license_dir.join("license.dat");

    // 备份旧文件
    if target_path.exists() && !dry_run {
        rotate_backup(&license_dir, force)?;
    }

    // 合并内容
    let merged = datas.join(&b'\n');

    if dry_run {
        println!(
            "  [DRY-RUN] 将写入: {} ({} 个文件合并, {} 字节)",
            target_path.display(),
            datas.len(),
            merged.len()
        );
    } else {
        std::fs::write(&target_path, &merged)
            .context(format!("写入文件失败: {}", target_path.display()))?;
        println!(
            "  [+] 已安装: {} ({} 个许可证合并, {} 字节)",
            target_path.display(),
            datas.len(),
            merged.len()
        );
    }

    Ok(())
}

/// 轮转备份 license.dat 文件（最多保留 3 个历史）
fn rotate_backup(license_dir: &Path, force: bool) -> Result<()> {
    const MAX_BACKUPS: u32 = 3;
    let target = license_dir.join("license.dat");
    let bak = license_dir.join("license.dat.bak");

    if bak.exists() && !force {
        for i in (1..MAX_BACKUPS).rev() {
            let old_path = if i == 1 {
                license_dir.join("license.dat.bak")
            } else {
                license_dir.join(format!("license.dat.bak.{}", i - 1))
            };
            let new_path = license_dir.join(format!("license.dat.bak.{}", i));
            if old_path.exists() {
                std::fs::rename(&old_path, &new_path).context(format!(
                    "轮转备份失败: {} → {}",
                    old_path.display(),
                    new_path.display()
                ))?;
            }
        }
        std::fs::rename(&target, &bak)
            .context("备份旧 license.dat 失败")?;
        println!("  [+] 已备份旧文件: {}", bak.display());
    } else if !bak.exists() {
        std::fs::rename(&target, &bak)
            .context("备份旧 license.dat 失败")?;
        println!("  [+] 已备份旧文件: {}", bak.display());
    } else {
        std::fs::rename(&target, &bak)
            .context("备份旧 license.dat 失败")?;
        println!("  [+] 已覆盖备份: {}", bak.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn test_rotate_naming() {
        let dir = PathBuf::from(r"C:\test");
        let bak0 = dir.join("license.dat.bak");
        let bak1 = dir.join("license.dat.bak.1");
        let bak2 = dir.join("license.dat.bak.2");

        assert_eq!(bak0.file_name().unwrap(), "license.dat.bak");
        assert_eq!(bak1.file_name().unwrap(), "license.dat.bak.1");
        assert_eq!(bak2.file_name().unwrap(), "license.dat.bak.2");
    }
}
