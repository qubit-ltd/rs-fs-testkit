// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::timeout;

const SCENARIO_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER_JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_REAP_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn run_http_scenario<F, G>(
    phase: &str,
    server: JoinHandle<()>,
    release_gate: G,
    scenario: F,
) -> Result<(), String>
where
    F: Future<Output = Result<(), String>> + Send + 'static,
    G: FnOnce() + Send + 'static,
{
    let deadline = async {
        let _ = timeout(SCENARIO_TIMEOUT, std::future::pending::<()>()).await;
    };
    run_http_scenario_until(
        phase,
        server,
        release_gate,
        scenario,
        deadline,
        "scenario exceeded 10 second deadline",
    )
    .await
}

/// Runs one HTTP case until either its scenario or a controlled deadline ends.
pub async fn run_http_scenario_until<F, G, D>(
    phase: &str,
    server: JoinHandle<()>,
    release_gate: G,
    scenario: F,
    deadline: D,
    deadline_diagnostic: &'static str,
) -> Result<(), String>
where
    F: Future<Output = Result<(), String>> + Send + 'static,
    G: FnOnce() + Send + 'static,
    D: Future<Output = ()> + Send + 'static,
{
    let mut server = Some(server);
    let mut scenario = tokio::spawn(scenario);
    let outcome = tokio::select! {
        outcome = &mut scenario => Some(outcome),
        () = deadline => None,
    };
    let release_error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(release_gate))
        .err()
        .map(|payload| panic_message(&payload));
    let scenario_result = match outcome {
        Some(Ok(result)) => result,
        Some(Err(join_error)) if join_error.is_panic() => {
            let panic = join_error.into_panic();
            if let Err(cleanup) = abort_and_reap(server.take().expect("server task")).await {
                eprintln!("{phase}: server cleanup after scenario panic failed: {cleanup}");
            }
            if let Some(error) = &release_error {
                eprintln!("{phase}: release gate also panicked: {error}");
            }
            std::panic::resume_unwind(panic);
        }
        Some(Err(join_error)) => Err(format!("{phase}: scenario task failed: {join_error}")),
        None => {
            scenario.abort();
            let scenario_reap = match timeout(SERVER_REAP_TIMEOUT, &mut scenario).await {
                Ok(Ok(_)) => "scenario task reaped".to_owned(),
                Ok(Err(error)) if error.is_cancelled() => "scenario task reaped".to_owned(),
                Ok(Err(error)) if error.is_panic() => {
                    format!(
                        "scenario task panicked while reaping: {}",
                        error
                            .into_panic()
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .unwrap_or("non-string panic")
                    )
                }
                Ok(Err(error)) => format!("scenario reaping failed: {error}"),
                Err(_) => "scenario task reaping deadline exceeded".to_owned(),
            };
            let server_reap = abort_and_reap(server.take().expect("server task"))
                .await
                .map(|()| "server task reaped".to_owned())
                .unwrap_or_else(|error| format!("server cleanup failed: {error}"));
            let release = release_error
                .as_ref()
                .map(|error| format!("; release gate panicked: {error}"))
                .unwrap_or_default();
            Err(format!(
                "{phase}: {deadline_diagnostic}; {scenario_reap}; {server_reap}{release}"
            ))
        }
    };

    match scenario_result {
        Ok(()) if release_error.is_some() => {
            let error = release_error.as_ref().expect("release error");
            match abort_and_reap(server.take().expect("server task")).await {
                Ok(()) => Err(format!(
                    "{phase}: release gate panicked: {error}; server task reaped"
                )),
                Err(cleanup) => Err(format!(
                    "{phase}: release gate panicked: {error}; server cleanup failed: {cleanup}"
                )),
            }
        }
        Ok(()) => match timeout(SERVER_JOIN_TIMEOUT, server.as_mut().expect("server task")).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(join_error)) if join_error.is_panic() => {
                std::panic::resume_unwind(join_error.into_panic());
            }
            Ok(Err(join_error)) => Err(format!("{phase}: server task failed: {join_error}")),
            Err(_) => {
                let reap = abort_and_reap(server.take().expect("server task")).await;
                Err(match reap {
                    Ok(()) => format!(
                        "{phase}: server join exceeded 5 second deadline; server task reaped"
                    ),
                    Err(error) => format!(
                        "{phase}: server join exceeded 5 second deadline; server cleanup failed: {error}"
                    ),
                })
            }
        },
        Err(error) => {
            if let Some(server) = server.take() {
                match abort_and_reap(server).await {
                    Ok(()) => Err(format!("{error}; server task reaped")),
                    Err(cleanup) => Err(format!("{error}; server cleanup failed: {cleanup}")),
                }
            } else {
                Err(error)
            }
        }
    }
}

async fn abort_and_reap(server: JoinHandle<()>) -> Result<(), String> {
    server.abort();
    match timeout(SERVER_REAP_TIMEOUT, server).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(join_error)) if join_error.is_cancelled() => Ok(()),
        Ok(Err(join_error)) if join_error.is_panic() => Err(format!(
            "server task panicked while reaping: {}",
            panic_message(&join_error.into_panic())
        )),
        Ok(Err(join_error)) => Err(format!("server task reaping failed: {join_error}")),
        Err(_) => Err("server task did not stop within 2 second reaping deadline".into()),
    }
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&'static str>()
                .map(|value| (*value).to_owned())
        })
        .unwrap_or_else(|| "non-string panic payload".into())
}
