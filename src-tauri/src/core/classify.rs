//! 分类与目录规则：扩展名 → 四类（镜像压缩 / 应用程序 / 影视视频 / 文档）。
//!
//! - 未匹配扩展名返回 `None`（调用方落默认目录）
//! - 总开关 `classify_enabled` 与各类目录（settings 的 `dir_{id}`）由调用方（manager）套用
//! - 大小写不敏感；BT 任务按每个文件的扩展名逐个分类

/// 资源分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    MirrorArchive,
    Application,
    Video,
    Document,
}

/// 一条分类规则。
pub struct CategoryRule {
    /// 存储与目录设置键（`dir_{id}`）
    pub id: &'static str,
    /// 界面显示名（对齐参考图格式筛选）
    pub label: &'static str,
    pub extensions: &'static [&'static str],
}

pub const RULES: [CategoryRule; 4] = [
    CategoryRule {
        id: "archive",
        label: "镜像压缩",
        extensions: &[
            "zip", "7z", "rar", "tar", "gz", "bz2", "xz", "zst", "iso", "img", "cab",
        ],
    },
    CategoryRule {
        id: "app",
        label: "应用程序",
        extensions: &[
            "exe", "msi", "msix", "appx", "apk", "dmg", "deb", "rpm", "appimage",
        ],
    },
    CategoryRule {
        id: "video",
        label: "影视视频",
        extensions: &[
            "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "mpg", "mpeg",
        ],
    },
    CategoryRule {
        id: "doc",
        label: "文档",
        extensions: &[
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "epub", "mobi",
            "chm",
        ],
    },
];

/// 按文件名（取末段扩展名）分类；无扩展名或未匹配返回 `None`。
pub fn classify(filename: &str) -> Option<&'static CategoryRule> {
    let (_, ext) = filename.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    RULES.iter().find(|r| r.extensions.contains(&ext.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_extension_case_insensitive() {
        assert_eq!(classify("setup.exe").map(|r| r.id), Some("app"));
        assert_eq!(classify("NODE-V22.MSI").map(|r| r.id), Some("app"));
        assert_eq!(classify("Unreal_Vehicle.zip").map(|r| r.id), Some("archive"));
        assert_eq!(classify("movie.mkv").map(|r| r.id), Some("video"));
        assert_eq!(classify("guide.PDF").map(|r| r.id), Some("doc"));
    }

    #[test]
    fn no_extension_or_unknown_returns_none() {
        assert_eq!(classify("无扩展名").map(|r| r.id), None);
        assert_eq!(classify("archive.tar.gz").map(|r| r.id), Some("archive")); // gz 命中
        assert_eq!(classify("readme.xyz").map(|r| r.id), None);
        assert_eq!(classify("").map(|r| r.id), None);
    }

    #[test]
    fn labels_match_design_filters() {
        let labels: Vec<&str> = RULES.iter().map(|r| r.label).collect();
        assert_eq!(labels, vec!["镜像压缩", "应用程序", "影视视频", "文档"]);
    }
}
