//! 任务持久层：SQLite（sqlx，运行时查询）。
//!
//! - `tasks`：任务主表（HTTP/BT 统一模型）
//! - `task_sources`：HTTP 多源镜像（每源独立 ETag/大小，供续传校验）
//! - `settings`：键值设置（v2 config.json 迁移后的落地处）
//!
//! 恢复点说明：分片断点的"真相"在 part 文件长度（v2 已验证的设计），
//! 本层只存任务元数据，崩溃丢失的最多是最后一次 500ms 进度节流，可接受。

use std::path::Path;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow,
};
use sqlx::{Row, SqlitePool};

use crate::core::config::Config;

/// 任务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TaskKind {
    Http,
    Bt,
}

impl TaskKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Http => "http",
            TaskKind::Bt => "bt",
        }
    }
    fn from_str(s: &str) -> Self {
        match s {
            "bt" => TaskKind::Bt,
            _ => TaskKind::Http,
        }
    }
}

/// 任务状态（队列语义见 engine 模块）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TaskStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Downloading => "downloading",
            TaskStatus::Paused => "paused",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }
    fn from_str(s: &str) -> Self {
        match s {
            "downloading" => TaskStatus::Downloading,
            "paused" => TaskStatus::Paused,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
            "cancelled" => TaskStatus::Cancelled,
            _ => TaskStatus::Queued,
        }
    }
}

/// 新任务描述（id 由本层生成）。
#[derive(Debug, Clone)]
pub struct NewTask {
    pub kind: TaskKind,
    pub name: String,
    pub save_dir: String,
    pub category: String,
    pub urls: Vec<String>,
    /// HTTP 分片数；BT 任务填 None（pieces 数由 metadata 决定）
    pub segments: Option<u32>,
    /// 任务级限速（B/s）；None = 跟随全局
    pub limit_bps: Option<u64>,
    /// 是否经 Clash 本地代理下载（仅 HTTP 任务有效）
    pub use_clash: bool,
}

/// 任务记录（行映射见 `Store::row_to_task`：kind/status 为自定义枚举，不走 FromRow 派生）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskRecord {
    pub id: String,
    pub kind: TaskKind,
    pub name: String,
    pub save_dir: String,
    pub category: String,
    pub status: TaskStatus,
    pub total_bytes: Option<i64>,
    pub received_bytes: i64,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    pub segments: Option<u32>,
    /// 任务级限速（B/s）；NULL = 不限
    pub limit_bps: Option<i64>,
    /// 是否经 Clash 本地代理下载
    pub use_clash: bool,
    /// 首源 URL（list_tasks 子查询附带；不落库）
    #[serde(default)]
    pub url: String,
}

/// 任务源（HTTP 多源镜像 / BT 无此表数据）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskSource {
    pub task_id: String,
    pub url: String,
    pub etag: Option<String>,
    pub total_bytes: Option<i64>,
}

/// SQLite 连接池封装。
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 解析 v2 历史时间 "YYYY-MM-DD HH:MM" 为毫秒（不引 chrono；只用于列表排序，解析失败回退当前时间）。
fn parse_v2_time_to_millis(s: &str) -> Option<i64> {
    let (date, time) = s.split_once(' ')?;
    let (y, rest) = date.split_once('-')?;
    let (m, d) = rest.split_once('-')?;
    let (y, m, d) = (y.parse::<i64>().ok()?, m.parse::<i64>().ok()?, d.parse::<i64>().ok()?);
    let (hh, mm) = time.split_once(':')?;
    let (hh, mm) = (hh.parse::<i64>().ok()?, mm.parse::<i64>().ok()?);
    // 平执近似：把日期折算成天数（不处理闰秒；相对排序足够）
    let days = days_from_civil(y, m, d);
    Some((days * 86_400 + hh * 3600 + mm * 60) * 1000)
}

/// Howard Hinnant 的 days_from_civil 算法（公历日期 → 自 1970-01-01 的天数）。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl Store {
    /// 打开（不存在则创建）数据库文件；启用 WAL 与外键。
    pub async fn open(path: &Path) -> sqlx::Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
        let store = Self { pool };
        store.init_schema().await?;
        Ok(store)
    }

    /// 内存库（测试用）；单连接保证共享同一库。
    pub async fn open_memory() -> sqlx::Result<Self> {
        let opts = SqliteConnectOptions::new()
            .in_memory(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
        let store = Self { pool };
        store.init_schema().await?;
        Ok(store)
    }

    async fn init_schema(&self) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tasks (
                id             TEXT PRIMARY KEY,
                kind           TEXT NOT NULL,
                name           TEXT NOT NULL,
                save_dir       TEXT NOT NULL,
                category       TEXT NOT NULL DEFAULT '',
                status         TEXT NOT NULL,
                total_bytes    INTEGER,
                received_bytes INTEGER NOT NULL DEFAULT 0,
                created_at     INTEGER NOT NULL,
                finished_at    INTEGER,
                error          TEXT,
                segments       INTEGER,
                limit_bps      INTEGER,
                use_clash      INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS task_sources (
                task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                url         TEXT NOT NULL,
                etag        TEXT,
                total_bytes INTEGER,
                PRIMARY KEY (task_id, url)
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_created ON tasks(created_at DESC);
            "#,
        )
        .execute(&self.pool)
        .await?;
        // 旧库迁移：v3.0-dev 早期建的表缺 use_clash 列，补列（已存在则忽略报错）
        let _ = sqlx::query("ALTER TABLE tasks ADD COLUMN use_clash INTEGER NOT NULL DEFAULT 0")
            .execute(&self.pool)
            .await;
        Ok(())
    }

    /// 新增任务（含源列表），返回生成的任务 id。新任务一律 Queued 入队。
    pub async fn insert_task(&self, t: &NewTask) -> sqlx::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_millis();
        let segments = t.segments.map(|s| s as i64);
        sqlx::query(
            r#"INSERT INTO tasks (id, kind, name, save_dir, category, status, created_at, segments, limit_bps, use_clash)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
        )
        .bind(&id)
        .bind(t.kind.as_str())
        .bind(&t.name)
        .bind(&t.save_dir)
        .bind(&t.category)
        .bind(TaskStatus::Queued.as_str())
        .bind(now)
        .bind(segments)
        .bind(t.limit_bps.map(|v| v as i64))
        .bind(t.use_clash)
        .execute(&self.pool)
        .await?;
        for url in &t.urls {
            self.upsert_source(&id, url, None, None).await?;
        }
        Ok(id)
    }

    fn row_to_task(row: &SqliteRow) -> TaskRecord {
        let kind: String = row.get("kind");
        let status: String = row.get("status");
        TaskRecord {
            id: row.get("id"),
            kind: TaskKind::from_str(&kind),
            name: row.get("name"),
            save_dir: row.get("save_dir"),
            category: row.get("category"),
            status: TaskStatus::from_str(&status),
            total_bytes: row.get("total_bytes"),
            received_bytes: row.get::<i64, _>("received_bytes"),
            created_at: row.get("created_at"),
            finished_at: row.get("finished_at"),
            error: row.get("error"),
            segments: row.get::<Option<i64>, _>("segments").map(|s| s as u32),
            limit_bps: row.get("limit_bps"),
            use_clash: row.get::<i64, _>("use_clash") != 0,
            url: row.try_get("primary_url").unwrap_or_default(),
        }
    }

    /// 查询单个任务；不存在返回 None。
    pub async fn get_task(&self, id: &str) -> sqlx::Result<Option<TaskRecord>> {
        let row = sqlx::query("SELECT * FROM tasks WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| Self::row_to_task(&r)))
    }

    /// 全部任务，创建时间倒序（新任务在前）；附带首源 URL（行展示/复制链接用）。
    pub async fn list_tasks(&self) -> sqlx::Result<Vec<TaskRecord>> {
        let rows = sqlx::query(
            r#"SELECT tasks.*,
                      (SELECT url FROM task_sources WHERE task_id = tasks.id LIMIT 1) AS primary_url
               FROM tasks ORDER BY created_at DESC, id DESC"#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(Self::row_to_task).collect())
    }

    /// 更新状态；终态（completed/failed/cancelled）记录 finished_at。
    pub async fn update_status(
        &self,
        id: &str,
        status: TaskStatus,
        error: Option<&str>,
    ) -> sqlx::Result<()> {
        let finished = matches!(
            status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        );
        sqlx::query(
            r#"UPDATE tasks
               SET status = ?2,
                   error = ?3,
                   finished_at = CASE WHEN ?4 THEN ?5 ELSE finished_at END
               WHERE id = ?1"#,
        )
        .bind(id)
        .bind(status.as_str())
        .bind(error)
        .bind(finished)
        .bind(now_millis())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 更新进度（500ms 节流由调用方负责）。
    pub async fn update_progress(
        &self,
        id: &str,
        received: i64,
        total: Option<i64>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"UPDATE tasks
               SET received_bytes = ?2,
                   total_bytes = COALESCE(?3, total_bytes)
               WHERE id = ?1"#,
        )
        .bind(id)
        .bind(received)
        .bind(total)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 更新分片数（探测后确定时回填）。
    pub async fn update_segments(&self, id: &str, segments: u32) -> sqlx::Result<()> {
        sqlx::query("UPDATE tasks SET segments = ?2 WHERE id = ?1")
            .bind(id)
            .bind(segments as i64)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 写入/更新源（幂等）；续传时回填 ETag 与大小。
    pub async fn upsert_source(
        &self,
        task_id: &str,
        url: &str,
        etag: Option<&str>,
        total: Option<i64>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"INSERT INTO task_sources (task_id, url, etag, total_bytes)
               VALUES (?1, ?2, ?3, ?4)
               ON CONFLICT(task_id, url) DO UPDATE
               SET etag = COALESCE(?3, etag),
                   total_bytes = COALESCE(?4, total_bytes)"#,
        )
        .bind(task_id)
        .bind(url)
        .bind(etag)
        .bind(total)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 任务源列表。
    pub async fn sources_of(&self, task_id: &str) -> sqlx::Result<Vec<TaskSource>> {
        sqlx::query_as::<_, TaskSource>(
            "SELECT task_id, url, etag, total_bytes FROM task_sources WHERE task_id = ?1",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
    }

    /// 删除任务（源表级联）。
    pub async fn delete_task(&self, id: &str) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM tasks WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 设置键值（覆盖写）。
    pub async fn set_setting(&self, key: &str, value: &str) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 读取设置。
    pub async fn get_setting(&self, key: &str) -> sqlx::Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    /// 重新下载：进度归零、清除错误与完成时间，状态回到排队（保留任务 id 与创建时间）。
    pub async fn reset_task_progress(&self, id: &str) -> sqlx::Result<()> {
        sqlx::query(
            r#"UPDATE tasks
               SET received_bytes = 0, total_bytes = NULL, status = 'queued',
                   error = NULL, finished_at = NULL
               WHERE id = ?1"#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 更新任务可编辑字段（编辑功能：名称/目录/线程数/Clash 通道）。
    pub async fn update_task_fields(
        &self,
        id: &str,
        name: &str,
        save_dir: &str,
        segments: Option<i64>,
        use_clash: bool,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"UPDATE tasks
               SET name = ?2, save_dir = ?3, segments = COALESCE(?4, segments), use_clash = ?5
               WHERE id = ?1"#,
        )
        .bind(id)
        .bind(name)
        .bind(save_dir)
        .bind(segments)
        .bind(use_clash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 崩溃恢复：应用启动时把遗留的 downloading 状态如实改为 paused
    ///（上次运行中断的任务，引擎未在跑，等用户手动继续）。
    pub async fn reset_stale_downloading(&self) -> sqlx::Result<()> {
        sqlx::query("UPDATE tasks SET status = 'paused' WHERE status = 'downloading'")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 全部设置（键值对）。
    pub async fn list_settings(&self) -> sqlx::Result<Vec<(String, String)>> {
        let rows = sqlx::query("SELECT key, value FROM settings")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get::<String, _>("key"), r.get::<String, _>("value")))
            .collect())
    }
}

/// 读取 v2 格式的 config.json（`crate::core::config::load_from` 的语义别名，供 IPC 调用）。
pub fn import_read_config(path: &Path) -> Result<crate::core::config::Config, String> {
    Ok(crate::core::config::load_from(path))
}

/// 一次性迁移 v2 `config.json`：偏好项入 settings，成功历史入 tasks（completed）。
/// 幂等：已迁移（settings 有 `v2_migrated`）时直接返回 0。
/// 返回导入的任务条数。
pub async fn migrate_v2_config(store: &Store, cfg: &Config) -> sqlx::Result<usize> {
    if store.get_setting("v2_migrated").await?.is_some() {
        return Ok(0);
    }
    // 偏好项（值统一转字符串；前端按需解析）
    let prefs: [(&str, String); 11] = [
        ("proxy_port", cfg.proxy_port.to_string()),
        ("save_dir", cfg.save_dir.clone()),
        ("engine", cfg.engine.clone()),
        ("auto_open", cfg.auto_open.to_string()),
        ("ghproxy_prefix", cfg.ghproxy_prefix.clone()),
        ("kkgithub_host", cfg.kkgithub_host.clone()),
        ("segments", cfg.segments.to_string()),
        ("theme_dark", cfg.theme_dark.to_string()),
        ("auto_clipboard", cfg.auto_clipboard.to_string()),
        ("sound_enabled", cfg.sound_enabled.to_string()),
        ("timeout_seconds", cfg.timeout_seconds.to_string()),
    ];
    for (k, v) in &prefs {
        store.set_setting(k, v).await?;
    }
    // 历史 → 已完成任务（失败的记录不导入：与"任务列表即历史"的语义一致，只留成功下载）
    let mut imported = 0usize;
    for h in &cfg.history {
        if !h.ok || h.name.is_empty() {
            continue;
        }
        let save_dir = std::path::Path::new(&h.save_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| cfg.save_dir.clone());
        let id = store
            .insert_task(&NewTask {
                kind: TaskKind::Http,
                name: h.name.clone(),
                save_dir,
                category: String::new(),
                urls: if h.url.is_empty() { Vec::new() } else { vec![h.url.clone()] },
                segments: Some(cfg.segments as u32),
                limit_bps: None,
                use_clash: false,
            })
            .await?;
        // 用 v2 记录的完成时间回填 created_at（列表按时间倒序）
        let created = parse_v2_time_to_millis(&h.time).unwrap_or_else(now_millis);
        sqlx::query("UPDATE tasks SET created_at = ?2, received_bytes = ?3, total_bytes = ?3, finished_at = ?2 WHERE id = ?1")
            .bind(&id)
            .bind(created)
            .bind(h.size as i64)
            .execute(&store.pool)
            .await?;
        store.update_status(&id, TaskStatus::Completed, None).await?;
        imported += 1;
    }
    store.set_setting("v2_migrated", "1").await?;
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(name: &str) -> NewTask {
        NewTask {
            kind: TaskKind::Http,
            name: name.into(),
            save_dir: "D:/Downloads".into(),
            category: "application".into(),
            urls: vec!["https://example.com/a.zip".into()],
            segments: Some(8),
            limit_bps: None,
            use_clash: false,
        }
    }

    #[tokio::test]
    async fn list_tasks_includes_primary_url() {
        let store = Store::open_memory().await.unwrap();
        let id = store.insert_task(&sample_task("a.zip")).await.unwrap();
        let list = store.list_tasks().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].url, "https://example.com/a.zip");
        assert!(!list[0].use_clash);
        let _ = id;
    }

    #[tokio::test]
    async fn task_crud_roundtrip() {
        let store = Store::open_memory().await.unwrap();
        let id = store.insert_task(&sample_task("a.zip")).await.unwrap();
        let t = store.get_task(&id).await.unwrap().unwrap();
        assert_eq!(t.name, "a.zip");
        assert_eq!(t.kind, TaskKind::Http);
        assert_eq!(t.status, TaskStatus::Queued);
        assert_eq!(t.segments, Some(8));

        store.update_progress(&id, 500, Some(1000)).await.unwrap();
        store.update_status(&id, TaskStatus::Downloading, None).await.unwrap();
        let t = store.get_task(&id).await.unwrap().unwrap();
        assert_eq!(t.received_bytes, 500);
        assert_eq!(t.total_bytes, Some(1000));
        assert_eq!(t.status, TaskStatus::Downloading);

        store.update_status(&id, TaskStatus::Completed, None).await.unwrap();
        let t = store.get_task(&id).await.unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
        assert!(t.finished_at.is_some());

        store.delete_task(&id).await.unwrap();
        assert!(store.get_task(&id).await.unwrap().is_none());
        assert!(store.sources_of(&id).await.unwrap().is_empty()); // 级联删除
    }

    #[tokio::test]
    async fn list_tasks_newest_first() {
        let store = Store::open_memory().await.unwrap();
        let a = store.insert_task(&sample_task("a")).await.unwrap();
        // 保证时间戳可区分（毫秒精度下可能同毫秒，主动睡 2ms）
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let b = store.insert_task(&sample_task("b")).await.unwrap();
        let list = store.list_tasks().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b); // 新任务在前
        assert_eq!(list[1].id, a);
    }

    #[tokio::test]
    async fn sources_upsert_idempotent() {
        let store = Store::open_memory().await.unwrap();
        let id = store.insert_task(&sample_task("a.zip")).await.unwrap();
        store.upsert_source(&id, "https://mirror.example/a.zip", None, None).await.unwrap();
        store.upsert_source(&id, "https://example.com/a.zip", Some("\"etag1\""), Some(2048)).await.unwrap();
        // 重复 URL 再写一次：不产生新行，只回填字段
        store.upsert_source(&id, "https://example.com/a.zip", None, Some(4096)).await.unwrap();
        let sources = store.sources_of(&id).await.unwrap();
        assert_eq!(sources.len(), 2);
        let primary = sources.iter().find(|s| s.url == "https://example.com/a.zip").unwrap();
        assert_eq!(primary.etag.as_deref(), Some("\"etag1\""));
        assert_eq!(primary.total_bytes, Some(4096)); // COALESCE 更新为非空新值
    }

    #[tokio::test]
    async fn settings_roundtrip() {
        let store = Store::open_memory().await.unwrap();
        assert_eq!(store.get_setting("nope").await.unwrap(), None);
        store.set_setting("engine", "ghproxy").await.unwrap();
        store.set_setting("engine", "moeyy").await.unwrap(); // 覆盖写
        assert_eq!(store.get_setting("engine").await.unwrap().as_deref(), Some("moeyy"));
    }

    fn v2_config_fixture() -> Config {
        let mut cfg = Config::default();
        cfg.proxy_port = 7897;
        cfg.save_dir = "D:/Downloads".into();
        cfg.engine = "ghproxy".into();
        cfg.segments = 16;
        cfg.history = vec![
            crate::core::config::HistoryEntry {
                time: "2026-10-13 12:00".into(),
                name: "vscode.zip".into(),
                size: 9_000_000,
                url: "https://github.com/m/vscode/zip".into(),
                ok: true,
                save_path: "D:/Downloads/vscode.zip".into(),
            },
            crate::core::config::HistoryEntry {
                time: "2026-10-14 09:30".into(),
                name: "failed.iso".into(),
                size: 0,
                url: "https://x/iso".into(),
                ok: false, // 失败记录：不导入
                save_path: String::new(),
            },
        ];
        cfg
    }

    #[tokio::test]
    async fn migrate_v2_config_imports_once() {
        let store = Store::open_memory().await.unwrap();
        let n = migrate_v2_config(&store, &v2_config_fixture()).await.unwrap();
        assert_eq!(n, 1); // 只导入成功记录
        assert_eq!(store.get_setting("proxy_port").await.unwrap().as_deref(), Some("7897"));
        assert_eq!(store.get_setting("segments").await.unwrap().as_deref(), Some("16"));

        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "vscode.zip");
        assert_eq!(tasks[0].status, TaskStatus::Completed);
        assert_eq!(tasks[0].total_bytes, Some(9_000_000));
        assert_eq!(tasks[0].save_dir, "D:/Downloads");
        let sources = store.sources_of(&tasks[0].id).await.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].url, "https://github.com/m/vscode/zip");

        // 幂等：二次迁移导入 0 条
        let n2 = migrate_v2_config(&store, &v2_config_fixture()).await.unwrap();
        assert_eq!(n2, 0);
        assert_eq!(store.list_tasks().await.unwrap().len(), 1);
    }
}
