use serde_json::Value;
use std::ffi::OsStr;
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

fn cli(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tinymem"))
        .args(args)
        .output()
        .expect("run compiled tinymem binary")
}

fn json(output: Output) -> Value {
    assert!(
        output.status.success(),
        "exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "successful command wrote stderr");
    serde_json::from_slice(&output.stdout).expect("stdout must be one JSON value")
}

#[test]
fn index_then_search_refreshes_edited_and_deleted_source() {
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("auth.py"),
        "def obsolete_login():\n    pass\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("stable.py"),
        "def stable_helper():\n    pass\n",
    )
    .unwrap();
    let root = repo.path().as_os_str();
    let repository = repo.path().canonicalize().unwrap();

    let report = json(cli(&[OsStr::new("index"), root]));
    assert_eq!(report["repository"], repository.to_str().unwrap());
    assert_eq!(report["indexed"], 2);
    assert_eq!(report["unchanged"], 0);
    assert_eq!(report["removed"], 0);
    assert_eq!(report["skipped"], 0);
    assert!(report["indexed_at_ms"].as_u64().unwrap() > 0);
    assert!(repo.path().join(".tinymem/repository.db").is_file());

    fs::write(
        repo.path().join("auth.py"),
        "def current_login():\n    return True\n",
    )
    .unwrap();
    fs::remove_file(repo.path().join("stable.py")).unwrap();
    let result = json(cli(&[
        OsStr::new("search"),
        root,
        OsStr::new("current_login"),
    ]));
    assert_eq!(result["repository"], report["repository"]);
    assert_eq!(result["refresh"]["repository"], report["repository"]);
    assert_eq!(result["refresh"]["indexed"], 1);
    assert_eq!(result["refresh"]["removed"], 1);
    let hits = result["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["path"], "auth.py");
    assert!(
        hits[0]["content"]
            .as_str()
            .unwrap()
            .contains("current_login")
    );

    let stale = json(cli(&[
        OsStr::new("search"),
        root,
        OsStr::new("obsolete_login"),
    ]));
    assert!(stale["hits"].as_array().unwrap().is_empty());
    assert_eq!(stale["refresh"]["indexed"], 0);
    assert_eq!(stale["refresh"]["unchanged"], 1);
}

#[test]
fn explicit_external_database_is_global_and_creates_its_parent() {
    let repo = tempdir().unwrap();
    let external = tempdir().unwrap();
    let db = external.path().join("nested/repository.db");
    fs::write(
        repo.path().join("auth.py"),
        "def authenticate():\n    pass\n",
    )
    .unwrap();
    let report = json(cli(&[
        OsStr::new("index"),
        repo.path().as_os_str(),
        OsStr::new("--db"),
        db.as_os_str(),
    ]));
    assert_eq!(report["indexed"], 1);
    assert!(db.is_file());
    assert!(!repo.path().join(".tinymem").exists());
    let result = json(cli(&[
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("search"),
        repo.path().as_os_str(),
        OsStr::new("authenticate"),
    ]));
    assert_eq!(result["refresh"]["unchanged"], 1);
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
}

#[test]
fn invalid_roots_fail_on_stderr_without_creating_a_database() {
    let directory = tempdir().unwrap();
    let missing = directory.path().join("missing");
    let file = directory.path().join("file.py");
    let db = directory.path().join("external/repository.db");
    fs::write(&file, "pass\n").unwrap();
    for root in [&missing, &file] {
        for subcommand in ["index", "search", "watch"] {
            let mut args = vec![
                OsStr::new("--db"),
                db.as_os_str(),
                OsStr::new(subcommand),
                root.as_os_str(),
            ];
            if subcommand == "search" {
                args.push(OsStr::new("pass"));
            } else if subcommand == "watch" {
                args.push(OsStr::new("--once"));
            }
            let output = cli(&args);
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains("repository root"));
        }
    }
    assert!(!missing.exists());
    assert!(!db.parent().unwrap().exists());
}

#[test]
fn search_honors_limit_and_byte_budget_but_always_refreshes() {
    let repo = tempdir().unwrap();
    for number in 0..7 {
        fs::write(
            repo.path().join(format!("source{number}.txt")),
            format!("needle {}\n", "context ".repeat(40)),
        )
        .unwrap();
    }
    let root = repo.path().as_os_str();
    let defaults = json(cli(&[OsStr::new("search"), root, OsStr::new("needle")]));
    assert_eq!(defaults["hits"].as_array().unwrap().len(), 5);
    let limited = json(cli(&[
        OsStr::new("search"),
        root,
        OsStr::new("needle"),
        OsStr::new("--limit"),
        OsStr::new("2"),
        OsStr::new("--max-bytes"),
        OsStr::new("16000"),
    ]));
    assert_eq!(limited["hits"].as_array().unwrap().len(), 2);
    let bounded = json(cli(&[
        OsStr::new("search"),
        root,
        OsStr::new("needle"),
        OsStr::new("--limit"),
        OsStr::new("7"),
        OsStr::new("--max-bytes"),
        OsStr::new("1000"),
    ]));
    let content_bytes: usize = bounded["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["content"].as_str().unwrap().len())
        .sum();
    assert!(content_bytes <= 1000);
    for (option, filename) in [
        ("--limit", "newlimit.txt"),
        ("--max-bytes", "newbudget.txt"),
    ] {
        fs::write(repo.path().join(filename), "needle freshly edited\n").unwrap();
        let empty = json(cli(&[
            OsStr::new("search"),
            root,
            OsStr::new("needle"),
            OsStr::new(option),
            OsStr::new("0"),
        ]));
        assert!(empty["hits"].as_array().unwrap().is_empty());
        assert_eq!(empty["refresh"]["indexed"], 1);
    }
}

#[test]
fn watch_once_emits_one_report_and_exits() {
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("auth.py"),
        "def authenticate():\n    pass\n",
    )
    .unwrap();
    let args = [
        OsStr::new("watch"),
        repo.path().as_os_str(),
        OsStr::new("--once"),
        OsStr::new("--interval-ms"),
        OsStr::new("1"),
    ];
    let report = json(cli(&args));
    assert_eq!(report["indexed"], 1);
    assert_eq!(
        report["repository"],
        repo.path().canonicalize().unwrap().to_str().unwrap()
    );
    let second = json(cli(&args));
    assert_eq!(second["indexed"], 0);
    assert_eq!(second["unchanged"], 1);
}

#[test]
fn continuous_watch_retries_failed_refresh_and_observes_edits_and_deletions() {
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    // Own the foreground child even if an assertion fails; never leave a watcher running.
    struct OwnedWatcher(Child);
    impl Drop for OwnedWatcher {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let repo = tempdir().unwrap();
    let source = repo.path().join("auth.py");
    fs::write(&source, "def old_login(): pass\n").unwrap();
    let mut watcher = OwnedWatcher(
        Command::new(env!("CARGO_BIN_EXE_tinymem"))
            .args([
                OsStr::new("watch"),
                repo.path().as_os_str(),
                OsStr::new("--interval-ms"),
                OsStr::new("100"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let (reports, output) = mpsc::channel();
    let stdout = watcher.0.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if reports.send(line).is_err() {
                break;
            }
        }
    });
    let (errors, diagnostics) = mpsc::channel();
    let stderr = watcher.0.stderr.take().unwrap();
    let error_reader = thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            if errors.send(line).is_err() {
                break;
            }
        }
    });
    let receive = || -> Value {
        serde_json::from_str(
            &output
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(receive()["indexed"], 1);
    fs::write(repo.path().join(".tinymemignore"), "[z-a]\n").unwrap();
    let error = diagnostics
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert!(error.contains("refresh failed"));
    assert!(error.contains("retrying"));
    fs::write(&source, "def current_login(): pass\n").unwrap();
    fs::remove_file(repo.path().join(".tinymemignore")).unwrap();
    let mut saw_edit = false;
    for _ in 0..20 {
        if receive()["indexed"] == 1 {
            saw_edit = true;
            break;
        }
    }
    assert!(saw_edit, "watch never indexed the edit after recovery");
    let result = json(cli(&[
        OsStr::new("search"),
        repo.path().as_os_str(),
        OsStr::new("current_login"),
    ]));
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
    fs::remove_file(&source).unwrap();
    let mut saw_deletion = false;
    for _ in 0..20 {
        if receive()["removed"] == 1 {
            saw_deletion = true;
            break;
        }
    }
    assert!(saw_deletion, "watch never removed deleted source");
    drop(watcher);
    reader.join().unwrap();
    error_reader.join().unwrap();
}
