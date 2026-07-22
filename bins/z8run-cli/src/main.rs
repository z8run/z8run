//! # z8run CLI
//!
//! Main entry point for the z8run flow engine.
//! Manages the server, migrations, plugins and system information.

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

/// z8run - Next Generation Visual Flow Engine
#[derive(Parser)]
#[command(name = "z8run", version, about, long_about = None)]
struct Cli {
    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "Z8_LOG_LEVEL", default_value = "info")]
    log_level: String,

    /// Data directory
    #[arg(long, env = "Z8_DATA_DIR", default_value = "./data")]
    data_dir: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the z8run server
    Serve {
        /// HTTP/WebSocket port
        #[arg(short, long, env = "Z8_PORT", default_value = "7700")]
        port: u16,

        /// Bind address
        #[arg(long, env = "Z8_BIND", default_value = "0.0.0.0")]
        bind: String,

        /// Database URL (sqlite://./data/z8run.db or postgres://...)
        #[arg(long, env = "Z8_DB_URL")]
        db_url: Option<String>,
    },

    /// Run database migrations
    Migrate {
        /// Database URL
        #[arg(long, env = "Z8_DB_URL")]
        db_url: Option<String>,
    },

    /// Plugin management
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Show system information
    Info,

    /// Validate a flow file
    Validate {
        /// Path to the flow file (JSON)
        file: String,
    },
}

#[derive(Subcommand)]
enum PluginAction {
    /// List installed plugins
    List,
    /// Install a plugin from a local .wasm file or directory
    ///
    /// Examples:
    ///   z8run plugin install ./csv-parser.wasm
    ///   z8run plugin install ./plugins/json-transform/
    Install {
        /// Path to .wasm file or plugin directory with manifest.toml
        source: String,
    },
    /// Uninstall a plugin by name
    ///
    /// Examples:
    ///   z8run plugin remove csv-parser
    Remove {
        /// Plugin name (as shown in 'plugin list')
        name: String,
    },
    /// Scan the plugin directory
    Scan,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file (silently ignore if not found)
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    // Configure tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
        )
        .with_target(true)
        .with_thread_ids(false)
        .init();

    match cli.command {
        Commands::Serve { port, bind, db_url } => {
            cmd_serve(port, bind, db_url, &cli.data_dir).await?;
        }
        Commands::Migrate { db_url } => {
            cmd_migrate(db_url, &cli.data_dir).await?;
        }
        Commands::Plugin { action } => {
            cmd_plugin(action, &cli.data_dir).await?;
        }
        Commands::Info => {
            cmd_info();
        }
        Commands::Validate { file } => {
            cmd_validate(&file).await?;
        }
    }

    Ok(())
}

/// The well-known placeholder secret shipped in `.env.example`.
const WEAK_PLACEHOLDER_SECRET: &str = "change-me-in-production";

/// Minimum acceptable secret length (in characters) for adequate entropy.
const MIN_SECRET_LEN: usize = 16;

/// Returns `true` if the given secret is a known default or too short.
fn is_weak_secret(value: &str) -> bool {
    value == WEAK_PLACEHOLDER_SECRET || value.len() < MIN_SECRET_LEN
}

/// Reject known-weak/default production secrets.
///
/// Fails with an actionable [`anyhow`] error when `value` matches the shipped
/// placeholder or is shorter than [`MIN_SECRET_LEN`] characters.
fn validate_production_secret(name: &str, value: &str) -> anyhow::Result<()> {
    if is_weak_secret(value) {
        anyhow::bail!(
            "{name} is a known default or too short; set a strong secret (>= {MIN_SECRET_LEN} chars). \
             Generate with: openssl rand -base64 32"
        );
    }
    Ok(())
}

/// Masks the password in a database URL so it is safe to log.
///
/// `scheme://user:pass@host/db` becomes `scheme://user:***@host/db`. URLs
/// without userinfo (e.g. SQLite file paths) are returned unchanged.
fn mask_db_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let after_scheme = scheme_end + 3;
    // The userinfo ends at the first '@' after the scheme.
    let Some(at_rel) = url[after_scheme..].find('@') else {
        return url.to_string();
    };
    let at = after_scheme + at_rel;
    let userinfo = &url[after_scheme..at];
    let masked_userinfo = match userinfo.find(':') {
        Some(colon) => format!("{}:***", &userinfo[..colon]),
        None => userinfo.to_string(),
    };
    format!("{}{}{}", &url[..after_scheme], masked_userinfo, &url[at..])
}

/// The storage and vault backends selected by the database URL.
type Backends = (
    Arc<dyn z8run_storage::repository::FlowRepository>,
    Arc<dyn z8run_storage::repository::UserRepository>,
    Arc<dyn z8run_storage::repository::ExecutionRepository>,
    Arc<dyn z8run_storage::credential_vault::CredentialVault>,
);

/// Start the z8run server.
async fn cmd_serve(
    port: u16,
    bind: String,
    db_url: Option<String>,
    data_dir: &str,
) -> anyhow::Result<()> {
    println!(
        r#"
    ╔═══════════════════════════════════════╗
    ║           z8run v{}            ║
    ║   Next Generation Flow Engine         ║
    ╚═══════════════════════════════════════╝
    "#,
        env!("CARGO_PKG_VERSION")
    );

    tracing::info!(port, bind = %bind, data_dir, "Starting z8run server");

    // Create data directory if it doesn't exist
    std::fs::create_dir_all(data_dir)?;
    std::fs::create_dir_all(format!("{}/plugins", data_dir))?;

    // Scan plugins
    let registry = z8run_runtime::registry::PluginRegistry::new(format!("{}/plugins", data_dir));
    let plugin_count = registry.scan().await.unwrap_or(0);
    tracing::info!(plugins = plugin_count, "Plugins scanned");

    // Initialize storage (PostgreSQL or SQLite based on URL)
    let url = db_url.unwrap_or_else(|| format!("sqlite://{}/z8run.db?mode=rwc", data_dir));

    // JWT secret: required in production, auto-generated for development
    let jwt_secret = match std::env::var("Z8_JWT_SECRET") {
        Ok(secret) if !secret.is_empty() => {
            tracing::info!("JWT secret loaded from Z8_JWT_SECRET");
            secret
        }
        _ => {
            if url.starts_with("postgres") || url.starts_with("mysql") {
                anyhow::bail!(
                    "Z8_JWT_SECRET is required when using PostgreSQL or MySQL. \
                     Generate one with: openssl rand -base64 32"
                );
            }
            let dev_secret: String = (0..32)
                .map(|_| format!("{:02x}", rand::random::<u8>()))
                .collect();
            tracing::warn!(
                "No Z8_JWT_SECRET set - generated ephemeral secret (tokens won't survive restarts)"
            );
            dev_secret
        }
    };
    // Z8_VAULT_SECRET falls back to the JWT secret when unset. Validate the
    // effective value that will actually be used for the vault.
    let vault_secret = std::env::var("Z8_VAULT_SECRET").unwrap_or_else(|_| jwt_secret.clone());

    // Reject known-weak/default secrets before startup.
    // Production databases (PostgreSQL/MySQL) hard-fail; SQLite (dev) only warns.
    let is_production_db = url.starts_with("postgres") || url.starts_with("mysql");
    if is_production_db {
        validate_production_secret("Z8_JWT_SECRET", &jwt_secret)?;
        validate_production_secret("Z8_VAULT_SECRET", &vault_secret)?;
    } else {
        if is_weak_secret(&jwt_secret) {
            tracing::warn!(
                "Z8_JWT_SECRET is a known default or too short (< 16 chars); this is insecure. \
                 Set a strong secret before deploying. Generate with: openssl rand -base64 32"
            );
        }
        if is_weak_secret(&vault_secret) {
            tracing::warn!(
                "Z8_VAULT_SECRET is a known default or too short (< 16 chars); this is insecure. \
                 Set a strong secret before deploying. Generate with: openssl rand -base64 32"
            );
        }
    }

    let (storage, user_storage, executions, vault): Backends = if url.starts_with("postgres") {
        tracing::info!(url = %mask_db_url(&url), "Connecting to PostgreSQL");
        let pg = z8run_storage::postgres::PgStorage::new(&url).await?;
        pg.migrate().await?;
        tracing::info!("PostgreSQL ready");
        let pg_arc = Arc::new(pg);
        let vault_pg = Arc::new(z8run_storage::credential_vault::PgCredentialVault::new(
            pg_arc.pool().clone(),
            &vault_secret,
        ));
        (
            pg_arc.clone() as Arc<dyn z8run_storage::repository::FlowRepository>,
            pg_arc.clone() as Arc<dyn z8run_storage::repository::UserRepository>,
            pg_arc as Arc<dyn z8run_storage::repository::ExecutionRepository>,
            vault_pg as Arc<dyn z8run_storage::credential_vault::CredentialVault>,
        )
    } else {
        tracing::info!(url = %mask_db_url(&url), "Connecting to SQLite");
        let sqlite = z8run_storage::sqlite::SqliteStorage::new(&url).await?;
        sqlite.migrate().await?;
        tracing::info!("SQLite ready");
        let sqlite_arc = Arc::new(sqlite);
        let vault_sqlite = Arc::new(z8run_storage::credential_vault::SqliteCredentialVault::new(
            sqlite_arc.pool().clone(),
            &vault_secret,
        ));
        (
            sqlite_arc.clone() as Arc<dyn z8run_storage::repository::FlowRepository>,
            sqlite_arc.clone() as Arc<dyn z8run_storage::repository::UserRepository>,
            sqlite_arc as Arc<dyn z8run_storage::repository::ExecutionRepository>,
            vault_sqlite as Arc<dyn z8run_storage::credential_vault::CredentialVault>,
        )
    };
    tracing::info!("Credential vault initialized");

    // Create application state
    let state = Arc::new(z8run_api::state::AppState::new(
        storage,
        user_storage,
        executions,
        vault,
        jwt_secret,
        port,
    ));

    // Register built-in node executors
    z8run_core::nodes::register_builtin_nodes(&state.engine).await;
    tracing::info!("Built-in nodes registered");

    // Build router
    let app = z8run_api::build_router(state);

    // Start server
    let addr = format!("{}:{}", bind, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(address = %addr, "Server ready");
    tracing::info!("Editor: http://{}:{}", bind, port);

    // Serve with connection info so per-IP rate limiting can read the real TCP
    // peer address (see z8run_api::rate_limit) instead of trusting spoofable
    // forwarded headers.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// Run database migrations.
async fn cmd_migrate(db_url: Option<String>, data_dir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let url = db_url.unwrap_or_else(|| format!("sqlite://{}/z8run.db?mode=rwc", data_dir));
    tracing::info!(url = %mask_db_url(&url), "Running migrations...");

    if url.starts_with("postgres") {
        let pg = z8run_storage::postgres::PgStorage::new(&url).await?;
        pg.migrate().await.map_err(|e| anyhow::anyhow!(e))?;
    } else {
        let sqlite = z8run_storage::sqlite::SqliteStorage::new(&url).await?;
        sqlite.migrate().await.map_err(|e| anyhow::anyhow!(e))?;
    }

    tracing::info!("Migrations completed");
    Ok(())
}

/// Plugin management.
async fn cmd_plugin(action: PluginAction, data_dir: &str) -> anyhow::Result<()> {
    let registry = z8run_runtime::registry::PluginRegistry::new(format!("{}/plugins", data_dir));

    match action {
        PluginAction::List => {
            let plugins = registry.list().await;
            if plugins.is_empty() {
                println!("No plugins installed.");
            } else {
                println!("{:<20} {:<10} DESCRIPTION", "NAME", "VERSION");
                println!("{}", "-".repeat(60));
                for p in plugins {
                    println!(
                        "{:<20} {:<10} {}",
                        p.manifest.name, p.manifest.version, p.manifest.description
                    );
                }
            }
        }
        PluginAction::Install { source } => {
            let source_path = std::path::Path::new(&source);
            println!("Installing plugin from: {}", source);
            match registry.install_local(source_path).await {
                Ok(name) => println!("✓ Plugin '{}' installed successfully", name),
                Err(e) => {
                    eprintln!("✗ Failed to install plugin: {}", e);
                    std::process::exit(1);
                }
            }
        }
        PluginAction::Remove { name } => {
            println!("Removing plugin: {}", name);
            match registry.remove(&name).await {
                Ok(()) => println!("✓ Plugin '{}' removed successfully", name),
                Err(e) => {
                    eprintln!("✗ Failed to remove plugin: {}", e);
                    std::process::exit(1);
                }
            }
        }
        PluginAction::Scan => {
            let count = registry.scan().await?;
            println!("{} plugins found and registered", count);
        }
    }

    Ok(())
}

/// Show system information.
fn cmd_info() {
    println!("z8run v{}", env!("CARGO_PKG_VERSION"));
    println!("Next Generation Visual Flow Engine");
    println!();
    println!("License:     Apache-2.0 / MIT");
    println!("Repository:  https://github.com/z8run/z8run");
    println!("Web:         https://z8run.org");
    println!();
    println!("Runtime:     Rust + Tokio (async multi-thread)");
    println!("Plugins:     WebAssembly (wasmtime)");
    println!("Protocol:    Binary over WebSockets");
}

/// Validate a JSON flow file.
async fn cmd_validate(file: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(file)?;
    let flow: z8run_core::Flow = serde_json::from_str(&content)?;

    println!("Flow: {} ({})", flow.name, flow.id);
    println!("Nodes: {}", flow.nodes.len());
    println!("Edges: {}", flow.edges.len());

    // Validate DAG
    match flow.validate_acyclic() {
        Ok(()) => println!("✓ Valid graph (DAG without cycles)"),
        Err(e) => println!("✗ Error: {}", e),
    }

    // Topological order
    match flow.topological_order() {
        Ok(order) => {
            println!("✓ Execution order:");
            for (i, node_id) in order.iter().enumerate() {
                if let Some(node) = flow.find_node(*node_id) {
                    println!("  {}. {} ({})", i + 1, node.name, node.node_type);
                }
            }
        }
        Err(e) => println!("✗ Error: {}", e),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_db_url_masks_password() {
        assert_eq!(
            mask_db_url("postgres://user:secret@localhost:5432/z8run"),
            "postgres://user:***@localhost:5432/z8run"
        );
        assert_eq!(
            mask_db_url("mysql://root:p%40ss@db:3306/app"),
            "mysql://root:***@db:3306/app"
        );
    }

    #[test]
    fn mask_db_url_leaves_credential_free_urls_unchanged() {
        // No userinfo -> unchanged.
        assert_eq!(
            mask_db_url("postgres://localhost:5432/z8run"),
            "postgres://localhost:5432/z8run"
        );
        // SQLite file paths have no "://" userinfo and are returned as-is.
        assert_eq!(
            mask_db_url("sqlite://./data/z8run.db?mode=rwc"),
            "sqlite://./data/z8run.db?mode=rwc"
        );
    }

    #[test]
    fn is_weak_secret_flags_placeholder_and_short_values() {
        assert!(is_weak_secret("change-me-in-production"));
        assert!(is_weak_secret("short"));
        assert!(!is_weak_secret("a-sufficiently-long-strong-secret-value"));
    }
}
