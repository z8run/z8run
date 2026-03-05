//! # z8run CLI
//!
//! Main entry point for the z8run flow engine.
//! Manages the server, migrations, plugins and system information.

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

/// z8run — Next Generation Visual Flow Engine
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
    /// Install a plugin from the registry
    Install {
        /// Plugin name
        name: String,
    },
    /// Uninstall a plugin
    Remove {
        /// Plugin name
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
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
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
    let registry = z8run_runtime::registry::PluginRegistry::new(
        format!("{}/plugins", data_dir),
    );
    let plugin_count = registry.scan().await.unwrap_or(0);
    tracing::info!(plugins = plugin_count, "Plugins scanned");

    // Initialize storage (PostgreSQL or SQLite based on URL)
    let url = db_url.unwrap_or_else(|| {
        format!("sqlite://{}/z8run.db?mode=rwc", data_dir)
    });

    let (storage, user_storage): (
        Arc<dyn z8run_storage::repository::FlowRepository>,
        Arc<dyn z8run_storage::repository::UserRepository>,
    ) = if url.starts_with("postgres") {
        tracing::info!(url = %url, "Connecting to PostgreSQL");
        let pg = z8run_storage::postgres::PgStorage::new(&url).await?;
        pg.migrate().await?;
        tracing::info!("PostgreSQL ready");
        let pg_arc = Arc::new(pg);
        (pg_arc.clone() as Arc<dyn z8run_storage::repository::FlowRepository>,
         pg_arc as Arc<dyn z8run_storage::repository::UserRepository>)
    } else {
        tracing::info!(url = %url, "Connecting to SQLite");
        let sqlite = z8run_storage::sqlite::SqliteStorage::new(&url).await?;
        sqlite.migrate().await?;
        tracing::info!("SQLite ready");
        let sqlite_arc = Arc::new(sqlite);
        (sqlite_arc.clone() as Arc<dyn z8run_storage::repository::FlowRepository>,
         sqlite_arc as Arc<dyn z8run_storage::repository::UserRepository>)
    };

    // Create application state
    let state = Arc::new(z8run_api::state::AppState::new(
        storage,
        user_storage,
        "z8run-dev-secret".to_string(), // TODO: generate or load from config
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

    axum::serve(listener, app).await?;

    Ok(())
}

/// Run database migrations.
async fn cmd_migrate(db_url: Option<String>, data_dir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let url = db_url.unwrap_or_else(|| {
        format!("sqlite://{}/z8run.db?mode=rwc", data_dir)
    });
    tracing::info!(url = %url, "Running migrations...");

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
    let registry = z8run_runtime::registry::PluginRegistry::new(
        format!("{}/plugins", data_dir),
    );

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
        PluginAction::Install { name } => {
            println!("Installing plugin: {}", name);
            // TODO: download from remote registry
            println!("Pending functionality: download from remote registry");
        }
        PluginAction::Remove { name } => {
            println!("Uninstalling plugin: {}", name);
            // TODO: remove from plugin directory
            println!("Pending functionality");
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
