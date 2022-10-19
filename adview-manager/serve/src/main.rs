use adview_serve::app::Application;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tracing_subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(tracing_subscriber)
    .expect("setting tracing default failed");

    Application::new()?.run().await?;

    Ok(())
}
