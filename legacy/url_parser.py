"""GitHub URL 解析器：识别链接类型并提取关键信息。"""

from dataclasses import dataclass
from enum import Enum
from urllib.parse import urlparse


class GitHubLinkType(Enum):
    RAW_FILE = "raw_file"
    RELEASE_ASSET = "release_asset"
    REPO_ARCHIVE = "repo_archive"
    REPO_PAGE = "repo_page"
    UNKNOWN = "unknown"


@dataclass
class ParsedGitHubURL:
    link_type: GitHubLinkType
    owner: str = ""
    repo: str = ""
    branch: str = ""
    tag: str = ""
    file_path: str = ""
    asset_name: str = ""
    download_url: str = ""
    suggested_filename: str = ""


def parse_github_url(raw_url: str) -> ParsedGitHubURL:
    url = raw_url.strip()
    parsed = urlparse(url)
    host = parsed.netloc.lower()
    path_parts = [p for p in parsed.path.split("/") if p]

    # raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}
    if host == "raw.githubusercontent.com" and len(path_parts) >= 3:
        owner = path_parts[0]
        repo = path_parts[1]
        branch = path_parts[2]
        file_path = "/".join(path_parts[3:])
        filename = path_parts[-1] if path_parts else "downloaded_file"
        return ParsedGitHubURL(
            link_type=GitHubLinkType.RAW_FILE,
            owner=owner, repo=repo, branch=branch,
            file_path=file_path,
            download_url=url,
            suggested_filename=filename,
        )

    # github.com/{owner}/{repo}/raw/{branch}/{path}
    if host in ("github.com", "www.github.com") and len(path_parts) >= 4 and path_parts[2] == "raw":
        owner = path_parts[0]
        repo = path_parts[1]
        branch = path_parts[3]
        file_path = "/".join(path_parts[4:])
        filename = path_parts[-1] if path_parts else "downloaded_file"
        raw_url = f"https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{file_path}"
        return ParsedGitHubURL(
            link_type=GitHubLinkType.RAW_FILE,
            owner=owner, repo=repo, branch=branch,
            file_path=file_path,
            download_url=raw_url,
            suggested_filename=filename,
        )

    # github.com/{owner}/{repo}/releases/download/{tag}/{asset}
    if host in ("github.com", "www.github.com") and len(path_parts) >= 5 and path_parts[2] == "releases" and path_parts[3] == "download":
        owner = path_parts[0]
        repo = path_parts[1]
        tag = path_parts[4]
        asset_name = "/".join(path_parts[5:])
        return ParsedGitHubURL(
            link_type=GitHubLinkType.RELEASE_ASSET,
            owner=owner, repo=repo, tag=tag,
            asset_name=asset_name,
            download_url=url,
            suggested_filename=asset_name,
        )

    # github.com/{owner}/{repo}/archive/refs/heads/{branch}.zip
    # github.com/{owner}/{repo}/archive/refs/tags/{tag}.zip
    # github.com/{owner}/{repo}/archive/{ref}.zip  (简写格式)
    if host in ("github.com", "www.github.com") and len(path_parts) >= 4 and path_parts[2] == "archive":
        owner = path_parts[0]
        repo = path_parts[1]
        ref_value_ext = "/".join(path_parts[3:])
        ref_value = ref_value_ext
        ext = ""
        branch = ""
        tag = ""

        if "." in ref_value_ext:
            dot_idx = ref_value_ext.rfind(".")
            ref_value = ref_value_ext[:dot_idx]
            ext = ref_value_ext[dot_idx + 1:]

        if ref_value.startswith("refs/"):
            parts = ref_value.split("/", 2)
            if len(parts) >= 3:
                ref_type = parts[1]
                ref_value = parts[2]
                if ref_type == "heads":
                    branch = ref_value
                elif ref_type == "tags":
                    tag = ref_value
        else:
            branch = ref_value

        filename = f"{repo}-{ref_value}.{ext}" if ext else f"{repo}-{ref_value}"
        return ParsedGitHubURL(
            link_type=GitHubLinkType.REPO_ARCHIVE,
            owner=owner, repo=repo, branch=branch, tag=tag,
            download_url=url,
            suggested_filename=filename,
        )

    # github.com/{owner}/{repo}  -> repo page, offer zip download
    if host in ("github.com", "www.github.com") and len(path_parts) >= 2:
        owner = path_parts[0]
        repo = path_parts[1]
        archive_url = f"https://github.com/{owner}/{repo}/archive/refs/heads/main.zip"
        return ParsedGitHubURL(
            link_type=GitHubLinkType.REPO_PAGE,
            owner=owner, repo=repo,
            download_url=archive_url,
            suggested_filename=f"{repo}-main.zip",
        )

    return ParsedGitHubURL(
        link_type=GitHubLinkType.UNKNOWN,
        download_url=url,
        suggested_filename="downloaded_file",
    )
