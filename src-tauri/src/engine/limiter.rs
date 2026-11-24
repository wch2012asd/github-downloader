//! 下载限速：令牌桶，全局与单任务两层叠加生效（各按自身速率发放，自然取 min）。
//!
//! 设计要点：
//! - `rate_bps = 0` 视为不限速（acquire 立即返回）
//! - 桶初始满（1 秒突发量），对齐 TCP 慢启动观感
//! - 等待以 ≤100ms 分片睡眠，等待期间**不持有锁**，保证暂停/取消在 100ms 级别响应
//! - 与 30s 读超时的互作：读超时只包裹网络 chunk 读取，限速等待发生在写入侧，
//!   结构上不可能误触（专项集成测试见 downloader::tests::it_speed_limit_no_read_timeout）

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 单速率令牌桶。
pub struct Limiter {
    rate_bps: u64,
    /// (剩余令牌, 上次补充时刻)
    state: Mutex<(f64, Instant)>,
}

impl Limiter {
    /// 创建限速器；`rate_bps` 必须 > 0。
    pub fn new(rate_bps: u64) -> Self {
        assert!(rate_bps > 0, "限速速率必须大于 0（不限速请用 None）");
        Self {
            rate_bps,
            state: Mutex::new((rate_bps as f64, Instant::now())),
        }
    }

    /// 获取 n 字节的发送预算。
    ///
    /// 负债模型：一次性扣减 want（允许余额为负——chunk 大于桶容量也能收敛），
    /// 之后等待按速率的持续补充把余额"还清"到 ≥0。等待期间不重新扣减，
    /// 并发任务的扣减互相延长等待 —— 这正是共享限速的公平语义。
    pub async fn acquire(&self, want: usize) {
        let initial_wait = {
            let mut st = self.state.lock().unwrap();
            let now = Instant::now();
            st.0 = (st.0 + now.duration_since(st.1).as_secs_f64() * self.rate_bps as f64)
                .min(self.rate_bps as f64);
            st.1 = now;
            st.0 -= want as f64;
            if st.0 >= 0.0 {
                None
            } else {
                Some(-st.0 / self.rate_bps as f64)
            }
        };
        let Some(total_wait) = initial_wait else {
            return;
        };
        // 分片等待余额还清；防御性上限（1.5×+1s）保证异常情况下放行而非卡死任务
        let deadline = Instant::now() + Duration::from_secs_f64(total_wait * 1.5 + 1.0);
        loop {
            let cleared = {
                let mut st = self.state.lock().unwrap();
                let now = Instant::now();
                st.0 = (st.0 + now.duration_since(st.1).as_secs_f64() * self.rate_bps as f64)
                    .min(self.rate_bps as f64);
                st.1 = now;
                st.0 >= 0.0
            };
            if cleared || Instant::now() > deadline {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

/// 一个任务生效的两层限速（全局 + 任务级）；None = 不限。
#[derive(Clone, Default)]
pub struct SpeedLimits {
    pub global: Option<Arc2<Limiter>>,
    pub task: Option<Arc2<Limiter>>,
}

/// 别名缩写（两层各持 Arc）。
pub type Arc2<T> = std::sync::Arc<T>;

impl SpeedLimits {
    /// 双层是否均不限速。
    pub fn is_unlimited(&self) -> bool {
        self.global.is_none() && self.task.is_none()
    }

    /// 在写入 n 字节前获取预算：两层各自扣减，实际速率即两者 min。
    pub async fn acquire(&self, n: usize) {
        if self.is_unlimited() {
            return;
        }
        if let Some(g) = &self.global {
            g.acquire(n).await;
        }
        if let Some(t) = &self.task {
            t.acquire(n).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn token_bucket_rate_accuracy() {
        // 100KB/s，取 300KB：突发 100KB 免费，剩余 200KB 需 2.0s
        let limiter = Limiter::new(100_000);
        let start = Instant::now();
        limiter.acquire(300_000).await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(1800),
            "限速未生效（仅 {elapsed:?}）"
        );
        assert!(
            elapsed <= Duration::from_millis(4000),
            "限速过慢（{elapsed:?}）"
        );
    }

    #[tokio::test]
    async fn two_layers_take_min() {
        // 全局 80KB/s + 任务 40KB/s：任务层为瓶颈
        // 取 120KB：任务层突发 40KB → 剩余 80KB/40KB/s = 2.0s
        let limits = SpeedLimits {
            global: Some(std::sync::Arc::new(Limiter::new(80_000))),
            task: Some(std::sync::Arc::new(Limiter::new(40_000))),
        };
        let start = Instant::now();
        limits.acquire(120_000).await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(1800),
            "任务层限速未生效（仅 {elapsed:?}）"
        );
    }

    #[tokio::test]
    async fn unlimited_is_instant() {
        let limits = SpeedLimits::default();
        let start = Instant::now();
        limits.acquire(1024 * 1024).await;
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
