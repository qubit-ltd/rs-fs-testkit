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
    let mut server = server;
    let mut scenario = tokio::spawn(scenario);
    let outcome = tokio::select! {
        outcome = &mut scenario => Some(outcome),
        () = deadline => None,
    };
    release_gate();
    let scenario_result = match outcome {
        Some(Ok(result)) => result,
        Some(Err(join_error)) if join_error.is_panic() => {
            let panic = join_error.into_panic();
            abort_and_reap(&mut server).await?;
            std::panic::resume_unwind(panic);
        }
        Some(Err(join_error)) => Err(format!("{phase}: scenario task failed: {join_error}")),
        None => {
            scenario.abort();
            if let Err(join_error) = scenario.await {
                if join_error.is_panic() {
                    std::panic::resume_unwind(join_error.into_panic());
                }
            }
            Err(format!("{phase}: {deadline_diagnostic}"))
        }
    };

    match scenario_result {
        Ok(()) => match timeout(SERVER_JOIN_TIMEOUT, &mut server).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(join_error)) if join_error.is_panic() => {
                std::panic::resume_unwind(join_error.into_panic());
            }
            Ok(Err(join_error)) => Err(format!("{phase}: server task failed: {join_error}")),
            Err(_) => {
                abort_and_reap(&mut server).await?;
                Err(format!(
                    "{phase}: server join exceeded 5 second deadline; server task reaped"
                ))
            }
        },
        Err(error) => {
            abort_and_reap(&mut server).await?;
            Err(format!("{error}; server task reaped"))
        }
    }
}

async fn abort_and_reap(server: &mut JoinHandle<()>) -> Result<(), String> {
    server.abort();
    match timeout(SERVER_REAP_TIMEOUT, server).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(join_error)) if join_error.is_cancelled() => Ok(()),
        Ok(Err(join_error)) if join_error.is_panic() => {
            std::panic::resume_unwind(join_error.into_panic());
        }
        Ok(Err(join_error)) => Err(format!("server task reaping failed: {join_error}")),
        Err(_) => Err("server task did not stop within 2 second reaping deadline".into()),
    }
}
