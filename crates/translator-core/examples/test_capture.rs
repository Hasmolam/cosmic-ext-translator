use std::time::Duration;
use tokio::time::timeout;
use tracing_subscriber::EnvFilter;
use translator_core::{CaptureSession, Roi};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    println!("=======================================================");
    println!("COSMIC Ext Lingua - PipeWire Capture Test Utility");
    println!("Requesting ScreenCast portal session...");
    println!("(Please select a screen/window in the system dialog)");
    println!("=======================================================");

    // Test ROI: Central 400x200 box or default full screen
    let initial_roi = Some(Roi::new(100, 100, 400, 200));

    let mut session = CaptureSession::start(None, false, initial_roi).await?;
    println!("Session acquired. Waiting for first frame...");

    let mut receiver = session.frame_receiver.clone();

    // Wait up to 10 seconds for the first frame
    let frame = timeout(Duration::from_secs(10), async {
        loop {
            receiver.changed().await.ok();
            let current = receiver.borrow().clone();
            if let Some(f) = current {
                return f;
            }
        }
    })
    .await?;

    println!("First frame captured successfully!");
    println!(
        "Dimensions: {}x{} (Original: {}x{})",
        frame.image.width(),
        frame.image.height(),
        frame.original_width,
        frame.original_height
    );

    let output_path = "test_captured_frame.png";
    frame.image.save(output_path)?;
    println!("Saved cropped frame to {output_path}");

    println!("Stopping session cleanly...");
    session.stop()?;
    println!("Capture test completed with 0 errors!");

    Ok(())
}
