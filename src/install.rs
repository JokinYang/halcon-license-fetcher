use anyhow::{Context, Result};
use std::path::Path;

/// 将多个 license 文件合并安装到单个 license.dat
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

    if target_path.exists() && !dry_run {
        rotate_backup(&license_dir, force)?;
    }

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
        std::fs::rename(&target, &bak).context("备份旧 license.dat 失败")?;
        println!("  [+] 已备份旧文件: {}", bak.display());
    } else if !bak.exists() {
        std::fs::rename(&target, &bak).context("备份旧 license.dat 失败")?;
        println!("  [+] 已备份旧文件: {}", bak.display());
    } else {
        std::fs::rename(&target, &bak).context("备份旧 license.dat 失败")?;
        println!("  [+] 已覆盖备份: {}", bak.display());
    }

    Ok(())
}

/// 检查产品当前的 license.dat 是否仍然有效
///
/// 解析 LICENSE 行中的 VALID=YYYY-MM-DD，只要有一个未过期即返回 true。
pub fn is_license_valid(product_root: &Path) -> bool {
    let license_path = product_root.join("license").join("license.dat");
    let content = match std::fs::read_to_string(&license_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let today = chrono::Local::now().date_naive();

    for line in content.lines() {
        if let Some(pos) = line.find("VALID=") {
            let rest = &line[pos + 6..];
            let date_str: String = rest.chars().take(10).collect();
            if let Ok(valid_date) = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
                if valid_date >= today {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use std::path::PathBuf;

    #[test]
    fn test_rotate_naming() {
        let dir = PathBuf::from(r"C:\test");
        assert_eq!(
            dir.join("license.dat.bak").file_name().unwrap(),
            "license.dat.bak"
        );
        assert_eq!(
            dir.join("license.dat.bak.1").file_name().unwrap(),
            "license.dat.bak.1"
        );
    }

    #[test]
    fn test_license_valid() {
        let dir = std::env::temp_dir().join("hlf_test_valid");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("license")).unwrap();
        let next_year = chrono::Local::now().year() + 1;
        std::fs::write(
            dir.join("license").join("license.dat"),
            format!("LICENSE MVTec_HALCON 26.08 VALID={next_year}-08-01 ID=TEST SIGNATURE=abc"),
        )
        .unwrap();
        assert!(is_license_valid(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_license_expired() {
        let dir = std::env::temp_dir().join("hlf_test_expired");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("license")).unwrap();
        std::fs::write(
            dir.join("license").join("license.dat"),
            "LICENSE MVTec_HALCON 26.08 VALID=2020-01-01 ID=TEST SIGNATURE=abc",
        )
        .unwrap();
        assert!(!is_license_valid(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_license_missing() {
        let dir = std::env::temp_dir().join("hlf_test_nonexistent");
        assert!(!is_license_valid(&dir));
    }
}
