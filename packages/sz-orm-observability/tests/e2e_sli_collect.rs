//! 端到端测试：SLI 采集 → SLO 计算

use sz_orm_observability::slo_automation::{
    CalcWindow, RequestResult, SliCollector, SloAchievementCalculator,
};

#[tokio::test]
async fn e2e_sli_collect_and_slo() {
    let collector = SliCollector::new();
    for i in 0..200 {
        collector.collect(RequestResult {
            success: true,
            latency_ms: 10.0,
            timestamp: i,
        });
    }
    let sli = collector.calculate_sli();
    assert!(sli.availability > 0.99);

    let calc = SloAchievementCalculator::new(0.999);
    let metrics: Vec<_> = (0..200).map(|_| sli.clone()).collect();
    let achievement = calc
        .calculate(&metrics, CalcWindow::TwentyFourHours)
        .unwrap();
    assert!(achievement.sufficient_data);
}
