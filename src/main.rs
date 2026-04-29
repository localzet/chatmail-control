mod audit;
mod auth;
mod bans;
mod chatmail;
mod config;
mod db;
mod error;
mod health;
mod logs;
mod routes;
mod services;
mod shell;
mod users;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use auth::LoginRateLimiter;
use axum::{extract::FromRef, routing::get_service, Router};
use axum_extra::extract::cookie::Key;
use clap::{Args, Parser, Subcommand};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{config::Config, shell::Shell};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: SqlitePool,
    pub shell: Shell,
    pub cookie_key: Key,
    pub login_rate_limiter: LoginRateLimiter,
}

impl FromRef<AppState> for Key {
    fn from_ref(input: &AppState) -> Self {
        input.cookie_key.clone()
    }
}

#[derive(Parser)]
#[command(name = "chatmail-control")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve(ServeArgs),
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
}

#[derive(Args)]
struct ServeArgs {
    #[arg(
        long,
        env = "CHATMAIL_CONTROL_CONFIG",
        default_value = "/etc/chatmail-control/config.toml"
    )]
    config: PathBuf,
}

#[derive(Subcommand)]
enum AdminCommand {
    Create(AdminArgs),
    ResetPassword(AdminArgs),
}

#[derive(Args)]
struct AdminArgs {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    username: String,
    #[arg(long)]
    password: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chatmail_control=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Serve(args) => serve(args).await?,
        Commands::Admin { command } => run_admin_command(command).await?,
    }
    Ok(())
}

async fn run_admin_command(command: AdminCommand) -> anyhow::Result<()> {
    let args = match &command {
        AdminCommand::Create(args) | AdminCommand::ResetPassword(args) => args,
    };
    let path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("/etc/chatmail-control/config.toml"));
    let config = Config::load(path)?;
    let pool = db::connect(&config.server.database_url).await?;
    auth::upsert_admin(&pool, &args.username, &args.password).await?;
    println!("admin {} updated", args.username);
    Ok(())
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let config = Arc::new(Config::load(args.config)?);
    let pool = db::connect(&config.server.database_url).await?;
    let shell = Shell::new(chatmail::COMMAND_TIMEOUT_SECONDS);
    let cookie_key = derive_cookie_key(&config.auth.session_secret);

    let state = AppState {
        config: config.clone(),
        pool,
        shell,
        cookie_key,
        login_rate_limiter: LoginRateLimiter::default(),
    };

    let app = Router::new()
        .merge(routes::router())
        .nest_service("/static", get_service(ServeDir::new("static")))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(&config.server.bind).await?;
    tracing::info!("listening on {}", config.server.bind);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn derive_cookie_key(secret: &str) -> Key {
    let mut bytes = [0u8; 64];
    let secret_bytes = secret.as_bytes();
    for (idx, slot) in bytes.iter_mut().enumerate() {
        *slot = *secret_bytes.get(idx % secret_bytes.len()).unwrap_or(&0);
    }
    Key::from(&bytes)
}
