use engine::http_server::{build_axum_router, build_axum_router_with_dashboard, AxumApiState};

#[tokio::main]
async fn main() {
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("{}:{}", host, port);

    let state = AxumApiState::new();
    let dashboard_dir =
        std::env::var("ACP_DASHBOARD_DIR").or_else(|_| std::env::var("DASHBOARD_DIR"));
    let router = match dashboard_dir {
        Ok(path) if !path.trim().is_empty() => {
            println!("dashboard assets served from {}", path);
            build_axum_router_with_dashboard(state, path)
        }
        _ => build_axum_router(state),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("engine listening on {}", addr);
    axum::serve(listener, router).await.unwrap();
}
