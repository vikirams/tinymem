#![cfg(unix)]
//! Actual shell-free process adapter, including blocked writes and bounded output.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use tinymem_engine::{CommandEmbedder, Embedder};

#[test]
fn command_adapter_uses_literal_paths_and_validates_output_with_finite_cleanup() {
    let directory = tempdir().unwrap();
    let executable = directory.path().join("embed ; literal path");
    let success = "#!/bin/sh\nIFS= read -r request || true\ncase \"$request\" in *'漢字'* ) printf '{\"embedding\":[3,4,0]}' ;; *) exit 4 ;; esac\n";
    fs::write(&executable, success).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let embedder = CommandEmbedder::new(&executable, "fixture", 3, Duration::from_secs(2)).unwrap();
    let vector = embedder.embed("漢字\nquoted \"text\"").unwrap();
    assert!((vector[0] - 0.6).abs() < 1e-6);
    assert!((vector[1] - 0.8).abs() < 1e-6);
    for body in [
        "printf 'not JSON'",
        "printf '{\"embedding\":[1,2]}'",
        "printf '{\"embedding\":[0,0,0]}'",
        "printf '{\"embedding\":[1e999,0,0]}'",
        "printf '{\"embedding\":[1,0,0],\"other\":true}'",
        "printf '{\"embedding\":[1,0,0]}'; exit 3",
        // exec ensures the timed-out owned process, not a descendant, sleeps.
        "exec sleep 10",
    ] {
        let read = if body.starts_with("exec sleep") {
            ""
        } else {
            "IFS= read -r request || true\n"
        };
        fs::write(&executable, format!("#!/bin/sh\n{read}{body}\n")).unwrap();
        let embedder =
            CommandEmbedder::new(&executable, "fixture", 3, Duration::from_millis(200)).unwrap();
        let started = Instant::now();
        assert!(
            embedder.embed(&"x".repeat(65536)).is_err(),
            "accepted {body}"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    for (bytes, redirect) in [(140000, ""), (18000, " >&2")] {
        let body = format!(
            "#!/bin/sh\nIFS= read -r request || true\nprintf '%s' '{}'{}\n",
            "x".repeat(bytes),
            redirect
        );
        fs::write(&executable, body).unwrap();
        let embedder =
            CommandEmbedder::new(&executable, "fixture", 3, Duration::from_secs(2)).unwrap();
        let error = embedder.embed("test").unwrap_err();
        assert!(
            error.to_string().contains("stream failed"),
            "overflow did not hit stream cap: {error:#}"
        );
    }
    assert!(CommandEmbedder::new(&executable, "fixture", 3, Duration::ZERO).is_err());
    assert!(CommandEmbedder::new(&executable, "fixture", 3, Duration::from_secs(121)).is_err());
    assert!(CommandEmbedder::new(&executable, "", 3, Duration::from_secs(1)).is_err());
}
