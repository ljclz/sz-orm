//! HLC 混合逻辑时钟（Hybrid Logical Clock）
//!
//! 物理时钟 + 逻辑计数器，时间戳单调递增，跨节点可比。
//! 为 last-write-wins 提供统一时钟源，禁止基于本地时钟。
//!
//! 算法依据：Kulkarni et al. "Logical Physical Clocks" (2014).

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::DistEnhanceError;

/// HLC 时间戳
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HlcTimestamp {
    /// 物理时钟（Unix 毫秒）
    pub physical: i64,
    /// 逻辑计数器
    pub logical: u64,
}

impl HlcTimestamp {
    /// 创建时间戳
    pub fn new(physical: i64, logical: u64) -> Self {
        Self { physical, logical }
    }

    /// 比较时间戳（先比物理，再比逻辑）
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        self.physical
            .cmp(&other.physical)
            .then(self.logical.cmp(&other.logical))
    }
}

impl PartialOrd for HlcTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HlcTimestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.compare(other)
    }
}

/// HLC 配置
#[derive(Debug, Clone)]
pub struct HlcConfig {
    /// 最大时钟偏移（毫秒），超过则报 `ClockSkew`
    pub max_offset_ms: i64,
}

impl Default for HlcConfig {
    fn default() -> Self {
        Self {
            max_offset_ms: 60_000,
        }
    }
}

/// HLC 混合逻辑时钟
///
/// 线程安全：基于 `AtomicI64` + `AtomicU64`，无锁。
pub struct HlcClock {
    config: HlcConfig,
    /// 上次时间戳的物理部分
    last_physical: AtomicI64,
    /// 逻辑计数器
    logical: AtomicU64,
}

impl HlcClock {
    /// 创建 HLC 时钟
    pub fn new(config: HlcConfig) -> Self {
        Self {
            config,
            last_physical: AtomicI64::new(now_ms()),
            logical: AtomicU64::new(0),
        }
    }

    /// 获取当前时间戳（单调递增）
    ///
    /// 算法：取 `max(本地物理, 上次物理)`，若相等则逻辑计数器 +1，否则重置为 0。
    pub fn now(&self) -> HlcTimestamp {
        let curr_physical = now_ms();
        let last_physical = self.last_physical.load(Ordering::Acquire);
        let new_physical = curr_physical.max(last_physical);
        let new_logical = if new_physical == last_physical {
            self.logical.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            self.logical.store(0, Ordering::Release);
            0
        };
        self.last_physical.store(new_physical, Ordering::Release);
        HlcTimestamp {
            physical: new_physical,
            logical: new_logical,
        }
    }

    /// 接收远端事件时间戳并更新本地时钟（跨节点可比）
    ///
    /// 算法：取 `max(本地物理, 上次物理, 远端物理)`，逻辑计数器取 `max(本地, 远端) + 1`。
    /// 时钟偏移超过 `max_offset_ms` 返回 `ClockSkew`。
    pub fn observe(&self, remote: HlcTimestamp) -> Result<HlcTimestamp, DistEnhanceError> {
        let curr_physical = now_ms();
        let offset = curr_physical - remote.physical;
        if offset.abs() > self.config.max_offset_ms {
            return Err(DistEnhanceError::ClockSkew(format!(
                "offset {offset}ms exceeds max {}ms",
                self.config.max_offset_ms
            )));
        }
        let last_physical = self.last_physical.load(Ordering::Acquire);
        let new_physical = curr_physical.max(last_physical).max(remote.physical);
        let new_logical = if new_physical == last_physical || new_physical == remote.physical {
            let local_l = self.logical.load(Ordering::Acquire);
            local_l.max(remote.logical) + 1
        } else {
            0
        };
        self.logical.store(new_logical, Ordering::Release);
        self.last_physical.store(new_physical, Ordering::Release);
        Ok(HlcTimestamp {
            physical: new_physical,
            logical: new_logical,
        })
    }

    /// 配置引用
    pub fn config(&self) -> &HlcConfig {
        &self.config
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_now_monotonic() {
        let clock = HlcClock::new(HlcConfig::default());
        let ts1 = clock.now();
        let ts2 = clock.now();
        assert!(ts2 >= ts1, "ts2 {:?} must >= ts1 {:?}", ts2, ts1);
        if ts1.physical == ts2.physical {
            assert!(ts2.logical > ts1.logical, "logical must increment");
        }
    }

    #[test]
    fn test_cross_node_comparable() {
        let clock_a = HlcClock::new(HlcConfig::default());
        let clock_b = HlcClock::new(HlcConfig::default());
        let ts_a = clock_a.now();
        let ts_b = clock_b.observe(ts_a).unwrap();
        assert!(ts_b >= ts_a, "observe must produce >= remote");
    }

    #[test]
    fn test_concurrent_safe() {
        let clock = Arc::new(HlcClock::new(HlcConfig::default()));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let c = clock.clone();
            handles.push(thread::spawn(move || c.now()));
        }
        let mut stamps: Vec<HlcTimestamp> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();
        stamps.sort();
        for w in stamps.windows(2) {
            assert!(w[1] > w[0], "timestamps must be strictly ordered");
        }
    }

    #[test]
    fn test_clock_skew_detection() {
        let clock = HlcClock::new(HlcConfig { max_offset_ms: 10 });
        let future = HlcTimestamp::new(now_ms() + 100_000, 0);
        assert!(matches!(
            clock.observe(future),
            Err(DistEnhanceError::ClockSkew(_))
        ));
    }

    #[test]
    fn test_timestamp_ordering() {
        let a = HlcTimestamp::new(100, 1);
        let b = HlcTimestamp::new(100, 2);
        let c = HlcTimestamp::new(101, 0);
        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }
}
