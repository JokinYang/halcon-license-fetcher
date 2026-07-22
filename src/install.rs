use anyhow::{Context, Result};
use std::path::Path;

/// 安装 license 文件到 MVTec 产品的 license 目录
///
/// 流程：
/// 1. 确保 license 目录存在
/// 2. 如果已有 license.dat，自动轮转备份 (.bak → .bak.1 → .bak.2)
/// 3. 写入新 license 文件为 license.dat
pub fn install_license(
    data: &[u8],
    product_root: &Path,
    original_filename: &str,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    let license_dir = product_root.join("license");

    if !license_dir.exists() {
        if dry_run {
            println!("[DRY-RUN] 将创建目录: {}", license_dir.display());
        } else {
            std::fs::create_dir_all(&license_dir)
                .context(format!("创建 license 目录失败: {}", license_dir.display()))?;
            println!("[+] 已创建目录: {}", license_dir.display());
        }
    }

    let target_path = license_dir.join("license.dat");

    // 备份旧文件（自动轮转备份）
    if target_path.exists() && !dry_run {
        rotate_backup(&license_dir, force)?;
    }

    if dry_run {
        println!(
            "[DRY-RUN] 将写入: {} (来源: {})",
            target_path.display(),
            original_filename
        );
    } else {
        std::fs::write(&target_path, data)
            .context(format!("写入文件失败: {}", target_path.display()))?;
        println!("[+] 已安装: {}", target_path.display());
    }

    Ok(())
}

/// 轮转备份 license.dat 文件
///
/// license.dat → license.dat.bak
/// license.dat.bak → license.dat.bak.1
/// license.dat.bak.1 → license.dat.bak.2
/// (最多保留 3 个历史备份)
fn rotate_backup(license_dir: &Path, force: bool) -> Result<()> {
    const MAX_BACKUPS: u32 = 3;
    let target = license_dir.join("license.dat");
    let bak = license_dir.join("license.dat.bak");

    if bak.exists() && !force {
        // 轮转旧备份
        for i in (1..MAX_BACKUPS).rev() {
            let old_path = if i == 1 {
                license_dir.join("license.dat.bak")
            } else {
                license_dir.join(format!("license.dat.bak.{}", i - 1))
            };
            let new_path = license_dir.join(format!("license.dat.bak.{}", i));
            if old_path.exists() {
                std::fs::rename(&old_path, &new_path)
                    .context(format!("轮转备份失败: {} → {}", old_path.display(), new_path.display()))?;
            }
        }
        // 现在 .bak 位置已空出，将当前 license.dat 移动过去
        std::fs::rename(&target, &bak)
            .context("备份旧 license.dat 失败")?;
        println!("[+] 已备份旧文件: {}", bak.display());
    } else if !bak.exists() {
        // 简单备份
        std::fs::rename(&target, &bak)
            .context("备份旧 license.dat 失败")?;
        println!("[+] 已备份旧文件: {}", bak.display());
    } else {
        // force = true: 直接覆盖 .bak
        std::fs::rename(&target, &bak)
            .context("备份旧 license.dat 失败")?;
        println!("[+] 已覆盖备份: {}", bak.display());
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
