//! Live end-to-end probe: resolves the real ChatGPT subscription credential
//! from ~/.codex/auth.json and streams a one-shot response through the Cos
//! harness — proving credential resolution + transport + SSE parsing against
//! the production backend. Run: `cargo run -p cos-core --example live_probe`.

use cos_core::*;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let provider = DefaultCatalog::providers()
        .into_iter()
        .find(|provider| provider.id == "chatgpt")
        .expect("chatgpt provider");
    let model = DefaultCatalog::models()
        .into_iter()
        .find(|model| model.provider_id == "chatgpt")
        .expect("chatgpt model");

    let runtime = AgentRuntime::default();
    match runtime.session_info(&provider) {
        Some(session) => println!("credential: {}", session.display_name()),
        None => {
            eprintln!("NO SESSION — credential resolution failed");
            std::process::exit(2);
        }
    }

    let mut thread = CosThread::new("/tmp", &model.id);
    thread.effort = ReasoningEffort::Low;
    let mut request = AgentRequest::new(
        "Reply with exactly: COS LIVE OK",
        Some("Reply with exactly: COS LIVE OK".into()),
        thread,
        model,
        provider,
        ReasoningEffort::Low,
        false,
        false,
    );
    request.workspace_is_trusted = true;
    request.tools_enabled = false;

    let mut stream = match runtime.stream(request) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("STREAM REFUSED: {error}");
            std::process::exit(3);
        }
    };

    let mut text = String::new();
    let mut statuses = 0usize;
    let mut usages = 0usize;
    while let Some(event) = stream.next().await {
        match event {
            Ok(AgentEvent::TextDelta(delta)) => text.push_str(&delta),
            Ok(AgentEvent::Status(status)) => {
                statuses += 1;
                println!("status: {status}");
            }
            Ok(AgentEvent::Usage { input, output }) => {
                usages += 1;
                println!("usage: in={input} out={output}");
            }
            Ok(AgentEvent::Completed) => println!("completed"),
            Ok(_) => {}
            Err(error) => {
                eprintln!("RUN ERROR: {error}");
                std::process::exit(4);
            }
        }
    }
    println!("reply: {}", text.trim());
    if text.contains("COS LIVE OK") && usages > 0 {
        println!("PROBE OK (statuses={statuses})");
    } else {
        eprintln!("PROBE FAILED: unexpected reply content");
        std::process::exit(5);
    }
}
