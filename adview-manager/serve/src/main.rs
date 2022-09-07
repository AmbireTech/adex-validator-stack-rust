use adview_serve::app::Application;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    Application::new()?.run().await?;

    Ok(())
}
