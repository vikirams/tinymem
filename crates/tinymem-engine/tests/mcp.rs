use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn structured_file_dependencies_are_bound_to_startup_root_and_survive_restart() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("clock.txt"), "UTC").unwrap();
    let mut server = Server::ready(root);
    let tools = server.rpc("tools/list", json!({}));
    let add = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "add_memory")
        .unwrap();
    assert_eq!(
        add["inputSchema"]["properties"]["source_refs"]["maxItems"],
        16
    );
    let saved = server.tool("add_memory", json!({"content":"clock correction", "source_refs":[{"kind":"file","role":"dependency","path":"clock.txt"}]}))["memory"].clone();
    let id = saved["id"].clone();
    assert_eq!(
        saved["source_refs"][0]["repository"],
        root.canonicalize().unwrap().to_str().unwrap()
    );
    assert!(
        saved["source_refs"][0]["content_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert_eq!(server.tool("get_memory", json!({"id":id}))["memory"], saved);
    let context = server.tool(
        "assemble_context",
        json!({"query":"correction","token_budget":8000}),
    );
    assert_eq!(
        context["entries"][0]["provenance"][0]["source_refs"],
        saved["source_refs"]
    );
    // Neither arbitrary roots nor caller-supplied capture timestamps/hashes are accepted.
    for extra in [
        json!({"repository":"/elsewhere"}),
        json!({"content_hash":"fabricated"}),
        json!({"observed_at_ms":0}),
    ] {
        let mut reference = json!({"kind":"file","role":"dependency","path":"clock.txt"});
        reference
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(
            server.call(
                "add_memory",
                json!({"content":"bad", "source_refs":[reference]})
            )["error"]["code"],
            -32602
        );
    }
    assert_eq!(server.call("add_memory", json!({"content":"bad", "source_refs":[{"kind":"file","role":"dependency","path":"../clock.txt"}]}))["result"]["isError"], true);
    assert_eq!(server.call("add_memory", json!({"content":"bad", "source_refs":[{"kind":"file","role":"dependency","path":"clock.txt","expected_hash":"blake3:wrong"}]}))["result"]["isError"], true);
    server.close();
    fs::remove_file(root.join("clock.txt")).unwrap();
    let mut server = Server::ready(root);
    assert!(server.tool("get_memory", json!({"id":id}))["memory"].is_null());
    assert_eq!(
        server.tool("search_memories", json!({"query":"clock"}))["memories"],
        json!([])
    );
    assert_eq!(
        server.tool("get_profile", json!({})),
        json!({"stable":[],"dynamic":[]})
    );
    assert_eq!(
        server.tool(
            "assemble_context",
            json!({"query":"clock","token_budget":8000})
        )["entries"],
        json!([])
    );
    let history = server.tool("memory_history", json!({"id":id}));
    assert_eq!(history["history"][0]["memory"], saved);
    assert_eq!(history["history"][0]["freshness"]["reason"], "file_missing");
    server.close();
}

fn initialize() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}
    }})
}

fn initialized() -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
}

// All assertions use the actual binary's stdio interface and real filesystem/DB.
// Drain stdout concurrently so large input/output tests cannot deadlock on pipes.
struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Receiver<Value>,
    reader: Option<JoinHandle<()>>,
    next_id: u64,
    closed: bool,
}

impl Server {
    fn spawn(
        root: &Path,
        user: &str,
        agent: Option<&str>,
        project: Option<&str>,
        db: Option<&Path>,
    ) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tinymem"));
        if let Some(db) = db {
            command.arg("--db").arg(db);
        }
        command.arg("mcp").arg(root).args(["--user", user]);
        if let Some(agent) = agent {
            command.args(["--agent", agent]);
        }
        if let Some(project) = project {
            command.args(["--project", project]);
        }
        Self::from_command(command)
    }

    fn from_command(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, responses) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                if stdout.read_until(b'\n', &mut line).unwrap() == 0 {
                    break;
                }
                assert_eq!(line.last(), Some(&b'\n'), "incomplete output frame");
                assert!(
                    line.len() <= 3 * 1024 * 1024,
                    "output exceeds SDK frame cap"
                );
                let value: Value =
                    serde_json::from_slice(&line).expect("stdout must contain only UTF-8 JSON");
                assert_eq!(value["jsonrpc"], "2.0");
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            responses,
            reader: Some(reader),
            next_id: 1,
            closed: false,
        }
    }

    fn ready(root: &Path) -> Self {
        let mut server = Self::spawn(root, "alice", None, None, None);
        server.handshake();
        server
    }

    fn handshake(&mut self) {
        self.send(&initialize());
        let response = self.receive();
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            response["result"]["capabilities"],
            json!({"tools":{"listChanged":false}})
        );
        self.send(&initialized());
    }

    fn raw(&mut self, bytes: &[u8]) {
        self.stdin.as_mut().unwrap().write_all(bytes).unwrap();
        self.stdin.as_mut().unwrap().flush().unwrap();
    }

    fn send(&mut self, message: &Value) {
        let mut frame = serde_json::to_vec(message).unwrap();
        frame.push(b'\n');
        self.raw(&frame);
    }

    fn receive(&self) -> Value {
        self.responses
            .recv_timeout(Duration::from_secs(10))
            .expect("missing server response")
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let response = self.receive();
        assert_eq!(
            response["id"], id,
            "unexpected response (possibly a notification reply)"
        );
        response
    }

    fn call(&mut self, name: &str, args: Value) -> Value {
        self.rpc("tools/call", json!({"name":name,"arguments":args}))
    }

    fn tool(&mut self, name: &str, args: Value) -> Value {
        let response = self.call(name, args);
        successful_data(&response)
    }

    fn close(mut self) {
        drop(self.stdin.take());
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "server did not exit on EOF"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "server must exit successfully on EOF: {status}"
        );
        self.reader.take().unwrap().join().unwrap();
        assert!(self.responses.try_recv().is_err(), "unsolicited output");
        let mut stderr = String::new();
        std::io::Read::read_to_string(&mut self.child.stderr.take().unwrap(), &mut stderr).unwrap();
        assert!(stderr.is_empty(), "unexpected server logs: {stderr}");
        self.closed = true;
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if !self.closed {
            drop(self.stdin.take());
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }
}

fn successful_data(response: &Value) -> Value {
    assert!(response.get("error").is_none(), "{response}");
    let result = &response["result"];
    assert_eq!(result["isError"], false, "{response}");
    assert!(result["structuredContent"].is_object());
    assert_eq!(result["content"].as_array().unwrap().len(), 1);
    assert_eq!(result["content"][0]["type"], "text");
    let text = result["content"][0]["text"].as_str().unwrap();
    let structured = result["structuredContent"].clone();
    if structured.get("prompt").is_some() {
        assert_eq!(text, structured["prompt"]);
        structured
    } else {
        let text_data: Value = serde_json::from_str(text).unwrap();
        assert_eq!(text_data, structured);
        text_data
    }
}

fn assert_error(response: &Value, code: i32) {
    assert_eq!(response["error"]["code"], code, "{response}");
    assert!(response.get("result").is_none());
}

fn assert_business_error(response: &Value) {
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
    let data = &response["result"]["structuredContent"];
    assert!(data.is_object(), "{response}");
    assert!(data["error"].is_string(), "{response}");
    let text: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, data);
}

fn session(root: &Path, input: &[Value]) -> Vec<Value> {
    let mut server = Server::spawn(root, "alice", None, None, None);
    for message in input {
        server.send(message);
    }
    let messages = (0..2).map(|_| server.receive()).collect();
    server.close();
    messages
}

#[test]
fn initializes_and_lists_scoped_native_tools_without_logging_on_stdout() {
    let repo = tempdir().unwrap();
    let messages = session(
        repo.path(),
        &[
            initialize(),
            initialized(),
            json!({"jsonrpc":"2.0","id":"list","method":"tools/list"}),
        ],
    );
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        messages[0]["result"]["capabilities"],
        json!({"tools":{"listChanged":false}})
    );
    assert!(messages[0]["result"]["serverInfo"]["name"].is_string());
    assert!(messages[0]["result"]["serverInfo"]["version"].is_string());
    assert_eq!(messages[1]["id"], "list");
    let tools = messages[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 11);
    for name in [
        "add_memory",
        "search_memories",
        "index_memories",
        "get_memory",
        "get_profile",
        "update_memory",
        "delete_memory",
        "memory_history",
        "refresh_repository",
        "search_repository",
        "assemble_context",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing {name}"
        );
    }
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        let name = tool["name"].as_str().unwrap();
        let read_only = matches!(
            name,
            "get_memory" | "search_memories" | "memory_history" | "get_profile"
        );
        assert_eq!(tool["annotations"]["readOnlyHint"], read_only);
        if name == "delete_memory" {
            assert_eq!(tool["annotations"]["destructiveHint"], true);
        }
        if name == "add_memory" {
            let properties = &tool["inputSchema"]["properties"];
            assert_eq!(properties["kind"]["default"], "fact");
            assert_eq!(properties["source"]["default"], "explicit");
            assert!(properties.get("metadata").is_none());
            assert!(properties.get("key").is_none());
            assert_eq!(
                properties["kind"]["enum"],
                json!([
                    "fact",
                    "preference",
                    "decision",
                    "plan",
                    "relationship",
                    "architectural_decision",
                    "issue_solution"
                ])
            );
        }
        if matches!(
            name,
            "search_memories" | "search_repository" | "get_profile" | "memory_history"
        ) {
            assert_eq!(tool["inputSchema"]["properties"]["limit"]["default"], 5);
            assert_eq!(
                tool["inputSchema"]["properties"]["max_bytes"]["default"],
                16000
            );
        }
    }
    for name in ["update_memory", "delete_memory"] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        assert_eq!(
            tool["inputSchema"]["properties"]["expected_version"]["minimum"],
            0
        );
        assert_eq!(
            tool["inputSchema"]["properties"]["expected_version"]["type"],
            json!(["integer", "null"])
        );
        assert!(tool["inputSchema"]["properties"].get("kind").is_none());
    }
    assert!(messages[1]["result"].get("nextCursor").is_none());
}

#[test]
fn lifecycle_requires_initialize_then_initialized_and_negotiates_supported_version() {
    let repo = tempdir().unwrap();
    let mut server = Server::spawn(repo.path(), "alice", None, None, None);
    server.send(&initialized()); // An early notification cannot ready the server.
    assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    assert_error(
        &server.call("add_memory", json!({"content":"must not be stored"})),
        -32600,
    );
    assert_error(&server.rpc("tools/list", json!({})), -32600);
    for bad in [
        json!({}),
        json!({"protocolVersion":null,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}),
        json!({"protocolVersion":"2025-06-18","capabilities":[],"clientInfo":{"name":"test","version":"1"}}),
        json!({"protocolVersion":"2025-06-18","capabilities":null,"clientInfo":{"name":"test","version":"1"}}),
        json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":3,"version":"1"}}),
        json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":false}}),
        json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test"}}),
        json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"},"root":"elsewhere"}),
    ] {
        assert_error(&server.rpc("initialize", bad), -32602);
    }
    let mut params = initialize()["params"].clone();
    params["protocolVersion"] = json!("2099-01-01");
    params["capabilities"] = json!({"roots":{"listChanged":true},"sampling":{}});
    params["clientInfo"]["title"] = json!("Test Client");
    assert_eq!(
        server.rpc("initialize", params.clone())["result"]["protocolVersion"],
        "2025-06-18"
    );
    assert_error(&server.rpc("initialize", params.clone()), -32600);
    assert_error(&server.rpc("tools/list", json!({})), -32600);
    assert_error(
        &server.call("add_memory", json!({"content":"also must not be stored"})),
        -32600,
    );
    server.send(
        &json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{"unexpected":true}}),
    );
    assert_error(&server.rpc("tools/list", json!({})), -32600);
    assert_error(&server.rpc("notifications/initialized", json!({})), -32601);
    assert_error(&server.rpc("tools/list", json!({})), -32600);
    server.send(&initialized());
    assert_eq!(
        server.tool("get_profile", json!({})),
        json!({"stable":[],"dynamic":[]})
    );
    assert_error(&server.rpc("initialize", params), -32600);
    server.send(&initialized()); // Duplicate notification is harmless.
    assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    server.close();
}

#[test]
fn initialize_echoes_each_supported_version_and_falls_back_to_latest() {
    let repo = tempdir().unwrap();
    for (requested, negotiated) in [
        ("2025-06-18", "2025-06-18"),
        ("2025-03-26", "2025-03-26"),
        ("2024-11-05", "2024-11-05"),
        ("2099-01-01", "2025-06-18"),
    ] {
        let mut server = Server::spawn(repo.path(), "alice", None, None, None);
        let mut params = initialize()["params"].clone();
        params["protocolVersion"] = json!(requested);
        let response = server.rpc("initialize", params);
        assert_eq!(response["result"]["protocolVersion"], negotiated);
        server.send(&initialized());
        let memory = server.tool("add_memory", json!({"content":"compatible client"}));
        assert_eq!(memory["memory"]["source"], "explicit");
        server.close();
    }
}

#[test]
fn scoped_memories_survive_restart_updates_conflicts_and_deletion_with_audit_history() {
    let repo = tempdir().unwrap();
    let dbdir = tempdir().unwrap();
    let db = dbdir.path().join("nested/shared.sqlite");
    let input = json!({
        "content":"prefers Rust café 東京\nsecond line", "kind":"preference",
        "source":"explicit user message", "expires_at_ms":4_102_444_800_000_u64
    });
    let mut server = Server::spawn(repo.path(), "alice", Some("worker"), Some("one"), Some(&db));
    server.handshake();
    let original = server.tool("add_memory", input.clone())["memory"].clone();
    let id = original["id"].as_str().unwrap();
    assert_eq!(original["content"], input["content"]);
    assert_eq!(
        original["scope"],
        json!({"user_id":"alice","agent_id":"worker","project_id":"one"})
    );
    assert_eq!(original["version"], 1);
    assert_eq!(server.tool("add_memory", input.clone())["memory"], original);
    assert_eq!(
        server.tool("memory_history", json!({"id":id}))["history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    server.close();
    assert!(db.is_file());
    assert!(!repo.path().join(".tinymem").exists());

    // Every exact scope tuple is isolated, including omitted agent/project.
    for (user, agent, project) in [
        ("bob", Some("worker"), Some("one")),
        ("alice", None, Some("one")),
        ("alice", Some("different"), Some("one")),
        ("alice", Some("worker"), None),
        ("alice", Some("worker"), Some("two")),
        ("alice", None, None),
    ] {
        let mut foreign = Server::spawn(repo.path(), user, agent, project, Some(&db));
        foreign.handshake();
        assert_eq!(
            foreign.tool("get_memory", json!({"id":id})),
            json!({"memory":null})
        );
        assert_eq!(
            foreign.tool("search_memories", json!({"query":"Rust"})),
            json!({"memories":[]})
        );
        assert_eq!(
            foreign.tool("memory_history", json!({"id":id})),
            json!({"history":[]})
        );
        assert_eq!(
            foreign.tool("get_profile", json!({})),
            json!({"stable":[],"dynamic":[]})
        );
        assert_eq!(
            foreign.tool("delete_memory", json!({"id":id})),
            json!({"deleted":false})
        );
        let error = foreign.call(
            "update_memory",
            json!({"id":id,"content":"foreign overwrite"}),
        );
        assert_business_error(&error);
        let error_text = error.to_string();
        for private in ["alice", "worker", "one", id, "prefers Rust"] {
            assert!(
                !error_text.contains(private),
                "error disclosed private evidence: {error}"
            );
        }
        let local = foreign.tool("add_memory", input.clone())["memory"].clone();
        assert_ne!(
            local["id"], original["id"],
            "duplicates must be scope-local"
        );
        assert_eq!(foreign.tool("add_memory", input.clone())["memory"], local);
        foreign.close();
    }

    let mut server = Server::spawn(repo.path(), "alice", Some("worker"), Some("one"), Some(&db));
    server.handshake();
    assert_eq!(
        server.tool("get_memory", json!({"id":id}))["memory"],
        original
    );
    let updated = server.tool(
        "update_memory",
        json!({
            "id":id, "content":"Rust remains the choice", "expected_version":1
        }),
    )["memory"]
        .clone();
    assert_eq!(updated["id"], original["id"]);
    assert_eq!(updated["version"], 2);
    assert_eq!(updated["content"], "Rust remains the choice");
    for field in ["kind", "source", "expires_at_ms", "created_at_ms", "scope"] {
        assert_eq!(updated[field], original[field], "update changed {field}");
    }
    assert_business_error(&server.call(
        "update_memory",
        json!({"id":id,"content":"lost update","expected_version":1}),
    ));
    assert_business_error(&server.call("delete_memory", json!({"id":id,"expected_version":1})));
    assert_eq!(
        server.tool("get_memory", json!({"id":id}))["memory"],
        updated
    );
    let history = server.tool("memory_history", json!({"id":id}))["history"].clone();
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert!(history.to_string().contains("prefers Rust"));
    assert!(history.to_string().contains("Rust remains the choice"));
    let plan = server.tool(
        "add_memory",
        json!({"content":"Build a Rust prototype","kind":"plan"}),
    )["memory"]
        .clone();
    assert_eq!(plan["source"], "explicit");
    assert_eq!(
        server.tool("get_profile", json!({"limit":10})),
        json!({"stable":[updated],"dynamic":[plan]})
    );
    assert_eq!(
        server.tool("search_memories", json!({"query":"Rust","limit":10}))["memories"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        server.tool("search_memories", json!({"query":"Rust","max_bytes":2})),
        json!({"memories":[]})
    );
    assert_eq!(
        server.tool("get_profile", json!({"limit":0})),
        json!({"stable":[],"dynamic":[]})
    );
    assert_eq!(
        server.tool("delete_memory", json!({"id":id,"expected_version":2})),
        json!({"deleted":true})
    );
    assert_eq!(
        server.tool("delete_memory", json!({"id":id})),
        json!({"deleted":false})
    );
    assert_business_error(&server.call(
        "update_memory",
        json!({"id":id,"content":"must not resurrect"}),
    ));
    let deleted_history = server.tool("memory_history", json!({"id":id}))["history"].clone();
    assert_eq!(deleted_history.as_array().unwrap().len(), 3);
    for event in history.as_array().unwrap() {
        assert!(deleted_history.as_array().unwrap().contains(event));
    }
    server.close();

    let mut server = Server::spawn(repo.path(), "alice", Some("worker"), Some("one"), Some(&db));
    server.handshake();
    assert_eq!(
        server.tool("get_memory", json!({"id":id})),
        json!({"memory":null})
    );
    assert_eq!(
        server.tool("memory_history", json!({"id":id}))["history"],
        deleted_history
    );
    assert_eq!(
        server.tool("search_memories", json!({"query":"remains"})),
        json!({"memories":[]})
    );
    assert_eq!(
        server.tool("get_profile", json!({})),
        json!({"stable":[],"dynamic":[plan]})
    );
    server.close();
}

#[test]
fn expired_memory_is_hidden_from_current_retrieval_but_audit_is_retained() {
    let repo = tempdir().unwrap();
    let mut server = Server::ready(repo.path());
    let expired = server.tool(
        "add_memory",
        json!({"content":"expired fact","expires_at_ms":0}),
    )["memory"]
        .clone();
    let id = &expired["id"];
    assert_eq!(
        server.tool("get_memory", json!({"id":id})),
        json!({"memory":null})
    );
    assert_eq!(
        server.tool("search_memories", json!({"query":"expired"})),
        json!({"memories":[]})
    );
    assert_eq!(
        server.tool("get_profile", json!({})),
        json!({"stable":[],"dynamic":[]})
    );
    let history = server.tool("memory_history", json!({"id":id}));
    assert_eq!(history["history"].as_array().unwrap().len(), 1);
    assert!(history.to_string().contains("expired fact"));
    assert_eq!(
        server.tool("memory_history", json!({"id":id})),
        history,
        "reads must not change memory audit"
    );
    server.close();
}

#[test]
fn invalid_frames_and_request_ids_recover_without_discarding_stored_memory() {
    let repo = tempdir().unwrap();
    let mut server = Server::ready(repo.path());
    let record =
        server.tool("add_memory", json!({"content":"preserve this memory"}))["memory"].clone();
    for bytes in [
        b"{broken\n".as_slice(),
        b"\xff\xfe\n",
        b"\n",
        b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\",\"params\":{\"x\":NaN}}\n",
    ] {
        server.raw(bytes);
        let response = server.receive();
        assert_error(&response, -32700);
        assert!(response["id"].is_null());
        assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    }
    for message in [
        json!(null),
        json!([]),
        json!([initialize()]),
        json!("scalar"),
        json!({}),
        json!({"jsonrpc":"1.0","id":"valid","method":"ping"}),
        json!({"jsonrpc":"2.0","id":2,"method":7}),
        json!({"jsonrpc":"2.0","id":2,"method":"ping","result":{}}),
        json!({"jsonrpc":"2.0","id":2,"result":{}}),
    ] {
        server.send(&message);
        let response = server.receive();
        assert_error(&response, -32600);
        if message["id"].is_string() || message["id"].is_number() {
            assert_eq!(response["id"], message["id"]);
        }
    }
    for id in [
        json!(null),
        json!(true),
        json!(false),
        json!(1.25),
        json!(1.0),
        json!([]),
        json!({}),
        json!("x".repeat(257)),
    ] {
        server.send(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"delete_memory","arguments":{"id":record["id"]}}}));
        let response = server.receive();
        assert_error(&response, -32600);
        assert!(response["id"].is_null());
    }
    for id in [
        json!(-12),
        json!(0),
        json!(u64::MAX),
        json!("x".repeat(256)),
        json!(""),
    ] {
        server.send(&json!({"jsonrpc":"2.0","id":id,"method":"ping"}));
        assert_eq!(
            server.receive(),
            json!({"jsonrpc":"2.0","id":id,"result":{}})
        );
    }
    assert_eq!(
        server.tool("get_memory", json!({"id":record["id"]}))["memory"],
        record
    );
    server.close();
}

#[test]
fn protocol_errors_are_distinct_from_business_errors_and_connection_remains_usable() {
    let repo = tempdir().unwrap();
    let mut server = Server::ready(repo.path());
    assert_error(&server.rpc("unknown/method", json!({})), -32601);
    for unsupported in [
        "resources/list",
        "prompts/list",
        "logging/setLevel",
        "shutdown",
    ] {
        assert_error(&server.rpc(unsupported, json!({})), -32601);
    }
    assert_error(&server.call("not_a_tool", json!({})), -32602);
    assert_error(
        &server.rpc("tools/list", json!({"cursor":"unsupported"})),
        -32602,
    );
    assert_error(&server.rpc("tools/list", json!({"cursor":5})), -32602);
    assert!(server.rpc("tools/list", json!({"cursor":""}))["result"]["tools"].is_array());
    let record = server.tool("add_memory", json!({"content":"original fact"}))["memory"].clone();
    assert_eq!(record["kind"], "fact");
    assert_eq!(record["source"], "explicit");
    assert!(record["expires_at_ms"].is_null());
    for (name, args) in [
        ("add_memory", json!({"content":1})),
        ("add_memory", json!({"content":"x","kind":"unknown"})),
        ("add_memory", json!({"content":"x","kind":null})),
        ("add_memory", json!({"content":"x","metadata":[]})),
        ("add_memory", json!({"content":"x","metadata":null})),
        ("add_memory", json!({"content":"x","key":false})),
        ("add_memory", json!({"content":"x","source":7})),
        ("add_memory", json!({"content":"x","source":null})),
        (
            "add_memory",
            json!({"content":"x","scope":{"user_id":"other"}}),
        ),
        (
            "update_memory",
            json!({"id":record["id"],"content":"x","expected_version":-1}),
        ),
        (
            "delete_memory",
            json!({"id":record["id"],"expected_version":1.0}),
        ),
        ("memory_history", json!({"id":record["id"],"limit":false})),
        (
            "memory_history",
            json!({"id":record["id"],"max_bytes":null}),
        ),
        ("add_memory", json!({"content":"x","expires_at_ms":-1})),
        ("add_memory", json!({"content":"x","expires_at_ms":1.0})),
        ("update_memory", json!({"id":record["id"],"content":null})),
        ("update_memory", json!({"id":record["id"]})),
        (
            "update_memory",
            json!({"id":record["id"],"content":"x","metadata":false}),
        ),
        ("get_memory", json!({"id":1})),
        ("memory_history", json!({"id":null})),
        ("delete_memory", json!({"id":false})),
        ("search_memories", json!({"query":true})),
        ("search_memories", json!({"query":"x","limit":1.0})),
        ("search_repository", json!({"query":"x","max_bytes":false})),
        ("get_profile", json!({"limit":-1})),
        ("get_profile", json!({"max_bytes":null})),
        ("refresh_repository", json!({"unused":1})),
        ("assemble_context", json!({"query":"x"})),
        ("assemble_context", json!({"query":"x","token_budget":-1})),
        ("assemble_context", json!({"query":"x","token_budget":1.5})),
        ("assemble_context", json!({"query":"x","token_budget":true})),
        (
            "assemble_context",
            json!({"query":null,"token_budget":8000}),
        ),
    ] {
        assert_error(&server.call(name, args), -32602);
    }
    for params in [
        json!({}),
        json!({"name":3}),
        json!({"name":"get_profile","arguments":[]}),
        json!({"name":"get_profile","arguments":null}),
        json!({"name":"get_profile","_meta":false}),
    ] {
        assert_error(&server.rpc("tools/call", params), -32602);
    }
    assert_error(&server.rpc("tools/call", json!([])), -32602);
    for (name, args) in [
        (
            "update_memory",
            json!({"id":"missing","content":"replacement"}),
        ),
        ("add_memory", json!({"content":"   "})),
        ("add_memory", json!({"content":"x".repeat(65537)})),
        ("search_memories", json!({"query":"bad\u{0000}query"})),
        (
            "assemble_context",
            json!({"query":"bad\u{0000}query", "token_budget":8000}),
        ),
    ] {
        assert_business_error(&server.call(name, args));
        assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    }
    assert_business_error(&server.call(
        "update_memory",
        json!({
            "id":record["id"], "content":"conflicting", "expected_version":9
        }),
    ));
    assert_eq!(
        server.tool("get_memory", json!({"id":record["id"]}))["memory"],
        record
    );
    let response = server.rpc("tools/call", json!({"name":"get_memory","arguments":{"id":record["id"]},"_meta":{"progressToken":"read","com.example/annotation":{"a":1}}}));
    assert_eq!(successful_data(&response)["memory"], record);
    server.close();
}

#[test]
fn every_tool_rejects_scope_root_and_future_argument_injection_before_side_effects() {
    let repo = tempdir().unwrap();
    let other_repo = tempdir().unwrap();
    fs::write(
        repo.path().join("current.py"),
        "def current():\n    return 'localneedle'\n",
    )
    .unwrap();
    fs::write(
        other_repo.path().join("foreign.py"),
        "def foreign():\n    return 'foreignneedle'\n",
    )
    .unwrap();
    let mut server = Server::ready(repo.path());
    let record = server.tool("add_memory", json!({"content":"local memory"}))["memory"].clone();
    for (name, args) in [
        ("add_memory", json!({"content":"injected memory"})),
        ("search_memories", json!({"query":"memory"})),
        ("index_memories", json!({})),
        (
            "search_memories",
            json!({"query":"memory", "mode":"hybrid"}),
        ),
        ("get_memory", json!({"id":record["id"]})),
        (
            "update_memory",
            json!({"id":record["id"],"content":"injected overwrite"}),
        ),
        ("delete_memory", json!({"id":record["id"]})),
        ("memory_history", json!({"id":record["id"]})),
        ("get_profile", json!({})),
        ("refresh_repository", json!({})),
        ("search_repository", json!({"query":"foreignneedle"})),
        (
            "assemble_context",
            json!({"query":"memory", "token_budget":8000}),
        ),
    ] {
        for field in [
            "scope",
            "user",
            "agent",
            "project",
            "user_id",
            "agent_id",
            "project_id",
            "userId",
            "agentId",
            "projectId",
            "future_scope",
            "root",
            "path",
            "file_path",
            "_meta",
        ] {
            let mut injected = args.clone();
            injected[field] = json!(other_repo.path().to_str().unwrap());
            assert_error(&server.call(name, injected), -32602);
        }
    }
    assert_eq!(
        server.tool("get_memory", json!({"id":record["id"]}))["memory"],
        record
    );
    let history = server.tool("memory_history", json!({"id":record["id"]}));
    assert_eq!(history["history"].as_array().unwrap().len(), 1);
    assert!(history.to_string().contains("local memory"));
    assert_eq!(
        server.tool("search_memories", json!({"query":"injected"})),
        json!({"memories":[]})
    );
    assert_eq!(server.tool("refresh_repository", json!({}))["indexed"], 1); // Bad searches/refreshes did not write cache.
    assert_eq!(
        server.tool("search_repository", json!({"query":"foreignneedle"}))["hits"],
        json!([])
    );
    server.close();
}

#[test]
fn notifications_never_reply_or_mutate_memory_or_repository_cache() {
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("source.py"), "needle = 1\n").unwrap();
    let mut server = Server::spawn(repo.path(), "alice", None, None, None);
    let mut init_notification = initialize();
    init_notification.as_object_mut().unwrap().remove("id");
    server.send(&init_notification);
    server.send(&json!({"jsonrpc":"2.0","method":"unknown/notification"}));
    assert_error(&server.rpc("tools/list", json!({})), -32600);
    server.handshake();
    let record =
        server.tool("add_memory", json!({"content":"persistent memory"}))["memory"].clone();
    for (name, args) in [
        ("add_memory", json!({"content":"notification memory"})),
        (
            "update_memory",
            json!({"id":record["id"],"content":"notification overwrite"}),
        ),
        ("delete_memory", json!({"id":record["id"]})),
        ("refresh_repository", json!({})),
        ("search_repository", json!({"query":"needle"})),
        (
            "assemble_context",
            json!({"query":"needle", "token_budget":8000}),
        ),
    ] {
        server.send(
            &json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":name,"arguments":args}}),
        );
    }
    server.send(&json!({"jsonrpc":"2.0","method":"tools/list"}));
    server.send(&json!({"jsonrpc":"2.0","method":"tools/call","params":false}));
    server.send(&json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"delete_memory","arguments":{"id":record["id"]}},"result":{}}));
    server.send(&json!({"jsonrpc":"2.0","method":"ping"}));
    server.send(&json!({"jsonrpc":"2.0","method":"unknown/notification","params":null}));
    server.send(&json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1,"reason":"stop"}}));
    assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    assert_eq!(
        server.tool("get_memory", json!({"id":record["id"]}))["memory"],
        record
    );
    assert_eq!(
        server.tool("get_profile", json!({})),
        json!({"stable":[record],"dynamic":[]})
    );
    assert_eq!(server.tool("refresh_repository", json!({}))["indexed"], 1);
    server.close();
}

#[test]
fn oversized_frames_are_drained_and_boundary_frames_and_eof_still_work() {
    let repo = tempdir().unwrap();
    let mut server = Server::ready(repo.path());
    let chunk = vec![b'x'; 64 * 1024];
    for _ in 0..128 {
        server.raw(&chunk);
    } // Eight MiB without a newline.
    server.raw(b"\n");
    assert_error(&server.receive(), -32600);
    assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    let mut boundary =
        json!({"jsonrpc":"2.0","id":"boundary","method":"ping","params":{"_meta":{"padding":""}}});
    let overhead = serde_json::to_vec(&boundary).unwrap().len();
    boundary["params"]["_meta"]["padding"] = json!("x".repeat(1024 * 1024 - overhead));
    assert_eq!(serde_json::to_vec(&boundary).unwrap().len(), 1024 * 1024);
    server.send(&boundary);
    assert_eq!(
        server.receive(),
        json!({"jsonrpc":"2.0","id":"boundary","result":{}})
    );
    let frame = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":"eof","method":"ping"})).unwrap();
    server.raw(&frame);
    drop(server.stdin.take());
    assert_eq!(
        server.receive(),
        json!({"jsonrpc":"2.0","id":"eof","result":{}})
    );
    server.close();
}

#[test]
fn history_search_and_profile_honor_count_and_serialized_byte_budgets() {
    let repo = tempdir().unwrap();
    let mut server = Server::ready(repo.path());
    let record = server.tool("add_memory", json!({"content":"initial history"}))["memory"].clone();
    let id = &record["id"];
    let padding = "\\".repeat(60 * 1024);
    for version in 2..=24 {
        let record = server.tool(
            "update_memory",
            json!({"id":id,"content":format!("{version} {padding}")}),
        )["memory"]
            .clone();
        assert_eq!(record["version"], version);
    }
    let history = server.tool(
        "memory_history",
        json!({"id":id,"limit":100,"max_bytes":1024 * 1024}),
    )["history"]
        .clone();
    assert!(serde_json::to_vec(&history).unwrap().len() <= 1024 * 1024);
    assert!(!history.as_array().unwrap().is_empty());
    assert!(history.as_array().unwrap().len() < 24);
    assert!(
        server.tool("memory_history", json!({"id":id}))["history"]
            .as_array()
            .unwrap()
            .len()
            <= 5
    );
    assert_eq!(
        server.tool("memory_history", json!({"id":id,"limit":0})),
        json!({"history":[]})
    );
    assert_eq!(
        server.tool("memory_history", json!({"id":id,"max_bytes":2})),
        json!({"history":[]})
    );

    for number in 0..7 {
        server.tool(
            "add_memory",
            json!({"content":format!("budgetneedle {number} context")}),
        );
    }
    let defaults = server.tool("search_memories", json!({"query":"budgetneedle"}));
    assert_eq!(defaults["memories"].as_array().unwrap().len(), 5);
    let limited = server.tool("search_memories", json!({"query":"budgetneedle","limit":2}));
    assert_eq!(limited["memories"].as_array().unwrap().len(), 2);
    let bounded = server.tool(
        "search_memories",
        json!({"query":"budgetneedle","limit":100,"max_bytes":1000}),
    );
    assert!(serde_json::to_vec(&bounded["memories"]).unwrap().len() <= 1000);
    let profile = server.tool("get_profile", json!({"limit":100,"max_bytes":1000}));
    assert!(serde_json::to_vec(&profile).unwrap().len() <= 1000);
    assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    server.close();
}

#[test]
fn repository_search_refreshes_edits_deletions_and_ignores_without_cross_root_leaks() {
    let repo = tempdir().unwrap();
    let other = tempdir().unwrap();
    let dbdir = tempdir().unwrap();
    let db = dbdir.path().join("shared.sqlite");
    let file = repo.path().join("feature.py");
    fs::write(&file, "def oldneedle():\n    return 1\n").unwrap();
    fs::write(
        other.path().join("foreign.py"),
        "def oldneedle():\n    return 999\n",
    )
    .unwrap();
    let mut foreign = Server::spawn(other.path(), "alice", None, None, Some(&db));
    foreign.handshake();
    foreign.tool("refresh_repository", json!({}));
    foreign.close();
    let alias = repo.path().join(".");
    let mut server = Server::spawn(&alias, "alice", None, None, Some(&db));
    server.handshake();
    let first = server.tool("search_repository", json!({"query":"oldneedle"}));
    assert_eq!(
        first["refresh"]["repository"],
        repo.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(first["refresh"]["indexed"], 1);
    assert_eq!(first["hits"].as_array().unwrap().len(), 1);
    assert_eq!(first["hits"][0]["path"], "feature.py");
    assert_eq!(first["hits"][0]["symbols"][0]["name"], "oldneedle");
    let old_hash = first["hits"][0]["content_hash"].clone();
    fs::write(&file, "def newneedle():\n    return 2\n").unwrap();
    let old = server.tool("search_repository", json!({"query":"oldneedle"}));
    assert_eq!(old["refresh"]["indexed"], 1);
    assert_eq!(old["hits"], json!([]));
    let current = server.tool("search_repository", json!({"query":"newneedle"}));
    assert_eq!(current["refresh"]["unchanged"], 1);
    assert_eq!(current["hits"][0]["symbols"][0]["name"], "newneedle");
    assert_ne!(current["hits"][0]["content_hash"], old_hash);
    fs::remove_file(&file).unwrap();
    let deleted = server.tool("search_repository", json!({"query":"newneedle"}));
    assert_eq!(deleted["refresh"]["removed"], 1);
    assert_eq!(deleted["hits"], json!([]));
    fs::write(&file, "newneedle = 3\n").unwrap();
    server.tool("search_repository", json!({"query":"newneedle"}));
    fs::write(repo.path().join(".tinymemignore"), "feature.py\n").unwrap();
    let ignored = server.tool("search_repository", json!({"query":"newneedle"}));
    assert_eq!(ignored["refresh"]["removed"], 1);
    assert_eq!(ignored["hits"], json!([]));
    fs::write(repo.path().join(".tinymemignore"), b"\xff").unwrap();
    assert_business_error(&server.call("search_repository", json!({"query":"newneedle"})));
    assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    fs::remove_file(repo.path().join(".tinymemignore")).unwrap();
    assert_eq!(
        server.tool("search_repository", json!({"query":"newneedle"}))["hits"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    server.close();
}

#[test]
fn repository_search_honors_budgets_refreshes_even_when_empty_and_accepts_global_db_after_mcp() {
    let directory = tempdir().unwrap();
    let root = directory.path().join("repository with spaces 東京");
    fs::create_dir(&root).unwrap();
    for number in 0..7 {
        fs::write(
            root.join(format!("source{number}.txt")),
            format!("budgetneedle {number}\n"),
        )
        .unwrap();
    }
    let db = directory.path().join("database with spaces/store.sqlite");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tinymem"));
    command
        .arg("mcp")
        .arg(&root)
        .args(["--user", "alice", "--db"])
        .arg(&db);
    let mut server = Server::from_command(command);
    server.handshake();
    let memory = server.tool(
        "add_memory",
        json!({"content":"repository reads must not mutate memory"}),
    )["memory"]
        .clone();
    let history = server.tool("memory_history", json!({"id":memory["id"]}));
    let defaults = server.tool("search_repository", json!({"query":"budgetneedle"}));
    assert_eq!(defaults["refresh"]["indexed"], 7);
    assert_eq!(defaults["hits"].as_array().unwrap().len(), 5);
    let limited = server.tool(
        "search_repository",
        json!({"query":"budgetneedle","limit":2}),
    );
    assert_eq!(limited["hits"].as_array().unwrap().len(), 2);
    let bounded = server.tool(
        "search_repository",
        json!({"query":"budgetneedle","limit":100,"max_bytes":1000}),
    );
    assert!(serde_json::to_vec(&bounded["hits"]).unwrap().len() <= 1000);
    for (option, filename) in [
        ("limit", "fresh-limit.txt"),
        ("max_bytes", "fresh-bytes.txt"),
    ] {
        fs::write(root.join(filename), "budgetneedle fresh\n").unwrap();
        let mut args = json!({"query":"budgetneedle"});
        args[option] = json!(0);
        let result = server.tool("search_repository", args);
        assert_eq!(result["refresh"]["indexed"], 1);
        assert_eq!(result["hits"], json!([]));
    }
    assert_eq!(
        server.tool("get_memory", json!({"id":memory["id"]}))["memory"],
        memory
    );
    assert_eq!(
        server.tool("memory_history", json!({"id":memory["id"]})),
        history
    );
    server.close();
    assert!(db.is_file());
    assert!(!root.join(".tinymem").exists());
}

#[test]
fn startup_requires_root_and_user_and_rejects_invalid_scope_without_stdout() {
    let repo = tempdir().unwrap();
    for args in [vec!["mcp"], vec!["mcp", "--user", "alice"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_tinymem"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let missing_user = Command::new(env!("CARGO_BIN_EXE_tinymem"))
        .arg("mcp")
        .arg(repo.path())
        .output()
        .unwrap();
    assert_eq!(missing_user.status.code(), Some(2));
    assert!(missing_user.stdout.is_empty());
    for (user, agent, project) in [
        ("   ", None, None),
        ("alice", Some(""), None),
        ("alice", None, Some("  ")),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tinymem"));
        command.arg("mcp").arg(repo.path()).args(["--user", user]);
        if let Some(agent) = agent {
            command.args(["--agent", agent]);
        }
        if let Some(project) = project {
            command.args(["--project", project]);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(
            output.stdout.is_empty(),
            "must validate startup scope before responding"
        );
        assert!(!output.stderr.is_empty());
    }
    let mut server = Server::ready(repo.path());
    assert_eq!(
        server.tool("refresh_repository", json!({}))["repository"],
        repo.path().canonicalize().unwrap().to_str().unwrap()
    );
    server.close();
    assert!(repo.path().join(".tinymem/repository.db").is_file());
}

#[test]
fn tool_bursts_are_rate_limited_but_ping_is_unaffected() {
    let repo = tempdir().unwrap();
    let mut server = Server::ready(repo.path());
    for id in 1000..3048 {
        server.send(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"get_profile","arguments":{"limit":0}}}));
    }
    let mut limited = 0;
    for id in 1000..3048 {
        let response = server.receive();
        assert_eq!(response["id"], id);
        if response["result"]["isError"] == true {
            assert_business_error(&response);
            assert!(
                response["result"]["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("rate limit")
            );
            limited += 1;
        } else {
            assert_eq!(
                successful_data(&response),
                json!({"stable":[],"dynamic":[]})
            );
        }
    }
    assert!(
        limited > 0,
        "a rapid burst must trigger local safety limits"
    );
    assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    // Real time refill, without depending on a private limiter or clock mock.
    thread::sleep(Duration::from_millis(20));
    assert_eq!(
        server.tool("get_profile", json!({"limit":0})),
        json!({"stable":[],"dynamic":[]})
    );
    server.close();
}

#[test]
fn semantic_arguments_are_strict_and_missing_provider_is_not_a_silent_lexical_fallback() {
    let repo = tempdir().unwrap();
    let mut server = Server::ready(repo.path());
    server.tool("add_memory", json!({"content":"espresso"}));
    for mode in ["semantic", "hybrid"] {
        assert_business_error(
            &server.call("search_memories", json!({"query":"espresso","mode":mode})),
        );
    }
    assert_business_error(&server.call("index_memories", json!({})));
    for args in [
        json!({"query":"espresso","mode":"unknown"}),
        json!({"query":"espresso","mode":null}),
        json!({"query":"espresso","min_similarity":"0.5"}),
    ] {
        assert_error(&server.call("search_memories", args), -32602);
    }
    assert_business_error(&server.call(
        "search_memories",
        json!({"query":"espresso","min_similarity":2}),
    ));
    assert_eq!(
        server.tool("search_memories", json!({"query":"espresso"}))["memories"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    server.close();
}

#[test]
#[cfg(unix)]
fn actual_embedding_command_backfill_and_hybrid_retrieval_work_over_stdio() {
    use std::os::unix::fs::PermissionsExt;
    let repo = tempdir().unwrap();
    let db = repo.path().join("native.db");
    let mut lexical = Server::spawn(repo.path(), "alice", None, None, Some(&db));
    lexical.handshake();
    let original = lexical.tool("add_memory", json!({"content":"morning drink"}))["memory"].clone();
    lexical.close();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fixtures/embed.py");
    fs::set_permissions(&fixture, fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tinymem"));
    command
        .arg("--db")
        .arg(&db)
        .arg("mcp")
        .arg(repo.path())
        .args(["--user", "alice", "--embedding-command"])
        .arg(fixture)
        .args([
            "--embedding-model",
            "fixture-v1",
            "--embedding-dimensions",
            "3",
        ]);
    let mut server = Server::from_command(command);
    server.handshake();
    assert_eq!(
        server.tool("index_memories", json!({"limit":1}))["indexed"],
        1
    );
    let semantic = server.tool(
        "search_memories",
        json!({"query":"hot beverage","mode":"semantic"}),
    );
    assert_eq!(semantic["memories"], json!([original]));
    let exact = server.tool("add_memory", json!({"content":"espresso"}))["memory"].clone();
    server.tool("add_memory", json!({"content":"compiler"}));
    let hybrid = server.tool(
        "search_memories",
        json!({"query":"espresso compiler","mode":"hybrid"}),
    );
    assert_eq!(hybrid["memories"].as_array().unwrap().len(), 3);
    assert_eq!(hybrid["memories"][0], exact);
    assert_eq!(
        server.tool("memory_history", json!({"id":original["id"]}))["history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    server.close();
}

#[test]
fn assembly_accepts_long_tasks_but_preserves_standalone_search_limits() {
    let repo = tempdir().unwrap();
    let mut server = Server::ready(repo.path());
    server.tool("add_memory", json!({"content":"auth evidence"}));
    let compact = server.tool(
        "assemble_context",
        json!({"query":"auth", "token_budget":8000}),
    );
    let task = format!("{}auth", "the ".repeat(16_383));
    assert_eq!(task.len(), 65_536);
    let long = server.tool(
        "assemble_context",
        json!({"query":task, "token_budget":8000}),
    );
    assert_eq!(long, compact);
    assert_business_error(&server.call(
        "assemble_context",
        json!({"query":format!("{task} "), "token_budget":0}),
    ));
    assert_business_error(&server.call("search_memories", json!({"query":task})));
    assert_business_error(&server.call("search_memories", json!({"query":"auth ".repeat(33)})));
    assert_eq!(server.rpc("ping", json!({}))["result"], json!({}));
    server.close();
}

#[test]
fn assembly_packs_prompt_and_provenance_in_fixed_scope_and_refreshes_without_memory_writes() {
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("auth.py"), "auth = True\n").unwrap();
    let mut server = Server::ready(repo.path());
    let original = server.tool(
        "add_memory",
        json!({"content":"Prefer auth tests", "kind":"preference", "source":"user"}),
    )["memory"]
        .clone();
    server.tool(
        "add_memory",
        json!({"content":"auth uses cookies", "kind":"decision", "source":"ADR-1"}),
    );
    server.tool(
        "add_memory",
        json!({"content":"auth expired", "expires_at_ms":0}),
    );
    let history = server.tool("memory_history", json!({"id":original["id"]}));
    let result = server.tool(
        "assemble_context",
        json!({"query":"Please update the auth flow", "token_budget":8000}),
    );
    let prompt = result["prompt"].as_str().unwrap();
    assert_eq!(result["token_count_method"], "utf8_bytes_upper_bound");
    assert_eq!(
        result["tokens_used"].as_u64().unwrap() as usize,
        prompt.len()
    );
    assert!(prompt.len() <= 8000);
    assert_eq!(result["entries"].as_array().unwrap().len(), 3);
    let entries: Value = serde_json::from_str(prompt.split_once('\n').unwrap().1).unwrap();
    assert_eq!(entries, result["entries"]);
    assert_eq!(result["entries"][0]["provenance"][0]["id"], original["id"]);
    assert_eq!(result["entries"][2]["provenance"][0]["path"], "auth.py");
    assert!(!prompt.contains("expired"));
    let exact = server.tool(
        "assemble_context",
        json!({"query":"auth", "token_budget":prompt.len()}),
    );
    assert_eq!(exact["prompt"], result["prompt"]);
    fs::write(repo.path().join("auth.py"), "auth = False\n").unwrap();
    let empty = server.tool(
        "assemble_context",
        json!({"query":"auth", "token_budget":0}),
    );
    assert_eq!(empty["prompt"], "");
    assert_eq!(empty["entries"], json!([]));
    assert_eq!(empty["tokens_used"], 0);
    assert_eq!(
        server.tool("search_repository", json!({"query":"auth"}))["refresh"]["unchanged"],
        1
    );
    assert_eq!(
        server.tool("memory_history", json!({"id":original["id"]})),
        history
    );
    assert_eq!(
        server.tool("get_memory", json!({"id":original["id"]}))["memory"],
        original
    );
    server.close();
}

#[test]
fn assembly_returns_only_budgeted_prompt_as_text_and_keeps_full_structured_content() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join("private")).unwrap();
    fs::write(
        root.join("private/auth.rs"),
        "auth needle private repository evidence\\n",
    )
    .unwrap();
    fs::write(
        root.join("public.txt"),
        "auth needle public repository evidence\\n",
    )
    .unwrap();
    let mut server = Server::ready(root);
    let saved = server.tool(
        "add_memory",
        json!({"content":"auth needle personal decision", "kind":"decision", "source":"test"}),
    )["memory"]
        .clone();

    let memory_only = server.call(
        "assemble_context",
        json!({
            "query":"auth needle",
            "token_budget":4000,
            "sources":["memory"],
            "exclude_paths":["private/**"]
        }),
    );
    let result = &memory_only["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["content"][0]["text"],
        result["structuredContent"]["prompt"]
    );
    let prompt = result["content"][0]["text"].as_str().unwrap();
    assert!(prompt.starts_with("Task context:"));
    assert!(
        serde_json::from_str::<Value>(prompt).is_err(),
        "do not duplicate the structured envelope into model text"
    );
    assert!(!prompt.contains("public repository evidence"));
    assert!(!prompt.contains("private repository evidence"));
    let structured = &result["structuredContent"];
    assert_eq!(structured["entries"].as_array().unwrap().len(), 1);
    assert_eq!(structured["entries"][0]["provenance"][0]["id"], saved["id"]);
    assert_eq!(
        structured["tokens_used"].as_u64().unwrap() as usize,
        prompt.len()
    );
    assert!(prompt.len() <= 4000);
    assert_eq!(structured["token_count_method"], "utf8_bytes_upper_bound");

    let both_sources = server.call(
        "assemble_context",
        json!({
            "query":"auth needle",
            "token_budget":4000,
            "exclude_paths":["private/**"]
        }),
    );
    let text = both_sources["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let structured = &both_sources["result"]["structuredContent"];
    assert_eq!(text, structured["prompt"]);
    assert!(
        structured["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry["category"] == "repository" && entry["provenance"][0]["path"] == "public.txt"
            })
    );
    assert!(
        structured["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| { entry["provenance"][0]["path"] != "private/auth.rs" })
    );

    // Automatic assembly options do not constrain the explicit repository search tool.
    let explicit = successful_data(&server.call("search_repository", json!({"query":"auth"})));
    assert!(
        explicit["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["path"] == "private/auth.rs")
    );

    let empty = server.call(
        "assemble_context",
        json!({"query":"auth", "token_budget":0, "sources":[]}),
    );
    assert_eq!(empty["result"]["content"][0]["text"], "");
    assert_eq!(empty["result"]["structuredContent"]["prompt"], "");
    assert_eq!(empty["result"]["structuredContent"]["entries"], json!([]));
    assert_eq!(empty["result"]["structuredContent"]["tokens_used"], 0);

    let tools = server.rpc("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .clone();
    let search = tools
        .iter()
        .find(|tool| tool["name"] == "search_memories")
        .unwrap();
    let repository = tools
        .iter()
        .find(|tool| tool["name"] == "search_repository")
        .unwrap();
    let assembly = tools
        .iter()
        .find(|tool| tool["name"] == "assemble_context")
        .unwrap();
    for tool in [search, repository] {
        let description = tool["description"].as_str().unwrap().to_lowercase();
        assert!(description.contains("literal and"));
        assert!(description.contains("1-3 distinctive terms"));
    }
    assert!(
        assembly["description"]
            .as_str()
            .unwrap()
            .contains("Skip redundant recall")
    );
    server.close();
}

#[test]
fn malformed_assembly_filters_are_rejected_and_filter_arguments_are_optional() {
    let directory = tempdir().unwrap();
    let mut server = Server::ready(directory.path());
    let assembly = server.rpc("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "assemble_context")
        .unwrap()
        .clone();
    let schema = &assembly["inputSchema"];
    assert!(schema["properties"].get("sources").is_some());
    assert!(schema["properties"].get("exclude_paths").is_some());
    assert_eq!(schema["required"], json!(["query", "token_budget"]));

    let invalid_source = server.call(
        "assemble_context",
        json!({"query":"x", "token_budget":100, "sources":["other"]}),
    );
    assert_eq!(invalid_source["error"]["code"], -32602);
    for args in [
        json!({"query":"x", "token_budget":100, "sources":["memory", "memory"]}),
        json!({"query":"x", "token_budget":100, "exclude_paths":["../private"]}),
        json!({"query":"x", "token_budget":100, "exclude_paths":["[broken"]}),
    ] {
        let response = server.call("assemble_context", args);
        assert_eq!(response["result"]["isError"], true, "{response}");
    }
    server.close();
}
