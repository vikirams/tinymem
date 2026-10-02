//! Tools-only MCP 2025-06-18 over newline-delimited UTF-8 JSON-RPC.
//!
//! Operations are serialized. Cancellation notifications are ignored: they cannot
//! interrupt a running operation. No watcher, network listener, server requests, or
//! logging capability is provided. An optional trusted embedding executable may
//! perform its own IO; personal-memory reads never populate the derived cache.

use crate::{
    ContextOptions, ContextScope, ContextSource, Engine, MAX_CONTEXT_TOKENS, MemoryScope,
    MemorySearchOptions, MemoryUpdate, NewMemory, SearchMode, SearchOptions,
};
use anyhow::{Context, Result};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_VERSIONS: &[&str] = &[PROTOCOL_VERSION, "2025-03-26", "2024-11-05"];
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 3 * 1024 * 1024;
const MAX_ID_BYTES: usize = 256;

/// Serve one foreground stdio session with an immutable scope and canonical root.
/// Closing stdin ends the session; no background work or network listener is started.
pub fn serve(mut engine: Engine, root: PathBuf, scope: MemoryScope) -> Result<()> {
    // Let the engine validate its own scope contract without retrieving evidence.
    engine.memory_profile(
        &scope,
        SearchOptions {
            limit: 0,
            max_bytes: 0,
        },
    )?;
    let root = crate::source::repository_root(&root)?;
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = io::stdout().lock();
    let mut frame = Vec::with_capacity(MAX_INPUT_BYTES);
    let mut state = State::Fresh;
    let mut limiter = RateLimiter::new();
    while let Some(within_limit) =
        read_frame(&mut input, &mut frame).map_err(|_| anyhow::anyhow!("MCP input read failed"))?
    {
        let response = if !within_limit {
            Some(rpc_error(Value::Null, -32600, "Input frame exceeds 1 MiB"))
        } else {
            match serde_json::from_slice(&frame) {
                Ok(message) => handle(
                    message,
                    &mut state,
                    &mut limiter,
                    &mut engine,
                    &root,
                    &scope,
                ),
                Err(_) => Some(rpc_error(Value::Null, -32700, "Parse error")),
            }
        };
        if let Some(response) = response {
            // Serialize and size-check the complete frame before touching stdout.
            let mut bytes = serde_json::to_vec(&response).context("MCP serialization failed")?;
            if bytes.len() + 1 > MAX_OUTPUT_BYTES {
                let id = response["id"].clone();
                bytes = serde_json::to_vec(&rpc_result(
                    id,
                    tool_error("Tool result exceeds the output byte limit"),
                ))?;
            }
            bytes.push(b'\n');
            output
                .write_all(&bytes)
                .and_then(|()| output.flush())
                .map_err(|_| anyhow::anyhow!("MCP output write failed"))?;
        }
    }
    Ok(())
}

/// Retain at most 1 MiB; consume an oversized frame all the way to its newline.
/// A final EOF-terminated frame is also handled, then EOF exits successfully.
fn read_frame(input: &mut impl BufRead, frame: &mut Vec<u8>) -> io::Result<Option<bool>> {
    frame.clear();
    let mut seen = false;
    let mut oversized = false;
    loop {
        let buffer = input.fill_buf()?;
        if buffer.is_empty() {
            return Ok(seen.then_some(!oversized));
        }
        seen = true;
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let len = newline.unwrap_or(buffer.len());
        if !oversized {
            if len > MAX_INPUT_BYTES - frame.len() {
                oversized = true;
            } else {
                frame.extend_from_slice(&buffer[..len]);
            }
        }
        input.consume(len + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Some(!oversized));
        }
    }
}

#[derive(PartialEq, Eq)]
enum State {
    Fresh,
    AwaitingInitialized,
    Ready,
}

struct RateLimiter {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            tokens: 128.0,
            last: Instant::now(),
        }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        self.tokens =
            (self.tokens + now.duration_since(self.last).as_secs_f64() * 128.0).min(128.0);
        self.last = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

fn valid_id(id: &Value) -> bool {
    match id {
        Value::String(text) => text.len() <= MAX_ID_BYTES,
        Value::Number(number) => number.is_i64() || number.is_u64(),
        _ => false,
    }
}

fn handle(
    message: Value,
    state: &mut State,
    limiter: &mut RateLimiter,
    engine: &mut Engine,
    root: &Path,
    scope: &MemoryScope,
) -> Option<Value> {
    let Some(object) = message.as_object() else {
        return Some(rpc_error(Value::Null, -32600, "Invalid request"));
    };
    let id = object.get("id");
    let response_id = id.filter(|id| valid_id(id)).cloned().unwrap_or(Value::Null);
    let method = object.get("method").and_then(Value::as_str);
    let is_rpc = object.get("jsonrpc").and_then(Value::as_str) == Some("2.0");
    let valid_fields = object
        .keys()
        .all(|key| matches!(key.as_str(), "jsonrpc" | "id" | "method" | "params"));
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    // Route notifications before any tool dispatch. Invalid notification params
    // or extra fields are also silent and cannot mutate memory or ready the session.
    if id.is_none() && is_rpc && method.is_some() {
        if valid_fields
            && method == Some("notifications/initialized")
            && *state == State::AwaitingInitialized
            && parse::<EmptyParams>(params).is_ok()
        {
            *state = State::Ready;
        }
        return None;
    }
    if !is_rpc || method.is_none() || !valid_fields || id.is_some_and(|id| !valid_id(id)) {
        return Some(rpc_error(response_id, -32600, "Invalid request"));
    }
    let method = method.unwrap();
    let result = match method {
        "ping" => parse::<EmptyParams>(params).map(|_| json!({})),
        "initialize" => {
            if *state != State::Fresh {
                Err((-32600, "Already initialized"))
            } else {
                parse::<InitializeParams>(params).map(|params| {
                    let version = if SUPPORTED_VERSIONS.contains(&params.protocol_version.as_str()) {
                        params.protocol_version.as_str()
                    } else {
                        PROTOCOL_VERSION
                    };
                    *state = State::AwaitingInitialized;
                    json!({
                        "protocolVersion": version,
                        "capabilities": {"tools": {"listChanged": false}},
                        "serverInfo": {"name": "tinymem", "version": env!("CARGO_PKG_VERSION")}
                    })
                })
            }
        }
        "tools/list" | "tools/call" if *state != State::Ready => {
            Err((-32600, "Initialize and send notifications/initialized first"))
        }
        "tools/list" => parse::<ListParams>(params).and_then(|params| {
            if params.cursor.is_some_and(|cursor| !cursor.is_empty()) {
                Err((-32602, "Unsupported cursor"))
            } else {
                Ok(json!({"tools": definitions()}))
            }
        }),
        "tools/call" => parse::<CallParams>(params).and_then(|params| {
            let tool = Tool::parse(&params.name, params.arguments)?;
            if !limiter.allow() {
                return Ok(tool_error("Tool call rate limit exceeded; retry later"));
            }
            let assembly = matches!(&tool, Tool::Assemble(_));
            Ok(match tool.execute(engine, root, scope) {
                Ok(data) if assembly => json!({
                    // Model clients receive one budgeted evidence copy. SDKs retain
                    // the complete structured result, including entries and counts.
                    "content": [{"type": "text", "text": data["prompt"]}],
                    "structuredContent": data,
                    "isError": false,
                }),
                Ok(data) => tool_result(data, false),
                // SQLite/IO errors can contain paths, SQL, or another user's scope.
                // Return a static actionable message, never the engine error chain.
                Err(_) => tool_error("Tool execution failed: check input, memory availability, expected version, and local storage/repository state"),
            })
        }),
        _ => Err((-32601, "Method not found")),
    };
    Some(match result {
        Ok(result) => rpc_result(response_id, result),
        Err((code, message)) => rpc_error(response_id, code, message),
    })
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_result(data: Value, is_error: bool) -> Value {
    json!({
        "content": [{"type": "text", "text": serde_json::to_string(&data).expect("JSON value serializes")}],
        "structuredContent": data,
        "isError": is_error
    })
}

fn tool_error(message: &str) -> Value {
    tool_result(json!({"error": message}), true)
}

type ProtocolResult<T> = std::result::Result<T, (i32, &'static str)>;

fn parse<T: DeserializeOwned>(value: Value) -> ProtocolResult<T> {
    if !value.is_object() {
        return Err((-32602, "Parameters must be an object"));
    }
    // Never echo arguments (or serde's offending values) in protocol errors.
    serde_json::from_value(value).map_err(|_| (-32602, "Invalid parameters"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    #[serde(rename = "capabilities")]
    _capabilities: Map<String, Value>,
    #[serde(rename = "clientInfo")]
    _client_info: ClientInfo,
    #[serde(default, rename = "_meta")]
    _meta: Map<String, Value>,
}

#[derive(Deserialize)]
struct ClientInfo {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "version")]
    _version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {
    #[serde(default, rename = "_meta")]
    _meta: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default, rename = "_meta")]
    _meta: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallParams {
    name: String,
    #[serde(default)]
    arguments: Map<String, Value>,
    #[serde(default, rename = "_meta")]
    _meta: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSourceArgs {
    kind: crate::SourceKind,
    role: crate::SourceRole,
    path: String,
    expected_hash: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddArgs {
    content: String,
    #[serde(default)]
    kind: crate::MemoryKind,
    #[serde(default = "default_memory_source")]
    source: String,
    expires_at_ms: Option<u64>,
    #[serde(default)]
    source_refs: Vec<FileSourceArgs>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    id: String,
    content: String,
    #[serde(default)]
    expected_version: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteArgs {
    id: String,
    #[serde(default)]
    expected_version: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryArgs {
    id: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_max_bytes")]
    max_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_max_bytes")]
    max_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemorySearchArgs {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_max_bytes")]
    max_bytes: usize,
    #[serde(default)]
    mode: SearchMode,
    #[serde(default = "default_similarity")]
    min_similarity: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssembleArgs {
    query: String,
    token_budget: usize,
    #[serde(default = "default_context_sources")]
    sources: Vec<ContextSource>,
    #[serde(default)]
    exclude_paths: Vec<String>,
}

fn default_context_sources() -> Vec<ContextSource> {
    ContextOptions::default().sources
}

fn default_memory_source() -> String {
    "explicit".into()
}

fn default_similarity() -> f64 {
    0.25
}
fn default_index_limit() -> usize {
    100
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexArgs {
    #[serde(default = "default_index_limit")]
    limit: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileArgs {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_max_bytes")]
    max_bytes: usize,
}

fn default_limit() -> usize {
    SearchOptions::default().limit
}
fn default_max_bytes() -> usize {
    SearchOptions::default().max_bytes
}

enum Tool {
    Add(AddArgs),
    SearchMemories(MemorySearchArgs),
    IndexMemories(IndexArgs),
    Get(IdArgs),
    Update(UpdateArgs),
    Delete(DeleteArgs),
    History(HistoryArgs),
    Profile(ProfileArgs),
    Refresh(EmptyArgs),
    SearchRepository(SearchArgs),
    Assemble(AssembleArgs),
}

impl Tool {
    fn parse(name: &str, arguments: Map<String, Value>) -> ProtocolResult<Self> {
        let args = Value::Object(arguments);
        match name {
            "add_memory" => parse(args).map(Self::Add),
            "search_memories" => parse(args).map(Self::SearchMemories),
            "index_memories" => parse(args).map(Self::IndexMemories),
            "get_memory" => parse(args).map(Self::Get),
            "update_memory" => parse(args).map(Self::Update),
            "delete_memory" => parse(args).map(Self::Delete),
            "memory_history" => parse(args).map(Self::History),
            "get_profile" => parse(args).map(Self::Profile),
            "refresh_repository" => parse(args).map(Self::Refresh),
            "search_repository" => parse(args).map(Self::SearchRepository),
            "assemble_context" => parse(args).map(Self::Assemble),
            _ => Err((-32602, "Unknown tool")),
        }
    }

    fn execute(self, engine: &mut Engine, root: &Path, scope: &MemoryScope) -> Result<Value> {
        Ok(match self {
            Self::Add(args) => {
                anyhow::ensure!(
                    args.source_refs.len() <= crate::freshness::MAX_SOURCE_REFS,
                    "too many source references"
                );
                let source_refs = args
                    .source_refs
                    .into_iter()
                    .map(|reference| {
                        let crate::SourceKind::File = reference.kind;
                        let captured = crate::FileSourceReference::capture(
                            root,
                            &reference.path,
                            reference.role,
                        )?;
                        anyhow::ensure!(
                            reference
                                .expected_hash
                                .is_none_or(|hash| hash == captured.content_hash),
                            "source hash conflict"
                        );
                        Ok(captured)
                    })
                    .collect::<Result<Vec<_>>>()?;
                json!({"memory": engine.add_memory(scope, NewMemory {
                    content: args.content, kind: args.kind,
                    source: args.source,
                    expires_at_ms: args.expires_at_ms, source_refs,
                })?})
            }
            Self::SearchMemories(args) => json!({"memories": engine.search_memories(
                scope, &args.query, MemorySearchOptions {
                    search: SearchOptions { limit: args.limit, max_bytes: args.max_bytes },
                    mode: args.mode, min_similarity: args.min_similarity,
                }
            )?}),
            Self::IndexMemories(args) => {
                serde_json::to_value(engine.index_memories(scope, args.limit)?)?
            }
            Self::Get(args) => json!({"memory": engine.get_memory(scope, &args.id)?}),
            Self::Update(args) => {
                json!({"memory": engine.update_memory(scope, &args.id, MemoryUpdate {
                content: args.content, expected_version: args.expected_version,
            })?})
            }
            Self::Delete(args) => {
                json!({"deleted": engine.delete_memory(scope, &args.id, args.expected_version)?})
            }
            Self::History(args) => json!({"history": engine.memory_events(scope, &args.id,
                SearchOptions { limit: args.limit, max_bytes: args.max_bytes }
            )?}),
            Self::Profile(args) => serde_json::to_value(engine.memory_profile(
                scope,
                SearchOptions {
                    limit: args.limit,
                    max_bytes: args.max_bytes,
                },
            )?)?,
            Self::Assemble(args) => serde_json::to_value(engine.assemble_context_with_options(
                &args.query,
                &ContextScope {
                    memory: scope.clone(),
                    repository: Some(root.to_path_buf()),
                },
                args.token_budget,
                &ContextOptions {
                    sources: args.sources,
                    exclude_paths: args.exclude_paths,
                },
            )?)?,
            Self::Refresh(_) => serde_json::to_value(engine.refresh(root)?)?,
            Self::SearchRepository(args) => {
                let refresh = engine.refresh(root)?;
                let hits = engine.search(
                    root,
                    &args.query,
                    SearchOptions {
                        limit: args.limit,
                        max_bytes: args.max_bytes,
                    },
                )?;
                json!({"refresh": refresh, "hits": hits})
            }
        })
    }
}

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false})
}

fn definitions() -> Vec<Value> {
    let memory = json!({
        "content": {"type": "string"},
        "kind": {"type": "string", "enum": ["fact", "preference", "decision", "plan", "relationship", "architectural_decision", "issue_solution"], "default": "fact"},
        "source": {"type": "string", "default": "explicit"},
        "expires_at_ms": {"type": ["integer", "null"], "minimum": 0},
        "source_refs": {"type": "array", "maxItems": crate::freshness::MAX_SOURCE_REFS, "default": [],
            "items": schema(json!({
                "kind": {"type": "string", "enum": ["file"]},
                "role": {"type": "string", "enum": ["origin", "dependency"]},
                "path": {"type": "string", "maxLength": 4096, "description": "Normalized relative path under the fixed startup repository; no symlinks."},
                "expected_hash": {"type": ["string", "null"], "description": "Optional blake3:<hex> precondition. Engine captures the actual hash."}
            }), &["kind", "role", "path"])
        }
    });
    let options = json!({
        "limit": {"type": "integer", "minimum": 0, "default": default_limit()},
        "max_bytes": {"type": "integer", "minimum": 0, "default": default_max_bytes()}
    });
    let mut search = options.clone();
    search["query"] = json!({"type": "string"});
    let mut memory_search = search.clone();
    memory_search["mode"] =
        json!({"type": "string", "enum": ["lexical", "semantic", "hybrid"], "default": "lexical"});
    memory_search["min_similarity"] =
        json!({"type": "number", "minimum": -1, "maximum": 1, "default": default_similarity()});
    let version = json!({"type": ["integer", "null"], "minimum": 0});
    let update = json!({"id": {"type": "string"}, "content": {"type": "string"}, "expected_version": version});
    let delete = json!({"id": {"type": "string"}, "expected_version": version});
    let mut history = options.clone();
    history["id"] = json!({"type": "string"});
    let id = schema(json!({"id": {"type": "string"}}), &["id"]);
    let assembly = schema(
        json!({
            "query": {"type": "string"},
            "token_budget": {"type": "integer", "minimum": 0, "maximum": MAX_CONTEXT_TOKENS},
            "sources": {"type": "array", "items": {"type": "string", "enum": ["memory", "repository"]},
                "uniqueItems": true, "maxItems": 2, "default": ["memory", "repository"],
                "description": "Assembly-only sources; empty selects no evidence. Does not restrict explicit searches."},
            "exclude_paths": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 1024},
                "maxItems": 64, "default": [],
                "description": "Assembly-only repository-relative globs (case-sensitive; * excludes /, ** crosses it). At most 16 KiB total UTF-8. Never filters personal-memory provenance or explicit searches."}
        }),
        &["query", "token_budget"],
    );
    [
        ("add_memory", "Store a personal memory in the fixed startup scope.", schema(memory, &["content"]), false, false),
        ("search_memories", "Search active personal memories in the fixed scope. Lexical search is literal AND: use 1-3 distinctive terms; shorten on zero hits. Semantic/hybrid require a configured embedder and indexed memories. Avoid redundant recall when injected evidence already answers the task.", schema(memory_search, &["query"]), true, false),
        ("index_memories", "Explicitly backfill the configured embedding cache for up to 100 active scoped memories; repeat until remaining is zero. Does not change personal evidence or audit history.", schema(json!({"limit": {"type": "integer", "minimum": 0, "maximum": 100, "default": 100}}), &[]), false, false),
        ("get_memory", "Get an active scoped memory by ID, or null.", id.clone(), true, false),
        ("update_memory", "Update scoped memory content with an optional expected version, retaining its ID and audit history.", schema(update, &["id", "content"]), false, true),
        ("delete_memory", "Hide current scoped memory with an optional expected version; retain its audit history (not secure erasure).", schema(delete, &["id"]), false, true),
        ("memory_history", "Read bounded immutable scoped memory audit events, including deletion, with current dependency freshness checks outside the snapshots.", schema(history, &["id"]), true, false),
        ("get_profile", "Bounded stable and dynamic active memories in the fixed scope; no generated summary.", schema(options, &[]), true, false),
        ("refresh_repository", "Reconcile the fixed repository's local index cache.", schema(json!({}), &[]), false, false),
        ("search_repository", "Refresh the fixed repository cache, then search current source. Literal AND: use 1-3 distinctive terms; shorten on zero hits. Automatic-context source filters do not restrict this search.", schema(search, &["query"]), false, false),
        ("assemble_context", "Select scoped current evidence from task text up to 64 KiB; pack whole entries with provenance. Model text is only the budgeted prompt; structuredContent retains entries and counts. Budget is a conservative UTF-8-byte token upper bound. Personal memory is read-only. Skip redundant recall when injected evidence suffices.", assembly, false, false),
    ].into_iter().map(|(name, description, input, read_only, destructive)| json!({
        "name": name, "description": description, "inputSchema": input,
        "annotations": {"readOnlyHint": read_only, "destructiveHint": destructive, "openWorldHint": false}
    })).collect()
}
