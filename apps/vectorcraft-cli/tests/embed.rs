//! `vectorcraft-cli embed`: the app with no window, driven over the control channel.
// Integration tests: unwrapping and panicking on failure is fine here, unlike in shipped code (AGENTS.md › Robustness).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_vectorcraft-cli");

#[test]
fn embed_serves_a_windowless_session_over_the_control_channel() {
    let dir = std::env::temp_dir().join(format!("vectorcraft-cli-embed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(BIN).args(["embed", "--control", &port.to_string(), "--size", "900x600"]).stderr(Stdio::null()).spawn().unwrap();
    let start = Instant::now();
    let stream = loop {
        match std::net::TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => break s,
            Err(_) if start.elapsed() < Duration::from_secs(60) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => panic!("embed never listened: {e}"),
        }
    };
    let mut out = stream.try_clone().unwrap();
    let mut lines = BufReader::new(stream).lines();
    let mut send = |line: String| -> Value {
        writeln!(out, "{line}").unwrap();
        serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap()
    };
    let mut call = |method: &str, params: Value| send(json!({"id": 3, "method": method, "params": params}).to_string());
    assert_eq!(call("engine.execute", json!({"command": "file.new", "params": {}}))["ok"], true);
    let r = call("engine.execute", json!({"command": "shape.rectangle", "params": {"x": 10, "y": 10, "width": 100, "height": 50}}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["id"], 3);
    let shot = dir.join("embed.png");
    let r = call("ui.screenshot", json!({"path": shot.to_str().unwrap()}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!((r["result"]["width"].as_u64(), r["result"]["height"].as_u64()), (Some(900), Some(600)));
    assert!(std::fs::metadata(&shot).unwrap().len() > 1000);
    // Junk is refused and closes the connection; the session keeps serving others.
    let r = send("GET / HTTP/1.1".into());
    assert_eq!(r["ok"], false, "{r}");
    // Unsaved work makes app.quit ask first (the host stops the session then); end this one.
    let _ = child.kill();
    let _ = child.wait();
    // With nothing unsaved, app.quit ends the session.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(BIN).args(["embed", "--control", &port.to_string()]).stderr(Stdio::null()).spawn().unwrap();
    let start = Instant::now();
    let mut s2 = loop {
        match std::net::TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => break s,
            Err(_) if start.elapsed() < Duration::from_secs(60) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => panic!("embed never listened: {e}"),
        }
    };
    writeln!(s2, "{}", json!({"id": 1, "method": "app.quit"})).unwrap();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            panic!("app.quit didn't end the session");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(status.success());
    let _ = std::fs::remove_dir_all(dir);
}
