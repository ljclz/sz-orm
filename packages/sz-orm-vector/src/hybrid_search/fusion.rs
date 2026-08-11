//! 融合排序（RRF + 加权 + 级联）

use super::{FusionStrategy, HybridSearchResult, SearchResultSource, SourceResult};
use std::collections::HashMap;

/// 融合三源结果
pub fn fuse(
    vector_results: &[SourceResult],
    fulltext_results: &[SourceResult],
    structured_results: &[SourceResult],
    strategy: FusionStrategy,
    top_k: usize,
) -> Vec<HybridSearchResult> {
    match strategy {
        FusionStrategy::Rrf { k } => rrf_fusion(
            vector_results,
            fulltext_results,
            structured_results,
            k,
            top_k,
        ),
        FusionStrategy::Weighted {
            vector_w,
            fulltext_w,
            structured_w,
        } => weighted_fusion(
            vector_results,
            fulltext_results,
            structured_results,
            vector_w,
            fulltext_w,
            structured_w,
            top_k,
        ),
        FusionStrategy::Cascade => {
            cascade_fusion(vector_results, fulltext_results, structured_results, top_k)
        }
    }
}

/// RRF 融合：`score = Σ 1/(k + rank_i)`
fn rrf_fusion(
    vector_results: &[SourceResult],
    fulltext_results: &[SourceResult],
    structured_results: &[SourceResult],
    k: u32,
    top_k: usize,
) -> Vec<HybridSearchResult> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    for (rank, result) in vector_results.iter().enumerate() {
        let rrf_score = 1.0 / (k as f32 + rank as f32 + 1.0);
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
    }
    for (rank, result) in fulltext_results.iter().enumerate() {
        let rrf_score = 1.0 / (k as f32 + rank as f32 + 1.0);
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
    }
    for (rank, result) in structured_results.iter().enumerate() {
        let rrf_score = 1.0 / (k as f32 + rank as f32 + 1.0);
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
    }

    let mut fused: Vec<HybridSearchResult> = scores
        .into_iter()
        .map(|(id, score)| HybridSearchResult {
            id,
            score,
            source: SearchResultSource::Hybrid,
            metadata: HashMap::new(),
        })
        .collect();

    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    fused.truncate(top_k);
    fused
}

/// 加权融合：`score = Σ weight_i × normalized_score_i`
fn weighted_fusion(
    vector_results: &[SourceResult],
    fulltext_results: &[SourceResult],
    structured_results: &[SourceResult],
    vector_w: f32,
    fulltext_w: f32,
    structured_w: f32,
    top_k: usize,
) -> Vec<HybridSearchResult> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    let normalize = |results: &[SourceResult]| -> HashMap<String, f32> {
        if results.is_empty() {
            return HashMap::new();
        }
        let max_score = results.iter().map(|r| r.score).fold(0.0f32, f32::max);
        let min_score = results
            .iter()
            .map(|r| r.score)
            .fold(f32::INFINITY, f32::min);
        let range = max_score - min_score;
        results
            .iter()
            .map(|r| {
                let normalized = if range > 0.0 {
                    (r.score - min_score) / range
                } else {
                    1.0
                };
                (r.id.clone(), normalized)
            })
            .collect()
    };

    for (id, norm_score) in normalize(vector_results) {
        *scores.entry(id).or_insert(0.0) += vector_w * norm_score;
    }
    for (id, norm_score) in normalize(fulltext_results) {
        *scores.entry(id).or_insert(0.0) += fulltext_w * norm_score;
    }
    for (id, norm_score) in normalize(structured_results) {
        *scores.entry(id).or_insert(0.0) += structured_w * norm_score;
    }

    let mut fused: Vec<HybridSearchResult> = scores
        .into_iter()
        .map(|(id, score)| HybridSearchResult {
            id,
            score,
            source: SearchResultSource::Hybrid,
            metadata: HashMap::new(),
        })
        .collect();

    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    fused.truncate(top_k);
    fused
}

/// 级联融合：向量召回 → 全文精排 → 结构化过滤
fn cascade_fusion(
    vector_results: &[SourceResult],
    fulltext_results: &[SourceResult],
    structured_results: &[SourceResult],
    top_k: usize,
) -> Vec<HybridSearchResult> {
    let fulltext_ids: std::collections::HashSet<&str> =
        fulltext_results.iter().map(|r| r.id.as_str()).collect();
    let structured_ids: std::collections::HashSet<&str> =
        structured_results.iter().map(|r| r.id.as_str()).collect();

    let mut candidates: Vec<&SourceResult> = vector_results.iter().collect();

    if !fulltext_results.is_empty() {
        candidates.sort_by(|a, b| {
            let a_in_ft = fulltext_ids.contains(a.id.as_str());
            let b_in_ft = fulltext_ids.contains(b.id.as_str());
            b_in_ft.cmp(&a_in_ft)
        });
    }

    if !structured_results.is_empty() {
        candidates.retain(|r| structured_ids.contains(r.id.as_str()));
    }

    candidates
        .into_iter()
        .take(top_k)
        .map(|r| HybridSearchResult {
            id: r.id.clone(),
            score: r.score,
            source: SearchResultSource::Hybrid,
            metadata: HashMap::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(id: &str, score: f32, source: SearchResultSource) -> SourceResult {
        SourceResult {
            id: id.to_string(),
            score,
            source,
        }
    }

    #[test]
    fn test_rrf_fusion() {
        let vector = vec![
            make_result("doc1", 0.9, SearchResultSource::Vector),
            make_result("doc2", 0.8, SearchResultSource::Vector),
        ];
        let fulltext = vec![
            make_result("doc2", 0.85, SearchResultSource::Fulltext),
            make_result("doc3", 0.7, SearchResultSource::Fulltext),
        ];

        let fused = rrf_fusion(&vector, &fulltext, &[], 60, 10);
        assert_eq!(fused.len(), 3);

        let doc2 = fused.iter().find(|r| r.id == "doc2").unwrap();
        let expected_doc2 = 1.0 / (60.0 + 1.0) + 1.0 / (60.0 + 1.0);
        assert!((doc2.score - expected_doc2).abs() < 0.001);
    }

    #[test]
    fn test_weighted_fusion() {
        let vector = vec![make_result("doc1", 1.0, SearchResultSource::Vector)];
        let fulltext = vec![make_result("doc1", 0.8, SearchResultSource::Fulltext)];

        let fused = weighted_fusion(&vector, &fulltext, &[], 0.5, 0.3, 0.2, 10);
        assert_eq!(fused.len(), 1);
        assert!(fused[0].score > 0.0);
    }

    #[test]
    fn test_cascade_fusion() {
        let vector = vec![
            make_result("doc1", 0.9, SearchResultSource::Vector),
            make_result("doc2", 0.8, SearchResultSource::Vector),
            make_result("doc3", 0.7, SearchResultSource::Vector),
        ];
        let fulltext = vec![make_result("doc2", 0.85, SearchResultSource::Fulltext)];
        let structured = vec![
            make_result("doc1", 1.0, SearchResultSource::Structured),
            make_result("doc2", 1.0, SearchResultSource::Structured),
        ];

        let fused = cascade_fusion(&vector, &fulltext, &structured, 10);
        assert!(fused.iter().all(|r| r.id == "doc1" || r.id == "doc2"));
        assert!(fused.iter().any(|r| r.id == "doc2"));
    }

    #[test]
    fn test_empty_results() {
        let fused = rrf_fusion(&[], &[], &[], 60, 10);
        assert!(fused.is_empty());
    }
}
