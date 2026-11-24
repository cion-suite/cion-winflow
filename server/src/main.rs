use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use cion_reader::app::{self, AppState};
use cion_reader::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // .env удобен в dev; в проде переменные задаёт systemd
    let _ = dotenvy::dotenv();
    init_tracing();

    let config = Config::from_env()?;
    let index = config.static_dir.join("index.html");
    if !index.is_file() {
        tracing::warn!(
            path = %index.display(),
            "SPA index.html not found: run `pnpm build` for static serving"
        );
    }

    let app = app::router(AppState::new(config.clone()));
    let listener = TcpListener::bind(config.bind_addr).await?;
    tracing::info!(
        addr = %listener.local_addr()?,
        static_dir = %config.static_dir.display(),
        "cion-reader server started"
    );

    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
