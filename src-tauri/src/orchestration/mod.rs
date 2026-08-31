use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub mod metagpt;
pub mod agent_loop;
pub mod sandbox;
pub mod task_store;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowEvent {
    pub event_type: String,
    pub task_id: Option<String>,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub timestamp: u64,
}

// MetaGPT Workflow Engine - Native Environment + Role Architecture
/// 可重试的瞬时错误（429/限流/过载/超时/网络抖动）：暂停而非跳过角色
/// （也被 native_engine 的聊天工具循环复用，做供应商请求自动重试）
pub fn is_retryable_error(e: &str) -> bool {
    let l = e.to_lowercase();
    ["429", "too many requests", "rate_limit", "service_unavailable",
     "overloaded", "temporarily unavailable", "502", "503", "504",
     "timeout", "timed out", "error sending request", "connection reset"]
        .iter().any(|k| l.contains(k))
}

/// 返工工程师角色名（评审不通过时追加的修复轮次）
const REWORK_ROLE: &str = "EngineerRework";

/// 解析评审输出中的结论（approved/quality_score）。评审未按约定格式给出结论时
/// 默认视为通过，避免格式缺失触发多余的返工轮次。
fn parse_review_verdict(output: &str) -> (bool, u8) {
    let verdict = metagpt::ReviewVerdict::from_text(output);
    let lower = output.to_lowercase();
    let has_verdict = ["approved", "quality_score", "quality score", "score:", "rating:"]
        .iter().any(|k| lower.contains(k));
    (!has_verdict || verdict.approved, verdict.quality_score)
}

pub async fn metagpt_workflow(
    goal: &str,
    provider: &ResolvedProvider,
    workspace: Option<&str>,
    event_tx: tokio::sync::broadcast::Sender<WorkflowEvent>,
    db_manager: Option<std::sync::Arc<crate::db::DbManager>>,
    embedding_engine: Option<std::sync::Arc<crate::memory::embedding::EmbeddingEngine>>,
    resume_outputs: Vec<(String, String, String)>,
) -> Result<serde_json::Value> {
    use metagpt::{Environment, Message, CauseBy};
    use metagpt::action::Action;
    let start = std::time::Instant::now();
    // 30 分钟：7 角色 + 可能的返工轮次，每个角色内部工具循环上限就有 600s，
    // 旧的 10 分钟总上限会被正常长流水线误杀
    let timeout = tokio::time::Duration::from_secs(1800);
    if let Some(ws) = workspace { std::env::set_var("METAGPT_WORKSPACE", ws); }
    let mut env = Environment::new();
    metagpt::tool_loop::set_progress_sender(event_tx.clone());
    // Background: load fastembed ONNX model (non-blocking)
    crate::memory::embedding::EmbeddingEngine::spawn_local_init();
    let mut roles: Vec<metagpt::Role> = vec![
        metagpt::roles::create_product_manager(),
        metagpt::roles::create_architect(),
        metagpt::roles::create_engineer(),
        metagpt::roles::create_reviewer(),
        // 返工工程师：排在 Reviewer 之后、QA 之前；仅在评审不通过时启动，
        // 修复后 QA 在同一轮内基于修复上下文出测试
        metagpt::roles::create_engineer_for_rework(),
        metagpt::roles::create_qa_engineer(),
        metagpt::roles::create_devops(),
        metagpt::roles::create_project_manager(),
    ];
    for role in &roles {
        let types: Vec<&str> = role.watch.iter().map(|c| c.as_str()).collect();
        env.subscribe(&role.name, types);
    }
    let _ = event_tx.send(WorkflowEvent {
        event_type: "workflow_start".to_string(), task_id: None,
        message: "MetaGPT workflow started".to_string(),
        data: Some(serde_json::json!({"roles": roles.iter().filter(|r| r.name != REWORK_ROLE).count()})),
        timestamp: now_ms(),
    });
    // 续跑：把已完成角色的产出按原 cause_by 回放进环境，供下游角色消费；
    // 这些角色在循环里直接跳过
    let mut completed_roles: Vec<String> = Vec::new();
    let mut role_outputs: Vec<(String, String, String)> = Vec::new(); // (role_name, cause_by, output)
    // 评审门控：true = 评审通过（或未给出结论），返工工程师不启动
    let mut review_passed = true;
    for (rname, rcause, routput) in &resume_outputs {
        let cb = CauseBy::from_name(rcause);
        env.publish_message(Message::new(routput.clone(), rname.as_str(), cb, rname.as_str()));
        completed_roles.push(rname.clone());
        role_outputs.push((rname.clone(), rcause.clone(), routput.clone()));
        // 续跑时 Reviewer 产出已回放，同样要解析结论，避免断点在返工/QA 时门控失效
        if rname == "Reviewer" {
            review_passed = parse_review_verdict(routput).0;
        }
    }
    if !resume_outputs.is_empty() {
        let _ = event_tx.send(WorkflowEvent {
            event_type: "workflow_resumed".to_string(), task_id: None,
            message: format!("workflow resumed, {} roles replayed", resume_outputs.len()),
            data: Some(serde_json::json!({"replayed": resume_outputs.len()})),
            timestamp: now_ms(),
        });
    }
    let user_msg = Message::new(goal, "user", CauseBy::UserRequirement, "user");
    env.publish_message(user_msg);
    let mut round = 0usize;
    const MAX_ROUNDS: usize = 20;
    loop {
        if start.elapsed() > timeout {
            tracing::error!(target: "metagpt", "Workflow timeout 30min");
            let _ = event_tx.send(WorkflowEvent {
                event_type: "workflow_failed".to_string(), task_id: None,
                message: "Workflow timeout exceeded 30 minutes".to_string(),
                data: None, timestamp: now_ms(),
            });
            break;
        }
        if round >= MAX_ROUNDS { break; }
        let mut any_active = false;
        for role in &mut roles {
            if completed_roles.contains(&role.name) { continue; }
            // 返工门控：评审通过（或未给出结论）时，返工工程师视为完成、不启动
            if role.name == REWORK_ROLE && review_passed {
                completed_roles.push(role.name.clone());
                continue;
            }
            let has_msgs = role.observe(&env);
            tracing::info!(target: "metagpt", "Round {}: role={} has_msgs={} completed={} buffer_size={}", round, role.name, has_msgs, completed_roles.contains(&role.name), env.peek_messages(&role.name).len());
            if !has_msgs {
                tracing::info!(target: "metagpt", "  Skipping {} - no messages in buffer", role.name);
                continue;
            }
            any_active = true;
            let _ = event_tx.send(WorkflowEvent {
                event_type: "task_started".to_string(),
                task_id: Some(role.name.clone()),
                message: format!("{}: started", role.profile),
                data: Some(serde_json::json!({"agent_role": role.name})),
                timestamp: now_ms(),
            });
            match role.run(&mut env, provider).await {
                Ok(()) => {
                    let output = env.history.get_by_cause(
                        &role.actions.first().map(|a| a.cause_by()).unwrap_or(CauseBy::General)
                    ).last().map(|m| m.content.clone()).unwrap_or_default();
                    tracing::info!(target: "metagpt", "Role '{}' completed, output={} chars, history={} msgs", role.name, output.len(), env.history.get_all().len());
                    let cause_by_str = role.actions.first().map(|a| a.cause_by().as_str().to_string()).unwrap_or_else(|| "General".to_string());
                    role_outputs.push((role.name.clone(), cause_by_str.clone(), output.clone()));
                    // 评审角色：解析结论驱动返工门控，并把结论带给前端
                    let review_info = if role.name == "Reviewer" {
                        let (approved, score) = parse_review_verdict(&output);
                        review_passed = approved;
                        Some((approved, score))
                    } else { None };
                    let mut data = serde_json::json!({
                        "agent_role": role.name,
                        "cause_by": cause_by_str,
                        "output": {"output": output}
                    });
                    if let Some((approved, score)) = review_info {
                        data["review_approved"] = serde_json::Value::Bool(approved);
                        data["quality_score"] = serde_json::Value::from(score);
                    }
                    let _ = event_tx.send(WorkflowEvent {
                        event_type: "task_completed".to_string(),
                        task_id: Some(role.name.clone()),
                        message: format!("{}: completed", role.profile),
                        data: Some(data),
                        timestamp: now_ms(),
                    });
                    completed_roles.push(role.name.clone());
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let retryable = is_retryable_error(&err_str);
                    tracing::error!(target: "metagpt", "Role failed (retryable={}): {}", retryable, e);
                    let _ = event_tx.send(WorkflowEvent {
                        event_type: "task_failed".to_string(),
                        task_id: Some(role.name.clone()),
                        message: format!("{}: failed: {}", role.profile, e),
                        data: Some(serde_json::json!({"agent_role": role.name, "retryable": retryable})),
                        timestamp: now_ms(),
                    });
                    if retryable {
                        // 瞬时错误（限流/过载/超时）：暂停等待前端重试续跑，不跳过该角色
                        let _ = event_tx.send(WorkflowEvent {
                            event_type: "workflow_paused".to_string(),
                            task_id: Some(role.name.clone()),
                            message: format!("workflow paused at {}: {}", role.name, err_str),
                            data: Some(serde_json::json!({"failed_role": role.name, "error": err_str})),
                            timestamp: now_ms(),
                        });
                        return Ok(serde_json::json!({"paused": true, "roles_completed": completed_roles, "rounds": round}));
                    }
                    completed_roles.push(role.name.clone());
                }
            }
        }
        round += 1;
        if !any_active { break; }
    }
    let duration_ms = start.elapsed().as_millis() as u64;
    let _ = event_tx.send(WorkflowEvent {
        event_type: "workflow_completed".to_string(), task_id: None,
        message: format!("MetaGPT workflow completed in {}ms", duration_ms),
        data: Some(serde_json::json!({"duration_ms": duration_ms, "roles_completed": completed_roles.len(), "rounds": round})),
        timestamp: now_ms(),
    });

    // Phase 3: Persist role outputs to long-term memory.
    // 必须在 spawn_blocking 阻塞线程上 block_on：之前在 tokio worker 上直接起
    // runtime 会 panic（Cannot start a runtime from within a runtime），且 panic
    // 发生在 with_conn 闭包内握锁时，导致整个 DB 层永久失效。
    if let Some(db) = db_manager {
        let ws = workspace.unwrap_or("default").to_string();
        let _ = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
            if let Ok(rt) = rt {
                let result = db.with_conn(|conn| {
                    rt.block_on(crate::orchestration::metagpt::persistence::save_workflow_result(
                        conn, &ws, &role_outputs, embedding_engine.as_deref(),
                    ))
                });
                if let Err(e) = result {
                    tracing::warn!(target: "metagpt", "Workflow persistence failed: {}", e);
                }
            }
        }).await;
    }

    Ok(serde_json::json!({"roles_completed": completed_roles, "duration_ms": duration_ms, "rounds": round}))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
