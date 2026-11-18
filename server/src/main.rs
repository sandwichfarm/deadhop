//! cvmpay binary. All logic lives in the library.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    cvmpay::app::command(&args, |k| std::env::var(k).ok(), cvmpay::app::shutdown_signal()).await
}
