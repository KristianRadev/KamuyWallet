//! # Kamuy Task Runner
//! 
//! Autonomous task execution with checkpointing and auto-recovery.
//! Agents can be aborted/restarted but work continues until complete.

use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Task that can be resumed after interruption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<TaskStep>,
    pub current_step: usize,
    pub completed_steps: Vec<usize>,
    pub created_at: u64,
    pub updated_at: u64,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub id: String,
    pub description: String,
    pub completed: bool,
    pub output_file: Option<String>,
}

/// Task runner that persists state and auto-recovers
pub struct TaskRunner {
    tasks_dir: PathBuf,
    current_task: Option<Task>,
}

impl TaskRunner {
    pub fn new() -> Self {
        let tasks_dir = dirs::data_dir()
            .map(|d| d.join("kamuy").join("tasks"))
            .unwrap_or_else(|| PathBuf::from(".kamuy/tasks"));
        
        // Ensure directory exists
        let _ = fs::create_dir_all(&tasks_dir);
        
        Self {
            tasks_dir,
            current_task: None,
        }
    }
    
    /// Create a new task
    pub fn create_task(&self, name: &str, description: &str, steps: Vec<TaskStep>) -> Task {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Task {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            current_step: 0,
            completed_steps: vec![],
            created_at: now,
            updated_at: now,
            status: TaskStatus::Pending,
            steps,
        }
    }
    
    /// Save task to disk
    pub fn save_task(&self, task: &Task) {
        let path = self.tasks_dir.join(format!("{}.json", task.id));
        if let Ok(content) = serde_json::to_string_pretty(task) {
            let _ = fs::write(path, content);
        }
    }
    
    /// Load task from disk
    pub fn load_task(&self, id: &str) -> Option<Task> {
        let path = self.tasks_dir.join(format!("{}.json", id));
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(task) = serde_json::from_str(&content) {
                    return Some(task);
                }
            }
        }
        None
    }
    
    /// Get all pending/in-progress tasks
    pub fn get_active_tasks(&self) -> Vec<Task> {
        let mut tasks = vec![];
        if let Ok(entries) = fs::read_dir(&self.tasks_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(task) = serde_json::from_str::<Task>(&content) {
                            if task.status == TaskStatus::Pending || task.status == TaskStatus::InProgress {
                                tasks.push(task);
                            }
                        }
                    }
                }
            }
        }
        tasks
    }
    
    /// Resume any incomplete tasks (for auto-recovery on startup)
    pub fn resume_pending(&self) -> Option<Task> {
        let active = self.get_active_tasks();
        active.into_iter().find(|t| t.status != TaskStatus::Completed)
    }
    
    /// Mark a step complete
    pub fn complete_step(&mut self, task: &mut Task, step_id: &str) {
        for (i, step) in task.steps.iter().enumerate() {
            if step.id == step_id && !task.completed_steps.contains(&i) {
                task.completed_steps.push(i);
                task.current_step = i + 1;
                task.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                task.status = if task.current_step >= task.steps.len() {
                    TaskStatus::Completed
                } else {
                    TaskStatus::InProgress
                };
                self.save_task(task);
                break;
            }
        }
    }
    
    /// Get current step description
    pub fn get_current_step(&self, task: &Task) -> Option<&TaskStep> {
        task.steps.get(task.current_step)
    }
}

impl Default for TaskRunner {
    fn default() -> Self {
        Self::new()
    }
}