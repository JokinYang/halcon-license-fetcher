use crate::detect::{ProductKind, MvtecProduct};
use crate::github::GitHubEntry;

/// 为 MVTec 产品匹配合适的 license 文件
///
/// 不同产品类型的匹配策略：
/// - DeepLearningTool: license_eval_dlt_*.dat
/// - HalconProgress: license_support_halcon_progress_*.dat
/// - HalconSteady: license_support_halcon{VER}_steady_*.dat (精确匹配)
/// - HalconSteadyDL: license_support_halcon{VER}_steady-dl_*.dat (优先 DL)
/// - HalconOther: 精确匹配 → 版本降级 → Progress fallback
pub fn pick_license_for_product<'a>(
    entries: &'a [GitHubEntry],
    product: &MvtecProduct,
) -> Option<&'a GitHubEntry> {
    let dat_files: Vec<&GitHubEntry> = entries
        .iter()
        .filter(|e| e.name.ends_with(".dat"))
        .collect();

    match product.kind {
        ProductKind::DeepLearningTool => pick_dlt_license(&dat_files),
        ProductKind::HalconProgress => pick_progress_license(&dat_files, &product.version),
        ProductKind::HalconSteady => pick_steady_license(&dat_files, &product.version),
        ProductKind::HalconSteadyDL => pick_steady_dl_license(&dat_files, &product.version),
        ProductKind::HalconOther => pick_generic_halcon_license(&dat_files, &product.version),
        ProductKind::Other => pick_generic_halcon_license(&dat_files, &product.version),
    }
}

/// DeepLearningTool 专属 license
fn pick_dlt_license<'a>(dat_files: &[&'a GitHubEntry]) -> Option<&'a GitHubEntry> {
    // 优先: license_eval_dlt_{date}.dat
    dat_files
        .iter()
        .find(|e| e.name.to_lowercase().contains("eval_dlt"))
        .copied()
}

/// Progress 版本: 优先精确匹配，否则用通用 Progress license
fn pick_progress_license<'a>(
    dat_files: &[&'a GitHubEntry],
    version: &str,
) -> Option<&'a GitHubEntry> {
    // 1. 尝试精确版本 + progress
    let version_key = format!("halcon{}", version);
    if let Some(entry) = dat_files.iter().find(|e| {
        let lower = e.name.to_lowercase();
        lower.contains(&version_key) && lower.contains("progress")
    }) {
        return Some(entry);
    }

    // 2. 通用 Progress license: license_support_halcon_progress_*
    if let Some(entry) = dat_files.iter().find(|e| {
        let lower = e.name.to_lowercase();
        lower.contains("support_halcon_progress") && !lower.contains("eval")
    }) {
        return Some(entry);
    }

    // 3. 评估版 Progress license: license_eval_halcon_progress_*
    dat_files
        .iter()
        .find(|e| e.name.to_lowercase().contains("eval_halcon_progress"))
        .copied()
}

/// Steady 版本: 精确匹配
fn pick_steady_license<'a>(
    dat_files: &[&'a GitHubEntry],
    version: &str,
) -> Option<&'a GitHubEntry> {
    let version_key = format!("halcon{}", version);

    // 1. license_support_halcon{VER}_steady_ (不含 -dl)
    if let Some(entry) = dat_files.iter().find(|e| {
        let lower = e.name.to_lowercase();
        lower.contains(&version_key)
            && lower.contains("_steady_")
            && !lower.contains("steady-dl")
    }) {
        return Some(entry);
    }

    // 2. 降级匹配: 尝试小于等于当前版本的 steady license
    let fallback = collect_modern_versions(dat_files);
    for fv in &fallback {
        if crate::detect::compare_versions(fv, version) != std::cmp::Ordering::Greater {
            if let Some(entry) = dat_files.iter().find(|e| {
                let key = format!("halcon{}", fv);
                let lower = e.name.to_lowercase();
                lower.contains(&key)
                    && lower.contains("_steady_")
                    && !lower.contains("steady-dl")
            }) {
                return Some(entry);
            }
        }
    }

    None
}

/// Steady + DL 版本: 优先 steady-dl，回退到 steady
fn pick_steady_dl_license<'a>(
    dat_files: &[&'a GitHubEntry],
    version: &str,
) -> Option<&'a GitHubEntry> {
    let version_key = format!("halcon{}", version);

    // 1. license_support_halcon{VER}_steady-dl_
    if let Some(entry) = dat_files.iter().find(|e| {
        let lower = e.name.to_lowercase();
        lower.contains(&version_key) && lower.contains("_steady-dl_")
    }) {
        return Some(entry);
    }

    // 2. license_support_halcon{VER}_steady_ (不含 -dl)
    if let Some(entry) = dat_files.iter().find(|e| {
        let lower = e.name.to_lowercase();
        lower.contains(&version_key)
            && lower.contains("_steady_")
            && !lower.contains("steady-dl")
    }) {
        return Some(entry);
    }

    // 3. 降级匹配
    let fallback = collect_modern_versions(dat_files);
    for fv in &fallback {
        if crate::detect::compare_versions(fv, version) != std::cmp::Ordering::Greater {
            // 先试 steady-dl
            if let Some(entry) = dat_files.iter().find(|e| {
                let key = format!("halcon{}", fv);
                let lower = e.name.to_lowercase();
                lower.contains(&key) && lower.contains("_steady-dl_")
            }) {
                return Some(entry);
            }
            // 再试 steady
            if let Some(entry) = dat_files.iter().find(|e| {
                let key = format!("halcon{}", fv);
                let lower = e.name.to_lowercase();
                lower.contains(&key)
                    && lower.contains("_steady_")
                    && !lower.contains("steady-dl")
            }) {
                return Some(entry);
            }
        }
    }

    None
}

/// 通用 Halcon 匹配（未知子类型）
fn pick_generic_halcon_license<'a>(
    dat_files: &[&'a GitHubEntry],
    version: &str,
) -> Option<&'a GitHubEntry> {
    let version_key = format!("halcon{}", version);

    // 1. license_support_halcon{VER}_steady_ (不含 -dl)
    if let Some(entry) = dat_files.iter().find(|e| {
        let lower = e.name.to_lowercase();
        lower.contains(&version_key)
            && lower.contains("_steady_")
            && !lower.contains("steady-dl")
    }) {
        return Some(entry);
    }

    // 2. license_support_halcon{VER}_ 基础 support (不含 steady)
    if let Some(entry) = dat_files.iter().find(|e| {
        let lower = e.name.to_lowercase();
        lower.contains(&version_key) && lower.contains("_support_") && !lower.contains("steady")
    }) {
        return Some(entry);
    }

    // 3. 任何包含 halcon{VER} 的文件
    if let Some(entry) = dat_files
        .iter()
        .find(|e| e.name.to_lowercase().contains(&version_key))
    {
        return Some(entry);
    }

    // 4. 降级匹配
    let fallback = collect_modern_versions(dat_files);
    for fv in &fallback {
        if crate::detect::compare_versions(fv, version) != std::cmp::Ordering::Greater {
            if let Some(entry) = dat_files.iter().find(|e| {
                let key = format!("halcon{}", fv);
                let lower = e.name.to_lowercase();
                lower.contains(&key)
                    && lower.contains("_steady_")
                    && !lower.contains("steady-dl")
            }) {
                return Some(entry);
            }
        }
    }

    // 5. 最后尝试 Progress 通用
    dat_files
        .iter()
        .find(|e| {
            let lower = e.name.to_lowercase();
            lower.contains("support_halcon_progress") && !lower.contains("eval")
        })
        .copied()
}

/// 从文件列表中提取可用的现代版本号（降序）
fn collect_modern_versions(dat_files: &[&GitHubEntry]) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut versions = BTreeSet::new();

    for entry in dat_files {
        let name = entry.name.to_lowercase();
        if let Some(pos) = name.find("halcon") {
            let after = &name[pos + 6..];
            let ver: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !ver.is_empty() && ver.contains('.') {
                versions.insert(ver);
            }
        }
    }

    let mut sorted: Vec<String> = versions.into_iter().collect();
    sorted.sort_by(|a, b| crate::detect::compare_versions(b, a));
    sorted
}

/// 获取 license 文件的简短描述
pub fn license_description(name: &str) -> &str {
    let lower = name.to_lowercase();
    if lower.contains("eval_dlt") {
        "Deep Learning Tool 评估许可证"
    } else if lower.contains("steady-dl") {
        "Steady + Deep Learning 许可证"
    } else if lower.contains("_steady_") {
        "Steady 许可证"
    } else if lower.contains("eval_halcon_progress") {
        "Halcon Progress 评估许可证"
    } else if lower.contains("support_halcon_progress") {
        "Halcon Progress 通用许可证"
    } else if lower.contains("_support_") {
        "Support 许可证"
    } else {
        "许可证"
    }
}

/// 检查是否使用了精确版本匹配
pub fn is_exact_match(entry_name: &str, version: &str) -> bool {
    let version_key = format!("halcon{}", version);
    entry_name.to_lowercase().contains(&version_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str) -> GitHubEntry {
        GitHubEntry {
            name: name.to_string(),
            entry_type: "file".to_string(),
            download_url: Some(format!("https://example.com/{}", name)),
        }
    }

    fn make_product(version: &str, kind: ProductKind) -> MvtecProduct {
        MvtecProduct {
            root: std::path::PathBuf::from(format!(r"C:\MVTec\HALCON-{}", version)),
            version: version.to_string(),
            kind,
        }
    }

    fn sample_entries() -> Vec<GitHubEntry> {
        vec![
            make_entry("license_eval_dlt_2026_07.dat"),
            make_entry("license_eval_halcon_progress_2026_07.dat"),
            make_entry("license_support_halcon24.11_steady-dl_2026_07.dat"),
            make_entry("license_support_halcon24.11_steady_2026_07.dat"),
            make_entry("license_support_halcon22.11_steady_2026_07.dat"),
            make_entry("license_support_halcon_progress_2026_07.dat"),
        ]
    }

    #[test]
    fn test_dlt_license() {
        let entries = sample_entries();
        let product = make_product("0.0", ProductKind::DeepLearningTool);
        let result = pick_license_for_product(&entries, &product);
        assert!(result.is_some());
        assert!(result.unwrap().name.contains("eval_dlt"));
    }

    #[test]
    fn test_progress_license_for_unknown_version() {
        let entries = sample_entries();
        let product = make_product("26.05", ProductKind::HalconProgress);
        let result = pick_license_for_product(&entries, &product);
        assert!(result.is_some());
        assert!(result.unwrap().name.contains("_progress_"));
    }

    #[test]
    fn test_steady_exact_match() {
        let entries = sample_entries();
        let product = make_product("24.11", ProductKind::HalconSteady);
        let result = pick_license_for_product(&entries, &product);
        assert!(result.is_some());
        let name = &result.unwrap().name;
        assert!(name.contains("halcon24.11"));
        assert!(name.contains("_steady_"));
        assert!(!name.contains("steady-dl"));
    }

    #[test]
    fn test_steady_dl_prefers_dl() {
        let entries = sample_entries();
        let product = make_product("24.11", ProductKind::HalconSteadyDL);
        let result = pick_license_for_product(&entries, &product);
        assert!(result.is_some());
        assert!(result.unwrap().name.contains("steady-dl"));
    }

    #[test]
    fn test_steady_downgrade() {
        let entries = sample_entries();
        let product = make_product("26.05", ProductKind::HalconSteady);
        let result = pick_license_for_product(&entries, &product);
        assert!(result.is_some());
        // 应该降级到 24.11
        assert!(result.unwrap().name.contains("halcon24.11"));
    }
}
