//! GitHub URL 解析器：识别链接类型并提取关键信息。
//!
//! 对齐 v1 url_parser.py 的全部规则，并新增 v2 支持：
//! - `/blob/{branch}/{path}` 文件页 → Raw 文件
//! - `/tree/{branch}` 目录页 → 该分支压缩包
//! - `.git` / `git@github.com:` 克隆地址 → 仓库主页
//!
//! 已知限制（与 v1 相同）：分支名含 `/` 时无法与路径区分。

/// 链接类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    RawFile,
    ReleaseAsset,
    RepoArchive,
    RepoPage,
    /// 非 GitHub 的 http(s) 直链（v2.1：任意官网/CDN 直链）
    GeneralUrl,
    Unknown,
}

/// 解析结果；`download_url` 是可直接下载的最终地址。
#[derive(Debug, Clone)]
pub struct ParsedUrl {
    pub link_type: LinkType,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub tag: String,
    pub file_path: String,
    pub asset_name: String,
    pub download_url: String,
    pub suggested_filename: String,
}

impl ParsedUrl {
    /// 类型显示名（对应 v1 TYPE_META）。
    pub fn type_label(&self) -> &'static str {
        match self.link_type {
            LinkType::RawFile => "Raw 原始文件",
            LinkType::ReleaseAsset => "Release 资产包",
            LinkType::RepoArchive => "源码压缩包 (Zip)",
            LinkType::RepoPage => "Git 仓库代码包",
            LinkType::GeneralUrl => "外部直链",
            LinkType::Unknown => "未知类型",
        }
    }
}

impl ParsedUrl {
    #[allow(clippy::too_many_arguments)]
    fn new(
        link_type: LinkType,
        owner: &str,
        repo: &str,
        download_url: String,
        suggested_filename: &str,
    ) -> Self {
        Self {
            link_type,
            owner: owner.into(),
            repo: repo.into(),
            branch: String::new(),
            tag: String::new(),
            file_path: String::new(),
            asset_name: String::new(),
            download_url,
            suggested_filename: percent_decode(suggested_filename),
        }
    }
}

/// 解析 GitHub 链接；无法识别时返回 `LinkType::Unknown`，`download_url` 保留原始输入。
pub fn parse_github_url(raw_url: &str) -> ParsedUrl {
    let url = raw_url.trim();
    let url = normalize_clone_url(url);

    // 手工拆 scheme://host/path?query#fragment（避免引入 url 解析依赖）
    let after_scheme = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url.as_str(),
    };
    let after_scheme = after_scheme.split(['?', '#']).next().unwrap_or(after_scheme);
    let (host, path) = match after_scheme.find('/') {
        Some(i) => (&after_scheme[..i], &after_scheme[i + 1..]),
        None => (after_scheme, ""),
    };
    let host = host.to_ascii_lowercase();
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();

    // raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}
    if host == "raw.githubusercontent.com" && parts.len() >= 4 {
        let filename = percent_decode(parts[parts.len() - 1]);
        let mut p = ParsedUrl::new(
            LinkType::RawFile,
            parts[0],
            parts[1],
            url.to_string(),
            &filename,
        );
        p.branch = parts[2].into();
        p.file_path = parts[3..].join("/");
        return p;
    }

    if host == "github.com" || host == "www.github.com" {
        // github.com/{owner}/{repo}/raw/{branch}/{path}
        if parts.len() >= 5 && parts[2] == "raw" {
            let raw_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                parts[0],
                parts[1],
                parts[3],
                parts[4..].join("/")
            );
            let mut p = ParsedUrl::new(
                LinkType::RawFile,
                parts[0],
                parts[1],
                raw_url,
                parts[parts.len() - 1],
            );
            p.branch = parts[3].into();
            p.file_path = parts[4..].join("/");
            return p;
        }

        // github.com/{owner}/{repo}/releases/download/{tag}/{asset}
        if parts.len() >= 6 && parts[2] == "releases" && parts[3] == "download" {
            let asset = parts[5..].join("/");
            let mut p = ParsedUrl::new(
                LinkType::ReleaseAsset,
                parts[0],
                parts[1],
                url.to_string(),
                &asset,
            );
            p.tag = parts[4].into();
            p.asset_name = asset;
            return p;
        }

        // github.com/{owner}/{repo}/archive/{ref}.zip|tar.gz
        // ref 可为 refs/heads/{branch}、refs/tags/{tag} 或简写 {ref}
        if parts.len() >= 4 && parts[2] == "archive" {
            let ref_ext = parts[3..].join("/");
            // 已知后缀优先匹配，避免把 refs/tags/v2.0.tar.gz 误切成 ref="v2.0.tar"
            let (ref_value, ext) = if let Some(r) = ref_ext.strip_suffix(".tar.gz") {
                (r, "tar.gz")
            } else if let Some(r) = ref_ext.strip_suffix(".zip") {
                (r, "zip")
            } else {
                (ref_ext.as_str(), "")
            };
            let (branch, tag) = split_ref(ref_value);
            // 建议文件名使用拆分后的短 ref（对齐 v1：refs/heads/main → main）
            let display_ref = if !branch.is_empty() {
                &branch
            } else if !tag.is_empty() {
                &tag
            } else {
                ref_value
            };
            let filename = if ext.is_empty() {
                format!("{}-{display_ref}", parts[1])
            } else {
                format!("{}-{display_ref}.{ext}", parts[1])
            };
            let mut p = ParsedUrl::new(
                LinkType::RepoArchive,
                parts[0],
                parts[1],
                url.to_string(),
                &filename,
            );
            p.branch = branch;
            p.tag = tag;
            return p;
        }

        // github.com/{owner}/{repo}/blob/{branch}/{path} → Raw 文件（v2 新增）
        if parts.len() >= 5 && parts[2] == "blob" {
            let raw_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                parts[0],
                parts[1],
                parts[3],
                parts[4..].join("/")
            );
            let mut p = ParsedUrl::new(
                LinkType::RawFile,
                parts[0],
                parts[1],
                raw_url,
                parts[parts.len() - 1],
            );
            p.branch = parts[3].into();
            p.file_path = parts[4..].join("/");
            return p;
        }

        // github.com/{owner}/{repo}/tree/{branch} → 该分支压缩包（v2 新增）
        if parts.len() == 4 && parts[2] == "tree" {
            let branch = parts[3];
            let archive = format!(
                "https://github.com/{}/{}/archive/refs/heads/{}.zip",
                parts[0],
                parts[1],
                branch
            );
            let mut p = ParsedUrl::new(
                LinkType::RepoArchive,
                parts[0],
                parts[1],
                archive,
                &format!("{}-{branch}.zip", parts[1]),
            );
            p.branch = branch.into();
            return p;
        }

        // github.com/{owner}/{repo} → 仓库主页，提供 main 分支 zip
        if parts.len() >= 2 {
            let archive_url = format!(
                "https://github.com/{}/{}/archive/refs/heads/main.zip",
                parts[0],
                parts[1]
            );
            let mut p = ParsedUrl::new(
                LinkType::RepoPage,
                parts[0],
                parts[1],
                archive_url,
                &format!("{}-main.zip", parts[1]),
            );
            p.branch = "main".into();
            return p;
        }
    }

    // 通用直链（v2.1）：任何 http(s) 网址均可直接下载，文件名取路径末段
    if (url.starts_with("http://") || url.starts_with("https://") || host.contains('.'))
        && !host.is_empty()
    {
        // 路径以 / 结尾时没有文件名（末段是目录），交由 Content-Disposition 兜底
        let filename = if url.ends_with('/') {
            String::new()
        } else {
            parts.last().map(|seg| percent_decode(seg)).unwrap_or_default()
        };
        let download = if url.contains("://") {
            url.to_string()
        } else {
            format!("https://{url}")
        };
        let p = ParsedUrl::new(LinkType::GeneralUrl, "", "", download, &filename);
        return p;
    }

    ParsedUrl::new(LinkType::Unknown, "", "", url.to_string(), "downloaded_file")
}

/// 克隆地址归一化：`git@github.com:owner/repo(.git)` 与 `https://github.com/owner/repo.git`
/// 都转成仓库主页 URL。
fn normalize_clone_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let repo = rest.trim_end_matches('/').strip_suffix(".git").unwrap_or(rest);
        let repo = repo.trim_end_matches('/');
        return format!("https://github.com/{repo}");
    }
    if url.starts_with("https://github.com/") && url.ends_with(".git") {
        return url.trim_end_matches(".git").to_string();
    }
    url.to_string()
}

/// 把 `refs/heads/{branch}` / `refs/tags/{tag}` / `{ref}` 拆成 (branch, tag)。
fn split_ref(ref_value: &str) -> (String, String) {
    if let Some(rest) = ref_value.strip_prefix("refs/") {
        let mut it = rest.splitn(2, '/');
        let kind = it.next().unwrap_or("");
        let value = it.next().unwrap_or("");
        match kind {
            "heads" => return (value.into(), String::new()),
            "tags" => return (String::new(), value.into()),
            _ => {}
        }
    }
    (ref_value.into(), String::new())
}

/// 百分号解码（`+` 不按空格处理，路径语境下是字面量）；解码失败时原样返回。
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> ParsedUrl {
        parse_github_url(s)
    }

    #[test]
    fn raw_file_direct() {
        let p = parse("https://raw.githubusercontent.com/owner/repo/main/src/app.py");
        assert_eq!(p.link_type, LinkType::RawFile);
        assert_eq!(p.owner, "owner");
        assert_eq!(p.repo, "repo");
        assert_eq!(p.branch, "main");
        assert_eq!(p.file_path, "src/app.py");
        assert_eq!(p.suggested_filename, "app.py");
        assert_eq!(p.download_url, "https://raw.githubusercontent.com/owner/repo/main/src/app.py");
    }

    #[test]
    fn raw_file_via_github_raw_path() {
        let p = parse("https://github.com/owner/repo/raw/main/a/b.txt");
        assert_eq!(p.link_type, LinkType::RawFile);
        assert_eq!(p.download_url, "https://raw.githubusercontent.com/owner/repo/main/a/b.txt");
        assert_eq!(p.branch, "main");
        assert_eq!(p.file_path, "a/b.txt");
    }

    #[test]
    fn blob_page_converts_to_raw() {
        let p = parse("https://github.com/owner/repo/blob/dev/readme.md");
        assert_eq!(p.link_type, LinkType::RawFile);
        assert_eq!(p.download_url, "https://raw.githubusercontent.com/owner/repo/dev/readme.md");
        assert_eq!(p.branch, "dev");
    }

    #[test]
    fn release_asset() {
        let p = parse("https://github.com/owner/repo/releases/download/v1.2.3/tool-setup.exe");
        assert_eq!(p.link_type, LinkType::ReleaseAsset);
        assert_eq!(p.tag, "v1.2.3");
        assert_eq!(p.asset_name, "tool-setup.exe");
        assert_eq!(p.suggested_filename, "tool-setup.exe");
    }

    #[test]
    fn archive_refs_heads() {
        let p = parse("https://github.com/owner/repo/archive/refs/heads/main.zip");
        assert_eq!(p.link_type, LinkType::RepoArchive);
        assert_eq!(p.branch, "main");
        assert_eq!(p.suggested_filename, "repo-main.zip");
    }

    #[test]
    fn archive_refs_tags() {
        let p = parse("https://github.com/owner/repo/archive/refs/tags/v2.0.tar.gz");
        assert_eq!(p.link_type, LinkType::RepoArchive);
        assert_eq!(p.tag, "v2.0");
        assert_eq!(p.suggested_filename, "repo-v2.0.tar.gz");
    }

    #[test]
    fn archive_short_form() {
        let p = parse("https://github.com/owner/repo/archive/main.zip");
        assert_eq!(p.link_type, LinkType::RepoArchive);
        assert_eq!(p.branch, "main");
        assert_eq!(p.suggested_filename, "repo-main.zip");
    }

    #[test]
    fn tree_page_converts_to_archive() {
        let p = parse("https://github.com/owner/repo/tree/dev");
        assert_eq!(p.link_type, LinkType::RepoArchive);
        assert_eq!(p.branch, "dev");
        assert_eq!(
            p.download_url,
            "https://github.com/owner/repo/archive/refs/heads/dev.zip"
        );
        assert_eq!(p.suggested_filename, "repo-dev.zip");
    }

    #[test]
    fn repo_page_offers_main_zip() {
        let p = parse("https://github.com/microsoft/vscode");
        assert_eq!(p.link_type, LinkType::RepoPage);
        assert_eq!(
            p.download_url,
            "https://github.com/microsoft/vscode/archive/refs/heads/main.zip"
        );
        assert_eq!(p.suggested_filename, "vscode-main.zip");
    }

    #[test]
    fn clone_url_https_git_suffix() {
        let p = parse("https://github.com/owner/repo.git");
        assert_eq!(p.link_type, LinkType::RepoPage);
        assert_eq!(p.repo, "repo");
    }

    #[test]
    fn clone_url_ssh_form() {
        let p = parse("git@github.com:owner/repo.git");
        assert_eq!(p.link_type, LinkType::RepoPage);
        assert_eq!(p.owner, "owner");
        assert_eq!(p.repo, "repo");
    }

    #[test]
    fn strips_query_and_fragment() {
        let p = parse("https://github.com/owner/repo/releases/download/v1/a.zip?foo=bar#L1");
        assert_eq!(p.link_type, LinkType::ReleaseAsset);
        assert_eq!(p.asset_name, "a.zip");
    }

    #[test]
    fn percent_decoded_filename() {
        let p = parse("https://raw.githubusercontent.com/o/r/main/%E4%B8%AD%E6%96%87.txt");
        assert_eq!(p.suggested_filename, "中文.txt");
    }

    #[test]
    fn general_direct_url() {
        let p = parse("https://www.segger.com/downloads/jlink/JLink_Windows_V988_x86_64.exe");
        assert_eq!(p.link_type, LinkType::GeneralUrl);
        assert_eq!(p.type_label(), "外部直链");
        assert_eq!(p.suggested_filename, "JLink_Windows_V988_x86_64.exe");
        assert_eq!(
            p.download_url,
            "https://www.segger.com/downloads/jlink/JLink_Windows_V988_x86_64.exe"
        );
    }

    #[test]
    fn general_url_without_scheme_gets_https() {
        let p = parse("www.segger.com/downloads/jlink/setup.exe");
        assert_eq!(p.link_type, LinkType::GeneralUrl);
        assert_eq!(
            p.download_url,
            "https://www.segger.com/downloads/jlink/setup.exe"
        );
        assert_eq!(p.suggested_filename, "setup.exe");
    }

    #[test]
    fn general_url_without_filename_segment() {
        let p = parse("https://example.com/downloads/");
        assert_eq!(p.link_type, LinkType::GeneralUrl);
        assert_eq!(p.suggested_filename, "");
    }

    #[test]
    fn general_url_query_stripped_from_filename() {
        let p = parse("https://cdn.example.com/pkg/app-1.0.zip?token=abc123");
        assert_eq!(p.link_type, LinkType::GeneralUrl);
        assert_eq!(p.suggested_filename, "app-1.0.zip");
    }

    #[test]
    fn garbage_without_host_stays_unknown() {
        let p = parse("不是链接的文字");
        assert_eq!(p.link_type, LinkType::Unknown);
        assert_eq!(p.type_label(), "未知类型");
    }

    #[test]
    fn percent_decode_edge_cases() {
        assert_eq!(percent_decode("a+b"), "a+b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
