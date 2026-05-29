use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use crate::AppState;

pub fn install() -> PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder")
}

/// テスト用: 同一プロセス内で複数回呼ばれても安全なハンドル取得。
/// OnceLock でグローバルレコーダーの二重登録を防ぐ。
pub fn install_once() -> PrometheusHandle {
    static HANDLE: std::sync::OnceLock<PrometheusHandle> = std::sync::OnceLock::new();
    HANDLE
        .get_or_init(|| {
            PrometheusBuilder::new()
                .install_recorder()
                .expect("failed to install Prometheus recorder")
        })
        .clone()
}

pub async fn handler(State(state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        state.metrics.render(),
    )
}

/// プロセスメトリクス(RSS / FD 数 / スレッド数)を 15 秒ごとに更新する。
/// Linux の /proc/self/status + /proc/self/fd を読む。
pub fn spawn_process_metrics_task() {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        loop {
            interval.tick().await;
            collect_process_metrics();
        }
    });
}

#[cfg(target_os = "linux")]
fn collect_process_metrics() {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                if let Some(kb) = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    metrics::gauge!("process_resident_memory_bytes").set((kb * 1024) as f64);
                }
            } else if line.starts_with("Threads:")
                && let Some(n) = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<u64>().ok())
            {
                metrics::gauge!("process_threads").set(n as f64);
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
        metrics::gauge!("process_open_fds").set(entries.count() as f64);
    }
}

#[cfg(not(target_os = "linux"))]
fn collect_process_metrics() {}
