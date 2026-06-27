mod app;
mod components;
mod copies;
mod server;
mod util;

fn main() {
    #[cfg(feature = "server")]
    {
        use clap::{
            Parser,
            builder::styling::{AnsiColor, Effects, Styles},
        };
        use clap_verbosity_flag::{InfoLevel, Verbosity};

        fn clap_styles() -> Styles {
            Styles::styled()
                .header(AnsiColor::Green.on_default() | Effects::BOLD)
                .usage(AnsiColor::Green.on_default() | Effects::BOLD)
                .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
                .placeholder(AnsiColor::Cyan.on_default())
        }

        /// Visit Quang Nam AI Trip Planner server.
        #[derive(Parser)]
        #[command(version, about, styles = clap_styles(), color = clap::ColorChoice::Always)]
        struct Cli {
            /// Verbosity level (-v, -vv, -q, -qq)
            #[command(flatten)]
            verbose: Verbosity<InfoLevel>,
        }

        let cli = Cli::parse();

        // Load ./.env if present. Process-supplied env vars win (dotenvy
        // never overrides an already-set var), so this is a no-op in prod
        // containers that inject keys via the orchestrator. Kept server
        // only — keys never ship to wasm.
        let _ = dotenvy::dotenv();

        // Build the tracing filter: start with the CLI verbosity as the
        // base level, then layer on any RUST_LOG directives so per-module
        // overrides win.
        let lvl = cli.verbose.log_level_filter();
        let base = match lvl {
            log::LevelFilter::Off => "off",
            log::LevelFilter::Error => "error",
            log::LevelFilter::Warn => "warn",
            log::LevelFilter::Info => "info",
            log::LevelFilter::Debug => "debug",
            log::LevelFilter::Trace => "trace",
        };
        let mut filter = tracing_subscriber::filter::EnvFilter::try_new(base).unwrap_or_default();
        for dir_str in std::env::var("RUST_LOG").unwrap_or_default().split(',') {
            let d = dir_str.trim();
            if !d.is_empty()
                && let Ok(directive) = d.parse::<tracing_subscriber::filter::Directive>()
            {
                filter = filter.add_directive(directive);
            }
        }

        // Runtime tracing is initialised once per process so server-side
        // `tracing` macros emit boot logs, query-embed errors, duplicate
        // activity warnings, etc. `try_init` is safe to call repeatedly
        // (defensive — `build_corpus` is a separate binary, but dx serve /
        // hot reload spawn the server once per process).
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
        // Eagerly warm the singletons so the first request doesn't pay
        // the corpus.json load + parse cost on its critical path. Failures
        // are cached in `OnceLock` per the existing contract — the first
        // real request will return the same error.
        let _ = server::shared_retriever();
        let _ = server::shared_llm();

        // Resolve the address once, matching Dioxus's own IP/PORT env-var
        // contract (defaults 127.0.0.1:8080).
        let ip = std::env::var("IP").unwrap_or_else(|_| "127.0.0.1".into());
        let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
        let addr: std::net::SocketAddr = format!("{ip}:{port}").parse().expect("invalid IP:PORT");

        tracing::info!("Serving frontend at http://{addr}");

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            dioxus_server::axum::serve(listener, dioxus_server::router(app::App))
                .await
                .unwrap();
        });
    }

    #[cfg(not(feature = "server"))]
    dioxus::launch(app::App);
}
