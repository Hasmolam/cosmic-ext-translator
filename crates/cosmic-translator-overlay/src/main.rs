#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("cosmic-translator-overlay initialized");
}
