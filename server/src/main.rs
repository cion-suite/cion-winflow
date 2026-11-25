use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use cion_reader::app;
use cion_reader::auth;
use cion_reader::config::Config;
use cion_reader::db;
use cion_reader::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // .env удобен в dev; в проде переменные задаёт systemd
    let _ = dotenvy::dotenv();
    init_tracing();

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    tokio::fs::create_dir_all(&config.data_dir).await?;

    if let Some(admin) = config.admin_bootstrap.clone() {
        if auth::bootstrap_admin(&pool, &admin).await? {
            tracing::warn!(
                username = %admin.username,
                "bootstrap admin created; unset ADMIN_USERNAME/ADMIN_PASSWORD"
            );
        } else {
            tracing::info!("bootstrap admin skipped: users already exist");
        }
    }

    let index = config.static_dir.join("index.html");
    if !index.is_file() {
        tracing::warn!(
            path = %index.display(),
            "SPA index.html not found: run `pnpm build` for static serving"
        );
    }

    let state = AppState::new(config.clone(), pool).await?;
    let app = app::router(state);
    let listener = TcpListener::bind(config.bind_addr).await?;
    tracing::info!(
        addr = %listener.local_addr()?,
        static_dir = %config.static_dir.display(),
        "cion-reader server started"
    );

    // ConnectInfo нужен rate limit по IP клиента
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
