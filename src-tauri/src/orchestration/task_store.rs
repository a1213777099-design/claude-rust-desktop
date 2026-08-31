/// Structured task system for inter-agent communication.
///
/// Agents can create tasks, update status, pass results, and query
/// other agents' outputs. Used by the multi-agent orchestrator.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

/// Task status in the lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// A structured task created by an agent or the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub task_id: String,
    pub created_by: String,
    pub assigned_to: Option<String>,
    pub title: String,
    pub description: String,
    pub state: TaskState,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub dependencies: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Event broadcast when a task state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub event_type: String,
    pub agent_id: String,
    pub data: Option<serde_json::Value>,
    pub timestamp: String,
}

/// Central task store shared across all agents.
pub struct TaskStore {
    tasks: Arc<Mutex<HashMap<String, AgentTask>>>,
    event_tx: broadcast::Sender<TaskEvent>,
}

impl TaskStore {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    /// Subscribe to task events.
    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.event_tx.subscribe()
    }

    /// Create a new task.
    pub async fn create_task(
        &self,
        created_by: &str,
        assigned_to: Option<&str>,
        title: &str,
        description: &str,
        input: Option<serde_json::Value>,
        dependencies: Vec<String>,
    ) -> String {
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let task = AgentTask {
            task_id: task_id.clone(),
            created_by: created_by.to_string(),
            assigned_to: assigned_to.map(|s| s.to_string()),
            title: title.to_string(),
            description: description.to_string(),
            state: TaskState::Pending,
            input,
            output: None,
            error: None,
            dependencies,
            created_at: now.clone(),
            updated_at: now,
        };

        self.tasks.lock().await.insert(task_id.clone(), task);
        self.emit_event(&task_id, "task_created", created_by, None).await;

        tracing::info!(target: "task_store", "Task '{}' created by '{}' (id: {})", title, created_by, task_id);
        task_id
    }

    /// Update task state.
    pub async fn update_state(&self, task_id: &str, agent_id: &str, new_state: TaskState) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.state = new_state.clone();
            task.updated_at = chrono::Utc::now().to_rfc3339();
            let event_type = match new_state {
                TaskState::InProgress => "task_started",
                TaskState::Completed => "task_completed",
                TaskState::Failed => "task_failed",
                TaskState::Cancelled => "task_cancelled",
                TaskState::Pending => "task_reset",
            };
            drop(tasks);
            self.emit_event(task_id, event_type, agent_id, None).await;
            true
        } else {
            false
        }
    }

    /// Set task output (result).
    pub async fn set_output(&self, task_id: &str, agent_id: &str, output: serde_json::Value) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.output = Some(output.clone());
            task.state = TaskState::Completed;
            task.updated_at = chrono::Utc::now().to_rfc3339();
            drop(tasks);
            self.emit_event(task_id, "task_completed", agent_id, Some(output)).await;
            true
        } else {
            false
        }
    }

    /// Set task error.
    pub async fn set_error(&self, task_id: &str, agent_id: &str, error: String) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.error = Some(error.clone());
            task.state = TaskState::Failed;
            task.updated_at = chrono::Utc::now().to_rfc3339();
            drop(tasks);
            self.emit_event(task_id, "task_failed", agent_id, Some(serde_json::json!({"error": error}))).await;
            true
        } else {
            false
        }
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Option<AgentTask> {
        self.tasks.lock().await.get(task_id).cloned()
    }

    /// Get output of a completed task (for dependency resolution).
    pub async fn get_output(&self, task_id: &str) -> Option<serde_json::Value> {
        self.tasks.lock().await.get(task_id)
            .and_then(|t| t.output.clone())
    }

    /// List all tasks, optionally filtered by state.
    pub async fn list_tasks(&self, filter_state: Option<TaskState>) -> Vec<AgentTask> {
        let tasks = self.tasks.lock().await;
        tasks.values()
            .filter(|t| filter_state.as_ref().map_or(true, |s| t.state == *s))
            .cloned()
            .collect()
    }

    /// Get tasks assigned to a specific agent.
    pub async fn get_agent_tasks(&self, agent_id: &str) -> Vec<AgentTask> {
        let tasks = self.tasks.lock().await;
        tasks.values()
            .filter(|t| t.assigned_to.as_deref() == Some(agent_id))
            .cloned()
            .collect()
    }

    /// Check if all dependencies of a task are completed.
    pub async fn are_dependencies_met(&self, task_id: &str) -> bool {
        let tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get(task_id) {
            task.dependencies.iter().all(|dep_id| {
                tasks.get(dep_id)
                    .map(|dep| dep.state == TaskState::Completed)
                    .unwrap_or(false)
            })
        } else {
            false
        }
    }

    /// Get dependency outputs for building context.
    pub async fn get_dependency_outputs(&self, task_id: &str) -> HashMap<String, serde_json::Value> {
        let tasks = self.tasks.lock().await;
        let mut outputs = HashMap::new();
        if let Some(task) = tasks.get(task_id) {
            for dep_id in &task.dependencies {
                if let Some(dep) = tasks.get(dep_id) {
                    if let Some(ref output) = dep.output {
                        outputs.insert(dep_id.clone(), output.clone());
                    }
                }
            }
        }
        outputs
    }

    /// Emit a task event.
    async fn emit_event(&self, task_id: &str, event_type: &str, agent_id: &str, data: Option<serde_json::Value>) {
        let event = TaskEvent {
            task_id: task_id.to_string(),
            event_type: event_type.to_string(),
            agent_id: agent_id.to_string(),
            data,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let _ = self.event_tx.send(event);
    }
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for TaskStore {
    fn clone(&self) -> Self {
        Self {
            tasks: self.tasks.clone(),
            event_tx: self.event_tx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_task() {
        let store = TaskStore::new();
        let id = store.create_task("agent-1", Some("agent-2"), "Test task", "Do something", None, vec![]).await;
        let task = store.get_task(&id).await.unwrap();
        assert_eq!(task.title, "Test task");
        assert_eq!(task.state, TaskState::Pending);
    }

    #[tokio::test]
    async fn test_update_state() {
        let store = TaskStore::new();
        let id = store.create_task("agent-1", None, "Task", "Desc", None, vec![]).await;
        assert!(store.update_state(&id, "agent-1", TaskState::InProgress).await);
        let task = store.get_task(&id).await.unwrap();
        assert_eq!(task.state, TaskState::InProgress);
    }

    #[tokio::test]
    async fn test_set_output() {
        let store = TaskStore::new();
        let id = store.create_task("agent-1", None, "Task", "Desc", None, vec![]).await;
        let output = serde_json::json!({"result": "done"});
        assert!(store.set_output(&id, "agent-1", output).await);
        let task = store.get_task(&id).await.unwrap();
        assert_eq!(task.state, TaskState::Completed);
        assert!(task.output.is_some());
    }

    #[tokio::test]
    async fn test_dependency_check() {
        let store = TaskStore::new();
        let dep_id = store.create_task("agent-1", None, "Dep", "First", None, vec![]).await;
        let task_id = store.create_task("agent-2", None, "Main", "Second", None, vec![dep_id.clone()]).await;

        assert!(!store.are_dependencies_met(&task_id).await);

        store.set_output(&dep_id, "agent-1", serde_json::json!({"done": true})).await;
        assert!(store.are_dependencies_met(&task_id).await);
    }

    #[tokio::test]
    async fn test_list_filter() {
        let store = TaskStore::new();
        store.create_task("a1", None, "T1", "D1", None, vec![]).await;
        let id2 = store.create_task("a2", None, "T2", "D2", None, vec![]).await;
        store.update_state(&id2, "a2", TaskState::Completed).await;

        let all = store.list_tasks(None).await;
        assert_eq!(all.len(), 2);

        let completed = store.list_tasks(Some(TaskState::Completed)).await;
        assert_eq!(completed.len(), 1);
    }
}
