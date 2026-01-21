/// Phase 6: Reciprocal Rank Fusion (RRF) for hybrid search
///
/// RRF is more robust than linear combination for merging ranked lists.
/// It works well across different score scales without normalization.
///
/// Formula: RRF(d) = Σ 1/(k + rank(d))
/// where k = 60 (standard constant from literature)

use std::collections::HashMap;

/// Standard RRF constant from literature
const DEFAULT_K: usize = 60;

/// Represents a ranked result with its position
#[derive(Debug, Clone)]
pub struct RankedResult {
    pub id: String,
    pub rank: usize,
}

/// Reciprocal Rank Fusion configuration
#[derive(Debug, Clone)]
pub struct RrfConfig {
    /// RRF constant (typically 60)
    pub k: usize,
}

impl Default for RrfConfig {
    fn default() -> Self {
        Self { k: DEFAULT_K }
    }
}

/// Merge multiple ranked lists using Reciprocal Rank Fusion
///
/// # Arguments
///
/// * `result_lists` - Vector of ranked result lists (each list is ordered by rank)
/// * `config` - RRF configuration
/// * `limit` - Maximum number of results to return
///
/// # Returns
///
/// Vector of (id, rrf_score) tuples, sorted by RRF score descending
///
/// # Example
///
/// ```
/// use ingestor_core::rrf::{rrf_fusion, RankedResult, RrfConfig};
///
/// let bm25_results = vec![
///     RankedResult { id: "doc1".to_string(), rank: 0 },
///     RankedResult { id: "doc2".to_string(), rank: 1 },
///     RankedResult { id: "doc3".to_string(), rank: 2 },
/// ];
///
/// let semantic_results = vec![
///     RankedResult { id: "doc3".to_string(), rank: 0 },
///     RankedResult { id: "doc1".to_string(), rank: 1 },
///     RankedResult { id: "doc4".to_string(), rank: 2 },
/// ];
///
/// let merged = rrf_fusion(
///     vec![bm25_results, semantic_results],
///     RrfConfig::default(),
///     10
/// );
///
/// // doc1 and doc3 appear in both lists, so they get higher RRF scores
/// ```
pub fn rrf_fusion(
    result_lists: Vec<Vec<RankedResult>>,
    config: RrfConfig,
    limit: usize,
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    for results in result_lists {
        for result in results {
            let rrf_score = 1.0 / (config.k + result.rank + 1) as f32;
            *scores.entry(result.id.clone()).or_default() += rrf_score;
        }
    }

    let mut merged: Vec<_> = scores.into_iter().collect();
    merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit);
    merged
}

/// Convert scored results to ranked results
///
/// Takes results with arbitrary scores and converts them to rank positions.
/// Higher scores get lower (better) ranks.
pub fn to_ranked_results<T>(results: &[(T, f32)]) -> Vec<RankedResult>
where
    T: AsRef<str>,
{
    results
        .iter()
        .enumerate()
        .map(|(rank, (id, _score))| RankedResult {
            id: id.as_ref().to_string(),
            rank,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_fusion_basic() {
        let list1 = vec![
            RankedResult { id: "doc1".to_string(), rank: 0 },
            RankedResult { id: "doc2".to_string(), rank: 1 },
        ];

        let list2 = vec![
            RankedResult { id: "doc2".to_string(), rank: 0 },
            RankedResult { id: "doc3".to_string(), rank: 1 },
        ];

        let merged = rrf_fusion(vec![list1, list2], RrfConfig::default(), 10);

        // doc2 appears first in list2 and second in list1, should rank highest
        assert_eq!(merged[0].0, "doc2");
        assert!(merged[0].1 > merged[1].1);
    }

    #[test]
    fn test_rrf_fusion_single_list() {
        let list = vec![
            RankedResult { id: "doc1".to_string(), rank: 0 },
            RankedResult { id: "doc2".to_string(), rank: 1 },
            RankedResult { id: "doc3".to_string(), rank: 2 },
        ];

        let merged = rrf_fusion(vec![list], RrfConfig::default(), 10);

        // Order should be preserved
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].0, "doc1");
        assert_eq!(merged[1].0, "doc2");
        assert_eq!(merged[2].0, "doc3");
    }

    #[test]
    fn test_rrf_fusion_limit() {
        let list1 = vec![
            RankedResult { id: "doc1".to_string(), rank: 0 },
            RankedResult { id: "doc2".to_string(), rank: 1 },
            RankedResult { id: "doc3".to_string(), rank: 2 },
        ];

        let merged = rrf_fusion(vec![list1], RrfConfig::default(), 2);

        // Should respect limit
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_rrf_boost_consensus() {
        // Documents appearing in multiple lists should rank higher
        let list1 = vec![
            RankedResult { id: "doc1".to_string(), rank: 0 },
            RankedResult { id: "doc2".to_string(), rank: 1 },
        ];

        let list2 = vec![
            RankedResult { id: "doc1".to_string(), rank: 0 },
            RankedResult { id: "doc3".to_string(), rank: 1 },
        ];

        let list3 = vec![
            RankedResult { id: "doc1".to_string(), rank: 1 },
            RankedResult { id: "doc4".to_string(), rank: 0 },
        ];

        let merged = rrf_fusion(vec![list1, list2, list3], RrfConfig::default(), 10);

        // doc1 appears in all 3 lists, should rank first
        assert_eq!(merged[0].0, "doc1");
    }

    #[test]
    fn test_to_ranked_results() {
        let scored = vec![
            ("doc1", 0.9),
            ("doc2", 0.8),
            ("doc3", 0.7),
        ];

        let ranked = to_ranked_results(&scored);

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].id, "doc1");
        assert_eq!(ranked[0].rank, 0);
        assert_eq!(ranked[1].id, "doc2");
        assert_eq!(ranked[1].rank, 1);
        assert_eq!(ranked[2].id, "doc3");
        assert_eq!(ranked[2].rank, 2);
    }

    #[test]
    fn test_custom_k_value() {
        let list = vec![
            RankedResult { id: "doc1".to_string(), rank: 0 },
            RankedResult { id: "doc2".to_string(), rank: 1 },
        ];

        let config = RrfConfig { k: 10 }; // Smaller k = more weight on rank differences
        let merged = rrf_fusion(vec![list], config, 10);

        // With k=10: doc1 gets 1/11, doc2 gets 1/12
        // With k=60: doc1 gets 1/61, doc2 gets 1/62
        // Smaller k makes score differences more pronounced
        assert!(merged[0].1 > 1.0 / 11.5); // Should be close to 1/11
        assert!(merged[1].1 < 1.0 / 11.5); // Should be close to 1/12
    }
}
