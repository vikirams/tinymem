//! Optional embeddings. Model identity is explicit; lexical operation needs no provider.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const MAX_DIMENSIONS: usize = 4096;
const MAX_STDOUT: usize = 128 * 1024;
const MAX_STDERR: usize = 16 * 1024;

/// A trusted adapter must use the same model/preprocessing for documents and queries.
/// Change the model identity whenever weights or preprocessing change.
pub trait Embedder: Send {
    fn model(&self) -> &str;
    fn dimensions(&self) -> usize;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

pub(crate) struct Provider {
    pub model: String,
    pub dimensions: usize,
    pub adapter: Box<dyn Embedder>,
}

impl Provider {
    pub fn new(adapter: impl Embedder + 'static) -> Result<Self> {
        let model = adapter.model().to_owned();
        let dimensions = adapter.dimensions();
        validate_model(&model, dimensions)?;
        Ok(Self {
            model,
            dimensions,
            adapter: Box::new(adapter),
        })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        normalize(self.adapter.embed(text)?, self.dimensions)
    }
}

fn validate_model(model: &str, dimensions: usize) -> Result<()> {
    ensure!(
        !model.trim().is_empty() && model.len() <= 4096 && !model.contains('\0'),
        "embedding model must be nonblank, NUL-free and at most 4096 bytes"
    );
    ensure!(
        (1..=MAX_DIMENSIONS).contains(&dimensions),
        "embedding dimensions must be between 1 and 4096"
    );
    Ok(())
}

pub(crate) fn normalize(mut vector: Vec<f32>, dimensions: usize) -> Result<Vec<f32>> {
    ensure!(vector.len() == dimensions, "embedding dimension mismatch");
    ensure!(
        vector.iter().all(|v| v.is_finite()),
        "embedding is not finite"
    );
    let norm = vector
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    ensure!(norm > 0.0, "embedding has zero norm");
    for value in &mut vector {
        *value = (f64::from(*value) / norm) as f32;
    }
    Ok(vector)
}

pub(crate) fn encode(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

pub(crate) fn decode(bytes: &[u8], dimensions: usize) -> Result<Vec<f32>> {
    ensure!(
        bytes.len() == dimensions * 4,
        "invalid stored embedding size"
    );
    normalize(
        bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect(),
        dimensions,
    )
}

/// A fresh, shell-free executable per text, with bounded streams and a finite deadline.
/// Input: {"model": "identity", "text": "..."}; output: {"embedding": [float, ...]}.
/// The executable is trusted configuration, not a sandbox; it must not leave descendants
/// holding its pipes open. Adapter errors/stderr are never echoed into MCP results.
pub struct CommandEmbedder {
    executable: PathBuf,
    model: String,
    dimensions: usize,
    timeout: Duration,
}

impl CommandEmbedder {
    pub fn new(
        executable: impl AsRef<Path>,
        model: impl Into<String>,
        dimensions: usize,
        timeout: Duration,
    ) -> Result<Self> {
        let model = model.into();
        validate_model(&model, dimensions)?;
        ensure!(
            !timeout.is_zero() && timeout <= Duration::from_secs(120),
            "embedding timeout must be positive and at most 120 seconds"
        );
        Ok(Self {
            executable: executable.as_ref().to_owned(),
            model,
            dimensions,
            timeout,
        })
    }
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    text: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    embedding: Vec<f32>,
}

impl Embedder for CommandEmbedder {
    fn model(&self) -> &str {
        &self.model
    }
    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let request = serde_json::to_vec(&Request {
            model: &self.model,
            text,
        })?;
        let mut child = Command::new(&self.executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("cannot start embedding executable")?;
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (send, receive) = mpsc::channel();
        let writer = send.clone();
        thread::spawn(move || {
            let result = stdin.write_all(&request).map(|()| None);
            drop(stdin); // EOF is part of the request contract.
            let _ = writer.send(result);
        });
        let output = send.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .by_ref()
                .take((MAX_STDOUT + 1) as u64)
                .read_to_end(&mut bytes)
                .and_then(|_| {
                    if bytes.len() > MAX_STDOUT {
                        Err(std::io::Error::other("embedding stdout limit"))
                    } else {
                        Ok(Some(bytes))
                    }
                });
            let _ = output.send(result);
        });
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stderr
                .by_ref()
                .take((MAX_STDERR + 1) as u64)
                .read_to_end(&mut bytes)
                .and_then(|_| {
                    if bytes.len() > MAX_STDERR {
                        Err(std::io::Error::other("embedding stderr limit"))
                    } else {
                        Ok(None)
                    }
                });
            let _ = send.send(result);
        });
        let deadline = Instant::now() + self.timeout;
        let result = (|| {
            let mut completed = 0;
            let mut bytes = None;
            let mut status = None;
            loop {
                while let Ok(result) = receive.try_recv() {
                    completed += 1;
                    if let Some(output) = result.context("embedding stream failed")? {
                        bytes = Some(output);
                    }
                }
                if status.is_none() {
                    status = child.try_wait()?;
                }
                if completed == 3
                    && let Some(status) = status
                {
                    ensure!(status.success(), "embedding executable failed");
                    let response: Response = serde_json::from_slice(&bytes.unwrap_or_default())
                        .context("invalid embedding response")?;
                    return normalize(response.embedding, self.dimensions);
                }
                ensure!(Instant::now() < deadline, "embedding executable timed out");
                thread::sleep(Duration::from_millis(5));
            }
        })();
        if result.is_err() {
            let _ = child.kill();
        }
        // Reap the owned child even on malformed output, stream overflow or timeout.
        let _ = child.wait();
        result
    }
}
