//! bwocd — BWOC control daemon (DESIGN.md §2, §4).
//!
//! Wraps `bwoc … --json` behind a small HTTP API so remote clients (the
//! control-center app on mobile, or a desktop controlling another host) can
//! read this host's fleet. Every route except `/health` requires an
//! ed25519-signed request (the bwoc-party wire format, X-BWOC-* headers).
//!
//! Deployment contract (guard-the-gate): bind LOOPBACK ONLY and expose via
//! `tailscale serve` — never a public port.

use bwocd::{auth, routes, AppState};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(
    name = "bwocd",
    about = "BWOC control daemon (read API, signed requests)"
)]
struct Args {
    /// Bind address. Loopback only by design; expose via `tailscale serve`.
    #[arg(long, default_value = "127.0.0.1:10130")]
    bind: SocketAddr,

    /// BWOC workspace this daemon serves (the primary — holds the enroll/pending
    /// store; also the sole workspace unless `--workspace-root` is set).
    #[arg(long, env = "BWOC_WORKSPACE")]
    workspace: PathBuf,

    /// Serve EVERY workspace under this root, not just `--workspace`. bwocd scans
    /// the root's direct children for `.bwoc/workspace.toml` on each request
    /// (dynamic — new workspaces appear without a restart). Agents from all of
    /// them show in `/fleet`, tagged with their workspace; targeted ops resolve
    /// the agent's own workspace.
    #[arg(long, env = "BWOCD_WORKSPACE_ROOT")]
    workspace_root: Option<PathBuf>,

    /// Controller registry (TOML). Default: <workspace>/.bwoc/bwocd/controllers.toml
    #[arg(long)]
    controllers: Option<PathBuf>,

    /// CORS allow-origin for a browser client (the chrome extension), e.g.
    /// `chrome-extension://<id>`. Omit for native/CLI controllers (no CORS).
    #[arg(long)]
    cors_origin: Option<String>,

    /// Dev escape: allow any origin (`*`) for CORS. Mirrors bwoc-mcp's
    /// `--http-insecure` posture — never use against a shared workspace.
    #[arg(long)]
    insecure_cors: bool,

    /// Max concurrent in-flight signed requests (bounds concurrent `bwoc`
    /// spawns). Excess is shed with 503. Raise for a larger fleet. Must be ≥ 1 —
    /// 0 would shed every signed request.
    #[arg(
        long,
        env = "BWOCD_MAX_CONCURRENCY",
        default_value_t = bwocd::DEFAULT_MAX_CONCURRENCY,
        value_parser = parse_max_concurrency
    )]
    max_concurrency: usize,
}

/// `--max-concurrency` must be a positive integer: 0 is a self-inflicted outage.
fn parse_max_concurrency(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(0) => Err("must be at least 1 (0 would refuse every signed request)".into()),
        Ok(n) => Ok(n),
        Err(e) => Err(e.to_string()),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if !args.bind.ip().is_loopback() {
        anyhow::bail!(
            "refusing to bind non-loopback address {} — expose via `tailscale serve` instead",
            args.bind
        );
    }
    anyhow::ensure!(
        args.workspace.is_dir(),
        "workspace not found: {}",
        args.workspace.display()
    );

    let controllers_path = args
        .controllers
        .unwrap_or_else(|| args.workspace.join(".bwoc/bwocd/controllers.toml"));
    let registry = auth::ControllerRegistry::load(&controllers_path)?;
    eprintln!(
        "bwocd: {} controller(s) from {}",
        registry.len(),
        controllers_path.display()
    );

    let cors_origin = if args.insecure_cors {
        eprintln!("bwocd: WARNING --insecure-cors enabled — allowing any browser origin");
        Some("*".to_string())
    } else {
        args.cors_origin
    };
    if let Some(o) = &cors_origin {
        // Fail fast on an origin that isn't a valid header value — otherwise the
        // CORS layer is silently dropped and browser clients fail mysteriously.
        anyhow::ensure!(
            o == "*" || axum::http::HeaderValue::from_str(o).is_ok(),
            "invalid --cors-origin {o:?}: not a valid HTTP header value"
        );
        eprintln!("bwocd: CORS allow-origin = {o}");
    }

    let workspaces = match args.workspace_root {
        Some(root) => {
            anyhow::ensure!(
                root.is_dir(),
                "workspace-root not found: {}",
                root.display()
            );
            let ws = bwocd::workspaces::Workspaces::rooted(root, args.workspace);
            let n = ws.discover().len();
            eprintln!(
                "bwocd: multi-workspace mode — {} workspace(s) under {}",
                n,
                ws.root.as_ref().unwrap().display()
            );
            ws
        }
        None => bwocd::workspaces::Workspaces::single(args.workspace),
    };

    let state = Arc::new(AppState {
        workspaces,
        registry,
        nonces: auth::NonceStore::default(),
        cors_origin,
        max_concurrency: args.max_concurrency,
    });
    eprintln!(
        "bwocd: max concurrency {} signed request(s)",
        args.max_concurrency
    );

    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    eprintln!("bwocd: listening on {}", args.bind);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
