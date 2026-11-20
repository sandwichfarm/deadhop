//! deadhop binary. All logic lives in the library.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    deadhop::app::command(&args, |k| std::env::var(k).ok(), deadhop::app::shutdown_signal()).await
}
