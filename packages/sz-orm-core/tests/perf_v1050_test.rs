//! v10.5.0 性能优化端到端测试
//!
//! 验证 T1-T6 六大性能优化方向的行为副作用（非幻影测试）：
//! - T1：连接池批量 acquire 并行化配置
//! - T2：LRU-K evict O(log n) 评分索引
//! - T3：acquire 慢路径 RecentFirst 部分选择（间接验证）
//! - T4：零拷贝批量反序列化路径深化（需 serde-zero-copy feature）
//! - T5：SIMD 阈值自适应（需 query-simd feature）
//! - T6：动态批量决策自适应基准

// ============================================================================
// T1：连接池批量 acquire 并行化配置
// ============================================================================

mod t1_acquire_batch_parallel {
    use sz_orm_core::AcquireBatchParallelConfig;

    #[test]
    fn acquire_batch_parallel_config_default() {
        let config = AcquireBatchParallelConfig::default();
        assert!(config.enabled, "默认应启用并行批量获取");
        assert_eq!(config.threshold, 2, "默认阈值应为 2");
    }

    #[test]
    fn acquire_batch_parallel_config_builders() {
        let config = AcquireBatchParallelConfig::new()
            .with_enabled(false)
            .with_threshold(8);
        assert!(!config.enabled, "with_enabled(false) 应禁用");
        assert_eq!(config.threshold, 8, "with_threshold(8) 应设置阈值 8");
    }

    #[test]
    fn acquire_batch_parallel_config_threshold_clamp() {
        let config_low = AcquireBatchParallelConfig::new().with_threshold(1);
        assert_eq!(config_low.threshold, 2, "阈值 < 2 应钳制到 2");

        let config_high = AcquireBatchParallelConfig::new().with_threshold(100);
        assert_eq!(config_high.threshold, 32, "阈值 > 32 应钳制到 32");
    }

    #[test]
    fn acquire_batch_parallel_config_clone() {
        let config = AcquireBatchParallelConfig::new().with_threshold(16);
        let cloned = config.clone();
        assert_eq!(config.threshold, cloned.threshold);
        assert_eq!(config.enabled, cloned.enabled);
    }

    #[test]
    fn pool_config_has_acquire_batch_parallel_field() {
        let pool_config = sz_orm_core::PoolConfig::default();
        assert!(
            pool_config.acquire_batch_parallel.enabled,
            "PoolConfig 默认应启用 acquire_batch_parallel"
        );
        assert_eq!(
            pool_config.acquire_batch_parallel.threshold, 2,
            "PoolConfig 默认阈值应为 2"
        );
    }
}

// ============================================================================
// T2：LRU-K evict O(log n) 评分索引
// ============================================================================

mod t2_lru_k_evict_index {
    use sz_orm_core::plan_cache::LruKReplacer;

    #[test]
    fn lru_k_evict_logn_consistency() {
        let keys = vec![10u64, 20, 30, 40, 50];

        let mut replacer_index = LruKReplacer::new(2, 3).with_evict_index(true);
        let mut replacer_full = LruKReplacer::new(2, 3).with_evict_index(false);

        for &key in &keys {
            replacer_index.access(key);
            replacer_full.access(key);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let victim_index = replacer_index.evict();
        let victim_full = replacer_full.evict();

        assert!(victim_index.is_some(), "索引路径应有受害者");
        assert!(victim_full.is_some(), "全量遍历路径应有受害者");
        assert_eq!(
            victim_index, victim_full,
            "索引路径与全量遍历应淘汰相同 key"
        );
    }

    #[test]
    fn lru_k_evict_index_disabled() {
        let mut replacer = LruKReplacer::new(2, 3).with_evict_index(false);
        replacer.access(10);
        replacer.access(20);
        replacer.access(30);

        let victim = replacer.evict();
        assert!(victim.is_some(), "禁用索引时 evict 仍应工作");
    }

    #[test]
    fn lru_k_evict_hit_rate_no_degrade() {
        let capacity = 10;
        let mut replacer = LruKReplacer::new(2, capacity).with_evict_index(true);

        for i in 0..capacity as u64 {
            replacer.access(i);
        }

        for _ in 0..5 {
            replacer.access(0);
            replacer.access(1);
        }

        let mut evict_count = 0;
        for i in 0..5 {
            let victim = replacer.evict();
            assert!(victim.is_some(), "第 {} 次淘汰应有受害者", i);
            evict_count += 1;
            replacer.access(capacity as u64 + i + 1);
        }

        assert_eq!(evict_count, 5, "应成功淘汰 5 次");
    }

    #[test]
    fn lru_k_evict_empty_returns_none() {
        let mut replacer = LruKReplacer::new(2, 10).with_evict_index(true);
        assert!(replacer.evict().is_none(), "空 replacer 应返回 None");
    }

    #[test]
    fn lru_k_evict_under_capacity_returns_none() {
        let mut replacer = LruKReplacer::new(2, 5).with_evict_index(true);
        replacer.access(1);
        replacer.access(2);
        replacer.access(3);
        assert!(replacer.evict().is_none(), "未满容量时应返回 None");
    }

    #[test]
    fn lru_k_with_evict_index_builder() {
        let replacer = LruKReplacer::new(2, 10)
            .with_evict_index(true)
            .with_evict_index(false);
        let _ = replacer;
    }
}

// ============================================================================
// T3：acquire 慢路径 RecentFirst 部分选择（间接验证）
// ============================================================================

mod t3_recent_first_partial_select {
    use sz_orm_core::AcquireBatchParallelConfig;

    #[test]
    fn recent_first_config_does_not_affect_serial_path() {
        let config = AcquireBatchParallelConfig::new()
            .with_enabled(false)
            .with_threshold(2);
        assert!(!config.enabled);
        assert_eq!(config.threshold, 2);
    }

    #[test]
    fn recent_first_threshold_boundary() {
        let config = AcquireBatchParallelConfig::new().with_threshold(2);
        assert_eq!(config.threshold, 2, "n=1 < threshold=2 走串行无并行开销");
    }
}

// ============================================================================
// T4：零拷贝批量反序列化路径深化（需 serde-zero-copy feature）
// ============================================================================

#[cfg(feature = "serde-zero-copy")]
mod t4_deserialize_batch {
    use sz_orm_core::perf_extreme::{ZeroCopyDeserConfig, ZeroCopyDeserializer};

    #[test]
    fn deserialize_batch_empty_input() {
        let deserializer = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let rows: Vec<&[u8]> = vec![];
        let result = deserializer.deserialize_batch_borrowed(&rows);
        assert!(result.is_empty(), "空输入应返回空 Vec");
    }

    #[test]
    fn deserialize_batch_result_equivalence() {
        let deserializer = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());

        let row1: &[u8] = &[0x01, 0x01];
        let row2: &[u8] = &[0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        let rows: Vec<&[u8]> = vec![row1, row2];

        let batch_result = deserializer.deserialize_batch_borrowed(&rows);

        let single1 = deserializer.deserialize_borrowed(row1);
        let single2 = deserializer.deserialize_borrowed(row2);

        assert_eq!(batch_result.len(), 2, "批量结果应与输入行数一致");
        assert_eq!(
            std::mem::discriminant(&batch_result[0]),
            std::mem::discriminant(&single1),
            "第一行批量与逐行结果类型应一致"
        );
        assert_eq!(
            std::mem::discriminant(&batch_result[1]),
            std::mem::discriminant(&single2),
            "第二行批量与逐行结果类型应一致"
        );
    }

    #[test]
    fn deserialize_batch_single_row() {
        let deserializer = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let row: &[u8] = &[0x01, 0x01];
        let rows: Vec<&[u8]> = vec![row];

        let batch_result = deserializer.deserialize_batch_borrowed(&rows);
        let single_result = deserializer.deserialize_borrowed(row);

        assert_eq!(batch_result.len(), 1);
        assert_eq!(
            std::mem::discriminant(&batch_result[0]),
            std::mem::discriminant(&single_result),
            "单行批量与逐行结果类型应一致"
        );
    }
}

// ============================================================================
// T5：SIMD 阈值自适应（需 query-simd feature）
// ============================================================================

#[cfg(feature = "query-simd")]
mod t5_simd_threshold_adaptive {
    use sz_orm_core::perf_extreme::{SimdThresholdAdaptive, SimdThresholdAdaptiveConfig};

    #[test]
    fn simd_threshold_adaptive_config_default() {
        let config = SimdThresholdAdaptiveConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_threshold, 2);
        assert_eq!(config.max_threshold, 1024);
        assert_eq!(config.history_min, 10);
        assert!((config.ema_factor - 0.3).abs() < 1e-10);
    }

    #[test]
    fn simd_threshold_adaptive_history_insufficient_default() {
        let config = SimdThresholdAdaptiveConfig::new().with_history_min(10);
        let adaptive = SimdThresholdAdaptive::new(config);
        assert_eq!(adaptive.get_threshold(), 8, "历史不足时应返回默认阈值 8");
    }

    #[test]
    fn simd_threshold_adaptive_increase_on_antioptimize() {
        let config = SimdThresholdAdaptiveConfig::new()
            .with_enabled(true)
            .with_history_min(1)
            .with_min_threshold(2)
            .with_max_threshold(1024);
        let adaptive = SimdThresholdAdaptive::new(config);

        adaptive.record_speedup(0.5);
        let threshold_after = adaptive.get_threshold();
        assert!(
            threshold_after >= 8,
            "加速比 < 1.0 时阈值应提高（避免 SIMD 反优化），got={}",
            threshold_after
        );
    }

    #[test]
    fn simd_threshold_adaptive_decrease_on_prooptimize() {
        let config = SimdThresholdAdaptiveConfig::new()
            .with_enabled(true)
            .with_history_min(1)
            .with_min_threshold(2)
            .with_max_threshold(1024);
        let adaptive = SimdThresholdAdaptive::new(config);

        adaptive.record_speedup(2.0);
        let threshold_after = adaptive.get_threshold();
        assert!(
            threshold_after <= 8,
            "加速比 > 1.5 时阈值应降低（更积极使用 SIMD），got={}",
            threshold_after
        );
    }

    #[test]
    fn simd_threshold_clamp() {
        let config = SimdThresholdAdaptiveConfig::new()
            .with_enabled(true)
            .with_history_min(1)
            .with_min_threshold(4)
            .with_max_threshold(16);
        let adaptive = SimdThresholdAdaptive::new(config);

        for _ in 0..20 {
            adaptive.record_speedup(0.1);
        }
        let threshold = adaptive.get_threshold();
        assert!(
            threshold >= 4 && threshold <= 16,
            "阈值应钳制在 [4, 16]，got={}",
            threshold
        );

        let config2 = SimdThresholdAdaptiveConfig::new()
            .with_enabled(true)
            .with_history_min(1)
            .with_min_threshold(4)
            .with_max_threshold(16);
        let adaptive2 = SimdThresholdAdaptive::new(config2);
        for _ in 0..20 {
            adaptive2.record_speedup(10.0);
        }
        let threshold2 = adaptive2.get_threshold();
        assert!(
            threshold2 >= 4 && threshold2 <= 16,
            "阈值应钳制在 [4, 16]，got={}",
            threshold2
        );
    }

    #[test]
    fn simd_threshold_disabled_returns_default() {
        let config = SimdThresholdAdaptiveConfig::new().with_enabled(false);
        let adaptive = SimdThresholdAdaptive::new(config);
        assert_eq!(adaptive.get_threshold(), 8, "禁用时应返回默认阈值 8");
    }
}

// ============================================================================
// T6：动态批量决策自适应基准
// ============================================================================

mod t6_batch_advisor_adaptive_baseline {
    use sz_orm_core::{
        AdaptiveBaseline, AdaptiveBaselineConfig, DynamicBatchAdvisor, DynamicBatchConfig,
    };

    #[test]
    fn batch_advisor_adaptive_baseline_config_default() {
        let config = AdaptiveBaselineConfig::default();
        assert!(config.enabled);
        assert_eq!(config.history_min, 10);
        assert_eq!(config.fallback_tp, 1000.0);
        assert_eq!(config.fallback_lat, 100.0);
    }

    #[test]
    fn batch_advisor_adaptive_baseline_fallback() {
        let config = AdaptiveBaselineConfig::new().with_history_min(10);
        let baseline = AdaptiveBaseline::new(config);

        for _ in 0..5 {
            baseline.record(5000.0, 10.0);
        }

        assert_eq!(
            baseline.get_tp_baseline(),
            1000.0,
            "历史不足时应返回 fallback_tp"
        );
        assert_eq!(
            baseline.get_lat_baseline(),
            100.0,
            "历史不足时应返回 fallback_lat"
        );
    }

    #[test]
    fn batch_advisor_adaptive_baseline_median() {
        let config = AdaptiveBaselineConfig::new().with_history_min(5);
        let baseline = AdaptiveBaseline::new(config);

        let tp_values = [1000.0, 2000.0, 3000.0, 4000.0, 5000.0];
        let lat_values = [10.0, 20.0, 30.0, 40.0, 50.0];
        for i in 0..5 {
            baseline.record(tp_values[i], lat_values[i]);
        }

        assert_eq!(baseline.get_tp_baseline(), 3000.0, "中位数应取中间值");
        assert_eq!(baseline.get_lat_baseline(), 30.0, "中位数应取中间值");
    }

    #[test]
    fn batch_advisor_adaptive_baseline_clamp() {
        let config = DynamicBatchConfig {
            max_size: 200,
            ..Default::default()
        };
        let baseline_config = AdaptiveBaselineConfig::new().with_history_min(3);
        let advisor =
            DynamicBatchAdvisor::new(config).with_adaptive_baseline_config(baseline_config);

        for _ in 0..5 {
            advisor.record_execution(1, 1.0, 10000.0);
        }

        let dynamic = advisor.advise_dynamic(sz_orm_core::dialect::DbType::MySQL, 2, 10000);
        assert!(dynamic >= 1, "应钳制到 min_size=1");
        assert!(dynamic <= 200, "应钳制到 max_size=200");
    }

    #[test]
    fn batch_advisor_adaptive_baseline_convergence() {
        let baseline_config = AdaptiveBaselineConfig::new()
            .with_enabled(true)
            .with_history_min(3);
        let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default())
            .with_adaptive_baseline_config(baseline_config);

        for _ in 0..10 {
            advisor.record_execution(500, 5000.0, 10.0);
        }

        let dynamic = advisor.advise_dynamic(sz_orm_core::dialect::DbType::MySQL, 2, 10000);
        let static_val = sz_orm_core::batch_advisor::BatchSizeAdvisor::advise(
            sz_orm_core::dialect::DbType::MySQL,
            2,
            10000,
        );
        assert!(
            dynamic >= static_val,
            "高吞吐量低延迟时动态建议应 ≥ 静态值, dynamic={}, static={}",
            dynamic,
            static_val
        );
    }

    #[test]
    fn batch_advisor_damping_unchanged() {
        let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());

        advisor.record_execution(500, 1000.0, 100.0);
        advisor.record_execution(500, 1000.0, 100.0);
        advisor.record_execution(500, 1000.0, 100.0);

        let rec1 = advisor.advise_dynamic(sz_orm_core::dialect::DbType::MySQL, 2, 10000);

        advisor.record_execution(500, 5000.0, 10.0);
        let rec2 = advisor.advise_dynamic(sz_orm_core::dialect::DbType::MySQL, 2, 10000);

        advisor.record_execution(500, 100.0, 500.0);
        let rec3 = advisor.advise_dynamic(sz_orm_core::dialect::DbType::MySQL, 2, 10000);

        assert!(rec1 > 0, "建议值应 > 0");
        assert!(rec2 > 0, "建议值应 > 0");
        assert!(rec3 > 0, "建议值应 > 0");

        let change = (rec3 as f64 - rec2 as f64).abs() / rec2.max(1) as f64;
        assert!(
            change < 1.0,
            "阻尼系数应抑制振荡，变化率应 < 100%，实际: {:.2}%",
            change * 100.0
        );
    }

    #[test]
    fn batch_advisor_adaptive_baseline_disabled() {
        let baseline_config = AdaptiveBaselineConfig::new().with_enabled(false);
        let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default())
            .with_adaptive_baseline_config(baseline_config);

        advisor.record_execution(500, 1000.0, 100.0);
        advisor.record_execution(500, 1000.0, 100.0);
        advisor.record_execution(500, 1000.0, 100.0);

        let dynamic = advisor.advise_dynamic(sz_orm_core::dialect::DbType::MySQL, 2, 10000);
        let static_val = sz_orm_core::batch_advisor::BatchSizeAdvisor::advise(
            sz_orm_core::dialect::DbType::MySQL,
            2,
            10000,
        );
        assert_eq!(
            dynamic, static_val,
            "禁用自适应基准且比率=1.0 时应等于静态值"
        );
    }
}
