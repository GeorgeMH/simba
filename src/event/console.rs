#![allow(clippy::similar_names)]

use std::collections::HashMap;
use std::sync::Arc;

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::event::{PipelineEventHandler, TaskUpdate};
use crate::pipeline::{ExecutionResult, NodeId, Pipeline, Stage, StepResult, StepTask};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Clone)]
pub struct ConsoleEventHandler {
    inner: Arc<Inner>,
}

struct Inner {
    print_pipeline_tree: bool,
    running: AtomicBool,
    multi_progress: MultiProgress,
    state: RwLock<ConsoleEventHandlerState>,
}

struct ConsoleEventHandlerState {
    pub stage_states: HashMap<NodeId, StageState>,
    pub task_states: HashMap<NodeId, ConsoleTaskState>,
}

struct ConsoleTaskState {
    pub progress_bar: ProgressBar,
}

#[derive(Clone)]
struct StageState {
    pub name: String,
    pub concurrent: bool,
}

impl ConsoleEventHandler {
    pub fn new(print_pipeline_tree: bool) -> Self {
        Self {
            inner: Arc::new(Inner {
                print_pipeline_tree,
                running: AtomicBool::new(true),
                multi_progress: MultiProgress::new(),
                state: RwLock::new(ConsoleEventHandlerState {
                    stage_states: HashMap::new(),
                    task_states: HashMap::new(),
                }),
            }),
        }
    }

    async fn get_state(&self) -> RwLockReadGuard<'_, ConsoleEventHandlerState> {
        self.inner.state.read().await
    }

    async fn get_state_mut(&self) -> RwLockWriteGuard<'_, ConsoleEventHandlerState> {
        self.inner.state.write().await
    }

    fn running(&self) -> bool {
        self.inner.running.load(Ordering::Relaxed)
    }

    fn set_running(&self, new_value: bool) {
        self.inner.running.store(new_value, Ordering::Relaxed);
    }

    async fn handle_finished_task(&self, task: &StepTask, step_result: StepResult) {
        let state_guard = self.get_state().await;

        let stage_state = state_guard
            .stage_states
            .get(&task.parent_id)
            .unwrap_or_else(|| panic!("Unknown Stage: {}", task.parent_id));

        let console_task_state = state_guard
            .task_states
            .get(&task.id)
            .unwrap_or_else(|| panic!("Unknown Task: {}", task.id));

        match step_result.result {
            ExecutionResult::Skipped(skipped_reason) => {
                update_task_style(
                    stage_state,
                    &console_task_state,
                    task,
                    FINISHED_TICK_STRINGS,
                    INIT_TICK_STRING_COLOR,
                    INIT_TICK_STRING_COLOR,
                );

                console_task_state
                    .progress_bar
                    .finish_with_message(skipped_reason);
            }
            ExecutionResult::Error(error_msg) => {
                // TODO: Can we have new lines displayed here?
                let message = error_msg.replace("\n", "\\n");

                update_task_style(
                    stage_state,
                    &console_task_state,
                    task,
                    FINISHED_TICK_STRINGS,
                    ERROR_COLOR,
                    ERROR_COLOR,
                );
                console_task_state.progress_bar.finish_with_message(message);
            }
            ExecutionResult::Response(response) => {
                let (is_success, post_script_result_msg, spinner_color, msg_color) =
                    match response.post_script_result {
                        None => (true, "None".to_string(), SUCCESS_COLOR, SUCCESS_COLOR),
                        Some(result_bool) => {
                            if result_bool {
                                (
                                    result_bool,
                                    result_bool.to_string(),
                                    SUCCESS_COLOR,
                                    SUCCESS_COLOR,
                                )
                            } else {
                                (
                                    result_bool,
                                    result_bool.to_string(),
                                    ERROR_COLOR,
                                    ERROR_COLOR,
                                )
                            }
                        }
                    };

                update_task_style(
                    stage_state,
                    &console_task_state,
                    task,
                    FINISHED_TICK_STRINGS,
                    spinner_color,
                    msg_color,
                );

                let mut message = style(format!(
                    "Status: {}, Post Script: {}",
                    response.status, post_script_result_msg
                ));

                if is_success {
                    message = message.green();
                } else {
                    message = message.red();
                }

                console_task_state
                    .progress_bar
                    .finish_with_message(message.to_string());
            }
        }
    }
}

#[async_trait]
impl PipelineEventHandler for ConsoleEventHandler {
    async fn pipeline_init(&self, pipeline: &Pipeline) {
        if self.inner.print_pipeline_tree {
            println!("Pipeline {}", pipeline.name());
            for stage in pipeline.stages() {
                println!("\tStage: {}, Concurrent: {}", stage.name, stage.concurrent);
                for task in &stage.tasks {
                    println!("\t\tTask: {}", task.step.desc)
                }
            }
        }
        spawn_background_tasks(self.clone()).await;
    }

    async fn stage_start(&self, stage: &Stage) {
        let mut state = self.get_state_mut().await;

        let stage_state = StageState {
            name: stage.name.clone(),
            concurrent: stage.concurrent,
        };
        state.stage_states.insert(stage.id, stage_state.clone());

        for task in &stage.tasks {
            let progress_bar = self.inner.multi_progress.add(ProgressBar::new_spinner());
            progress_bar.set_message("Pending");
            let task_state = ConsoleTaskState { progress_bar };
            update_task_style(
                &stage_state,
                &task_state,
                task,
                INIT_TICK_STRINGS,
                INIT_TICK_STRING_COLOR,
                INIT_TICK_STRING_COLOR,
            );

            state.task_states.insert(task.id, task_state);
        }
    }

    async fn task_update(&self, task: &StepTask, task_update: TaskUpdate) {
        log::info!("Task State: {} - {:?}", task.id, task_update);

        match task_update {
            TaskUpdate::Processing(message) => {
                let state_guard = self.get_state().await;
                let stage_state = state_guard
                    .stage_states
                    .get(&task.parent_id)
                    .unwrap_or_else(|| panic!("Unknown Stage: {}", task.parent_id));
                let console_task_state = state_guard
                    .task_states
                    .get(&task.id)
                    .unwrap_or_else(|| panic!("Unknown Task: {}", task.id));

                update_task_style(
                    stage_state,
                    &console_task_state,
                    task,
                    EXECUTING_TICK_STRING,
                    EXECUTING_TICK_STRING_COLOR,
                    EXECUTING_TICK_STRING_COLOR,
                );

                console_task_state
                    .progress_bar
                    .set_message(format!("Executing: {}", message));
            }
            TaskUpdate::Finished(step_result) => self.handle_finished_task(task, step_result).await,
        }
    }

    async fn stage_end(&self, _stage: &Stage) {
        // let state = self.get_state().await;
    }

    async fn pipeline_finish(&self, pipeline: &Pipeline) {
        log::info!("Pipeline {} finished", pipeline.name());
        self.set_running(false);
    }
}

async fn spawn_background_tasks(console_output_writer: ConsoleEventHandler) {
    // Create a task that will "tick" each running ProgressBar until all are stopped
    let tick_clone = console_output_writer.clone();
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(80));
        loop {
            interval.tick().await;

            if !tick_clone.running() {
                break;
            }

            let state = tick_clone.get_state().await;
            for console_task_state in state.task_states.values() {
                if !console_task_state.progress_bar.is_finished() {
                    console_task_state.progress_bar.tick();
                }
            }
        }
    });

    // `MultiProgress::join` must be called to draw the progress bars
    tokio::task::spawn_blocking(move || loop {
        if !console_output_writer.running() {
            break;
        }
        if let Err(error) = console_output_writer.inner.multi_progress.join() {
            panic!("Failed joining multi-progress bar {}", error);
        }
    });
}

// For more spinners check out the cli-spinners project:
// https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json
const EXECUTING_TICK_STRING_COLOR: &str = "blue";
const EXECUTING_TICK_STRING: &[&str] = &[
    "▱▱▱▱▱▱▱",
    "▰▱▱▱▱▱▱",
    "▰▰▱▱▱▱▱",
    "▰▰▰▱▱▱▱",
    "▰▰▰▰▱▱▱",
    "▰▰▰▰▰▱▱",
    "▰▰▰▰▰▰▱",
    "▰▰▰▰▰▰▰",
    "▱▱▱▱▱▱▱",
];

const INIT_TICK_STRING_COLOR: &str = "gray";
const INIT_TICK_STRINGS: &[&str] = &["▱▱▱▱▱▱▱", "▱▱▱▱▱▱▱"];

const FINISHED_TICK_STRINGS: &[&str] = &["▰▰▰▰▰▰▰", "▰▰▰▰▰▰▰"];

const ERROR_COLOR: &str = "red";
const SUCCESS_COLOR: &str = "green";

fn update_task_style(
    stage_state: &StageState,
    task_state: &ConsoleTaskState,
    step_task: &StepTask,
    tick_strings: &[&str],
    spinner_color: &str,
    msg_color: &str,
) {
    let pb_style = ProgressStyle::default_spinner()
        .tick_strings(tick_strings)
        .template(&*format!(
            "{{spinner:.{}}} - Stage[{},{}] - {} - {{msg:.{}}}",
            spinner_color, stage_state.name, stage_state.concurrent, step_task.step.desc, msg_color
        ));
    task_state.progress_bar.set_style(pb_style);
}
