//! Local, model-free assembly timing; not a relevance or end-to-end Pi benchmark.
//! Run: cargo run --release -p tinymem-engine --example context_benchmark
use anyhow::Result;
use std::{hint::black_box, time::Instant};
use tempfile::tempdir;
use tinymem_engine::{ContextScope, Engine, MAX_CONTEXT_TOKENS, MemoryScope, NewMemory};

fn main() -> Result<()> {
    let directory = tempdir()?;
    let mut engine = Engine::open(directory.path().join("memory.db"))?;
    let scope = ContextScope {
        memory: MemoryScope {
            user_id: "benchmark".into(),
            agent_id: None,
            project_id: None,
        },
        repository: None,
    };
    for index in 0..100 {
        engine.add_memory(
            &scope.memory,
            NewMemory {
                content: format!(
                    "auth note {index}: {}",
                    "quoted \"text\" café\\path\n".repeat(60)
                ),
                kind: tinymem_engine::MemoryKind::Fact,
                source: "benchmark".into(),
                expires_at_ms: None,
                source_refs: Vec::new(),
            },
        )?;
    }
    for budget in [4000, MAX_CONTEXT_TOKENS] {
        let expected = engine.assemble_context("auth", &scope, budget)?;
        let start = Instant::now();
        for _ in 0..200 {
            let result = engine.assemble_context(black_box("auth"), &scope, budget)?;
            assert_eq!(result.prompt, expected.prompt);
            black_box(result);
        }
        println!(
            "budget={budget} entries={} bytes={} iterations=200 elapsed_ms={:.2}",
            expected.entries.len(),
            expected.prompt.len(),
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(())
}
