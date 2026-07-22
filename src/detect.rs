use anyhow::Result;
use std::path::{Path, PathBuf};

/// MVTec 产品类型
#[derive(Debug, Clone, PartialEq)]
pub enum ProductKind {
    /// 标准 Halcon（Steady 版本）
    HalconSteady,
    /// Halcon Progress 版本
    HalconProgress,
    /// Halcon Steady + Deep Learning
    HalconSteadyDL,
    /// MVTec Deep Learning Tool
    DeepLearningTool,
    /// Halcon 其他/未知版本
    HalconOther,
    /// 其他 MVTec 产品
    Other,
}

impl ProductKind {
    /// 根据目录名识别产品类型
    pub fn from_dir_name(name: &str) -> Self {
        let lower = name.to_lowercase();

        if lower.contains("deeplearningtool") || lower.contains("deep_learning") {
            return ProductKind::DeepLearningTool;
        }

        if lower.contains("halcon") {
            let has_steady = lower.contains("steady");
            let has_progress = lower.contains("progress");
            let has_dl = lower.contains("-dl") || lower.contains("_dl");

            match (has_steady, has_progress, has_dl) {
                (true, true, _) => ProductKind::HalconSteadyDL,
                (true, false, true) => ProductKind::HalconSteadyDL,
                (true, false, false) => ProductKind::HalconSteady,
                (false, true, _) => ProductKind::HalconProgress,
                (false, false, _) => ProductKind::HalconOther,
            }
        } else {
            ProductKind::Other
        }
    }

    pub fn label(&self) -> &str {
        match self {
            ProductKind::HalconSteady => "Halcon Steady",
            ProductKind::HalconProgress => "Halcon Progress",
            ProductKind::HalconSteadyDL => "Halcon Steady + DL",
            ProductKind::DeepLearningTool => "Deep Learning Tool",
            ProductKind::HalconOther => "Halcon",
            ProductKind::Other => "MVTec Product",
        }
    }
}

/// 检测到的 MVTec 产品安装信息
#[derive(Debug, Clone)]
pub struct MvtecProduct {
    pub root: PathBuf,
    pub version: String,
    pub kind: ProductKind,
}

/// 扫描所有已安装的 MVTec 产品
pub fn find_all_products() -> Result<Vec<MvtecProduct>> {
    let mut products = Vec::new();

    // 1. 检查环境变量 (仅 HALCON)
    if let Ok(root) = std::env::var("HALCONROOT") {
        let root = PathBuf::from(&root);
        if root.exists() {
            if let Some(product) = identify_product(&root) {
                products.push(product);
            }
        }
    }

    // 2. 扫描 Program Files 目录
    for search_dir in &[
        r"C:\Program Files\MVTec",
        r"C:\Program Files (x86)\MVTec",
        r"E:\Program Files\MVTec",
        r"D:\Program Files\MVTec",
    ] {
        let search_path = PathBuf::from(search_dir);
        if !search_path.exists() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(&search_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                // 跳过非产品目录
                if name.eq_ignore_ascii_case("SoftwareManager")
                    || name.eq_ignore_ascii_case("extensions")
                {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() {
                    if let Some(product) = identify_product(&path) {
                        // 避免重复（环境变量可能指向其中一个）
                        if !products.iter().any(|p| p.root == product.root) {
                            products.push(product);
                        }
                    }
                }
            }
        }
    }

    // 按版本降序排列
    products.sort_by(|a, b| compare_versions(&b.version, &a.version));

    if products.is_empty() {
        anyhow::bail!(
            "未找到任何 MVTec 产品安装。\n\
             请确保产品安装在 MVTec 目录下，或设置 HALCONROOT 环境变量"
        );
    }

    Ok(products)
}

/// 识别单个产品目录
fn identify_product(path: &Path) -> Option<MvtecProduct> {
    let dir_name = path.file_name()?.to_string_lossy().to_string();
    let kind = ProductKind::from_dir_name(&dir_name);

    // 只处理可识别的 MVTec 产品
    if kind == ProductKind::Other && !dir_name.to_lowercase().contains("halcon") {
        return None;
    }

    let version = extract_version_from_dir_name(&dir_name).unwrap_or_else(|| "0.0".to_string());

    Some(MvtecProduct {
        root: path.to_path_buf(),
        version,
        kind,
    })
}

/// 从目录名提取版本号
fn extract_version_from_dir_name(name: &str) -> Option<String> {
    let lower = name.to_lowercase();

    if let Some(pos) = lower.find("halcon") {
        let after = &name[pos + 6..]; // skip "halcon"
        let after = after.trim_start_matches('-').trim_start_matches('_');

        let version: String = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();

        if !version.is_empty() {
            return Some(version);
        }
    }

    None
}

/// 从路径提取版本号（公开 API，兼容旧版）
pub fn extract_version_from_path(path: &Path) -> Option<String> {
    let dir_name = path.file_name()?.to_string_lossy();
    extract_version_from_dir_name(&dir_name)
}

/// 比较版本号
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<u32> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let b_parts: Vec<u32> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    a_parts.cmp(&b_parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_product_kind_detection() {
        assert_eq!(
            ProductKind::from_dir_name("HALCON-24.11-Progress-Steady"),
            ProductKind::HalconSteadyDL
        );
        assert_eq!(
            ProductKind::from_dir_name("HALCON-26.05-Progress"),
            ProductKind::HalconProgress
        );
        assert_eq!(
            ProductKind::from_dir_name("HALCON-24.11"),
            ProductKind::HalconOther
        );
        assert_eq!(
            ProductKind::from_dir_name("DeepLearningTool"),
            ProductKind::DeepLearningTool
        );
    }

    #[test]
    fn test_version_extraction() {
        assert_eq!(
            extract_version_from_dir_name("HALCON-24.11-Progress-Steady"),
            Some("24.11".to_string())
        );
        assert_eq!(
            extract_version_from_dir_name("HALCON-26.05-Progress"),
            Some("26.05".to_string())
        );
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(
            compare_versions("24.11", "22.11"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_versions("18.11", "24.11"),
            std::cmp::Ordering::Less
        );
    }
}
