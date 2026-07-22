use crate::detect::{MvtecProduct, ProductKind};
use crate::github::GitHubEntry;

/// 为 MVTec 产品收集所有匹配的 license 文件
///
/// 返回需要合并到 license.dat 的文件列表。
/// 策略: 精确版本匹配 + 通用 license（确保 HDevelop 等工具的 license 也被包含）
pub fn pick_licenses_for_product<'a>(
    entries: &'a [GitHubEntry],
    product: &MvtecProduct,
) -> Vec<&'a GitHubEntry> {
    let mut result: Vec<&GitHubEntry> = Vec::new();
    let dat_files: Vec<&GitHubEntry> = entries
        .iter()
        .filter(|e| e.name.ends_with(".dat"))
        .collect();

    match product.kind {
        ProductKind::DeepLearningTool => {
            collect_dlt(&dat_files, &mut result);
            // DLT 也需要 eval_progress (含 HDevelop license)
            collect_progress_generic(&dat_files, &mut result);
        }
        ProductKind::HalconProgress => {
            collect_exact_steady(&dat_files, &product.version, &mut result);
            collect_exact_steady_dl(&dat_files, &product.version, &mut result);
            collect_exact_support(&dat_files, &product.version, &mut result);
            collect_progress_generic(&dat_files, &mut result);
        }
        ProductKind::HalconSteady => {
            collect_exact_steady(&dat_files, &product.version, &mut result);
            collect_exact_steady_dl(&dat_files, &product.version, &mut result);
            collect_progress_generic(&dat_files, &mut result);
        }
        ProductKind::HalconSteadyDL => {
            collect_exact_steady_dl(&dat_files, &product.version, &mut result);
            collect_exact_steady(&dat_files, &product.version, &mut result);
            collect_progress_generic(&dat_files, &mut result);
        }
        ProductKind::HalconOther | ProductKind::Other => {
            collect_exact_steady(&dat_files, &product.version, &mut result);
            collect_exact_support(&dat_files, &product.version, &mut result);
            collect_progress_generic(&dat_files, &mut result);
        }
    }

    // 压缩重复项（按文件名去重）
    dedup(&mut result);
    result
}

/// 收集 `license_eval_dlt_*`
fn collect_dlt<'a>(files: &[&'a GitHubEntry], out: &mut Vec<&'a GitHubEntry>) {
    for e in files {
        if e.name.to_lowercase().contains("eval_dlt") {
            out.push(e);
        }
    }
}

/// 收集版本精确匹配的 steady (不含 -dl): `*halcon{VER}_steady_*`
fn collect_exact_steady<'a>(files: &[&'a GitHubEntry], version: &str, out: &mut Vec<&'a GitHubEntry>) {
    let key = format!("halcon{}", version);
    for e in files {
        let lower = e.name.to_lowercase();
        if lower.contains(&key) && lower.contains("_steady_") && !lower.contains("steady-dl") {
            out.push(e);
        }
    }
}

/// 收集版本精确匹配的 steady-dl: `*halcon{VER}_steady-dl_*`
fn collect_exact_steady_dl<'a>(files: &[&'a GitHubEntry], version: &str, out: &mut Vec<&'a GitHubEntry>) {
    let key = format!("halcon{}", version);
    for e in files {
        let lower = e.name.to_lowercase();
        if lower.contains(&key) && lower.contains("_steady-dl_") {
            out.push(e);
        }
    }
}

/// 收集版本精确匹配的基础 support: `*halcon{VER}_` (不含 steady)
fn collect_exact_support<'a>(files: &[&'a GitHubEntry], version: &str, out: &mut Vec<&'a GitHubEntry>) {
    let key = format!("halcon{}", version);
    for e in files {
        let lower = e.name.to_lowercase();
        if lower.contains(&key) && lower.contains("_support_") && !lower.contains("steady") {
            out.push(e);
        }
    }
}

/// 收集通用 Progress license: `license_support_halcon_progress_*` + `license_eval_halcon_progress_*`
fn collect_progress_generic<'a>(files: &[&'a GitHubEntry], out: &mut Vec<&'a GitHubEntry>) {
    for e in files {
        let lower = e.name.to_lowercase();
        if lower.contains("halcon_progress")
            && (lower.contains("support_halcon_progress") || lower.contains("eval_halcon_progress"))
        {
            out.push(e);
        }
    }
}

/// 按文件名去重
fn dedup(entries: &mut Vec<&GitHubEntry>) {
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| seen.insert(e.name.clone()));
}

/// 获取 license 条目描述（用于用户友好输出）
pub fn describe_bundle(entries: &[&GitHubEntry]) -> Vec<String> {
    entries.iter().map(|e| describe_single(&e.name)).collect()
}

fn describe_single(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("eval_dlt") {
        "Deep Learning Tool".into()
    } else if lower.contains("steady-dl") {
        "Steady + DL".into()
    } else if lower.contains("_steady_") {
        "Steady".into()
    } else if lower.contains("eval_halcon_progress") {
        "Progress 评估 (含 HDevelop)".into()
    } else if lower.contains("support_halcon_progress") {
        "Progress 支持".into()
    } else if lower.contains("_support_") {
        "Support".into()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str) -> GitHubEntry {
        GitHubEntry {
            name: name.to_string(),
            entry_type: "file".to_string(),
            download_url: None,
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
    fn test_progress_gets_both_progress_files() {
        let entries = sample_entries();
        let product = make_product("26.05", ProductKind::HalconProgress);
        let result = pick_licenses_for_product(&entries, &product);
        // 应该包含 support_progress 和 eval_progress
        assert_eq!(result.len(), 2);
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"license_support_halcon_progress_2026_07.dat"));
        assert!(names.contains(&"license_eval_halcon_progress_2026_07.dat"));
    }

    #[test]
    fn test_steady_dl_gets_all_three() {
        let entries = sample_entries();
        let product = make_product("24.11", ProductKind::HalconSteadyDL);
        let result = pick_licenses_for_product(&entries, &product);
        // steady-dl + steady + progress*2
        assert!(result.len() >= 1);
        // 必须有 steady-dl
        assert!(result.iter().any(|e| e.name.contains("steady-dl")));
        // 必须有 progress (含 HDevelop)
        assert!(result.iter().any(|e| e.name.contains("eval_halcon_progress")));
    }

    #[test]
    fn test_dlt_gets_dlt_and_hdevelop() {
        let entries = sample_entries();
        let product = make_product("0.0", ProductKind::DeepLearningTool);
        let result = pick_licenses_for_product(&entries, &product);
        assert!(result.iter().any(|e| e.name.contains("eval_dlt")));
        assert!(result.iter().any(|e| e.name.contains("eval_halcon_progress")));
    }

    #[test]
    fn test_dedup() {
        let entries = vec![
            make_entry("same_file.dat"),
            make_entry("same_file.dat"),
            make_entry("other_file.dat"),
        ];
        let mut result: Vec<&GitHubEntry> = entries.iter().collect();
        dedup(&mut result);
        assert_eq!(result.len(), 2);
    }
}
