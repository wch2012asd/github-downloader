//! 下载引擎（阶段 C3）：tokio async 下载核心，语义对齐 v2 downloader.rs。
//!
//! 并发模型：每任务一个编排 task；分段模式由编排 task 经 `JoinSet` 派发 N 个分段 task，
//! 各分段独立写 `.part{i}` 文件（文件长度即进度，崩溃后可恢复）。
//! 暂停/恢复/停止通过 [`TaskControl`]（watch 通道）在 chunk 间隙生效。

pub mod bt;
pub mod downloader;
pub mod limiter;
pub mod manager;
pub mod takeover;

pub use downloader::{
    fetch_size, run, spawn_task, Ctrl, LogLevel, Probe, Progress, TaskControl, TaskEvent, TaskFail,
    TaskSpec,
};
pub use limiter::{Limiter, SpeedLimits};
pub use manager::TaskManager;
