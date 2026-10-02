use anyhow::Result;
use tempfile::tempdir;
use tinymem_engine::{
    Embedder, Engine, MemoryKind, MemoryScope, MemorySearchOptions, NewMemory, SearchMode,
    SearchOptions,
};

struct Constant(usize);
impl Embedder for Constant {
    fn model(&self) -> &str {
        "same-identity"
    }
    fn dimensions(&self) -> usize {
        self.0
    }
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![1.0; self.0])
    }
}

#[test]
fn dimension_spaces_are_isolated_and_backfill_results_and_candidate_pools_are_capped() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("native.db");
    let scope = MemoryScope {
        user_id: "alice".into(),
        agent_id: None,
        project_id: None,
    };
    let mut writer = Engine::open(&db).unwrap();
    for index in 0..101 {
        writer
            .add_memory(
                &scope,
                NewMemory {
                    content: format!("record {index}"),
                    kind: MemoryKind::Fact,
                    source: "test".into(),
                    expires_at_ms: None,
                    source_refs: Vec::new(),
                },
            )
            .unwrap();
    }
    let mut engine = writer.with_embedder(Constant(3)).unwrap();
    let report = engine.index_memories(&scope, usize::MAX).unwrap();
    assert_eq!((report.indexed, report.remaining), (100, 1));
    assert_eq!(engine.index_memories(&scope, 100).unwrap().indexed, 1);
    let query = MemorySearchOptions {
        mode: SearchMode::Semantic,
        search: SearchOptions {
            limit: usize::MAX,
            max_bytes: usize::MAX,
        },
        min_similarity: 0.99,
    };
    assert_eq!(
        engine
            .search_memories(&scope, "query", query)
            .unwrap()
            .len(),
        100
    );
    let mut other_dimensions = Engine::open(&db)
        .unwrap()
        .with_embedder(Constant(2))
        .unwrap();
    assert!(
        other_dimensions
            .search_memories(&scope, "query", query)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        other_dimensions.index_memories(&scope, 1).unwrap().indexed,
        1
    );
    assert_eq!(
        other_dimensions
            .search_memories(&scope, "query", query)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        engine
            .search_memories(&scope, "query", query)
            .unwrap()
            .len(),
        100
    );
}
