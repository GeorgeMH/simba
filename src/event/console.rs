#![allow(clippy::similar_names)]

use std::collections::HashMap;
use std::sync::Arc;

use console::{pad_str, style, Alignment, Color};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::event::{PipelineEventHandler, TaskUpdate};
use crate::pipeline::{ExecutionResult, NodeId, Pipeline, Stage, StepResult, StepTask};
use async_trait::async_trait;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Clone)]
pub struct ConsoleEventHandler {
    inner: Arc<Inner>,
}

struct Inner {
    print_options: PrintOptions,
    running: AtomicBool,
    multi_progress: MultiProgress,
    state: RwLock<ConsoleEventHandlerState>,
}

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct PrintOptions {
    pub print_execution_tree: bool,
    pub print_request_headers: bool,
    pub print_response_headers: bool,
    pub print_body: bool,
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
    pub progress_bar: ProgressBar,
}

impl ConsoleEventHandler {
    pub fn new(print_options: PrintOptions) -> Self {
        let multi_progress = MultiProgress::new();
        Self {
            inner: Arc::new(Inner {
                print_options,
                running: AtomicBool::new(true),
                multi_progress,
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
        self.inner.running.load(Ordering::SeqCst)
    }

    fn set_running(&self, new_value: bool) {
        self.inner.running.store(new_value, Ordering::SeqCst);
    }

    async fn handle_finished_task(&self, task: &StepTask, step_result: StepResult) {
        let state_guard = self.get_state().await;

        let stage_state = state_guard
            .stage_states
            .get(&task.parent_id)
            .unwrap_or_else(|| panic!("Unknown Stage: {}", task.parent_id));

        let task_state = state_guard
            .task_states
            .get(&task.id)
            .unwrap_or_else(|| panic!("Unknown Task: {}", task.id));

        stage_state.progress_bar.inc(1);

        match step_result.result {
            ExecutionResult::Skipped(skipped_reason) => {
                finish_with_message(
                    &task_state.progress_bar,
                    task,
                    INIT_TICK_STRINGS[0],
                    Color::White,
                    style(skipped_reason.as_str()).white().to_string().as_str(),
                );
            }
            ExecutionResult::Error(error_msg) => {
                finish_with_message(
                    &task_state.progress_bar,
                    task,
                    FINISHED_TICK_STRINGS[0],
                    Color::Red,
                    style(error_msg.as_str()).red().to_string().as_str(),
                );
            }
            ExecutionResult::Response(response) => {
                let (post_script_result_msg, color) = match response.post_script_result {
                    None => ("None".to_string(), Color::Green),
                    Some(true) => ("true".to_string(), Color::Green),
                    Some(false) => ("false".to_string(), Color::Yellow),
                };

                let message = style(format!(
                    "Status: {}, Post Script: {}, {}ms",
                    response.status, post_script_result_msg, response.execution_time_ms
                ))
                .fg(color);

                let mut message = message.to_string();
                let mut additional_msg = String::new();
                if let Some(rendered_step) = &task.rendered_step {
                    if self.inner.print_options.print_request_headers {
                        if let Some(req_headers) = &rendered_step.headers {
                            additional_msg += "Request Headers: \n";
                            for (k, v) in req_headers {
                                additional_msg += format!("{}{}: {}", indent(1), k, v).as_str()
                            }
                        }
                    }
                }

                if self.inner.print_options.print_response_headers {
                    additional_msg += "Response Headers:\n";
                    for (k, v) in &response.headers {
                        additional_msg += format!("{}{}: {}\n", indent(1), k, v).as_str();
                    }
                }

                if self.inner.print_options.print_body {
                    if let Some(body) = response.body_string {
                        additional_msg += "Body:\n";
                        additional_msg += body.as_str();
                    }
                }

                if !additional_msg.is_empty() {
                    message += "\n";
                    message += additional_msg.as_str();
                }

                finish_with_message(
                    &task_state.progress_bar,
                    task,
                    FINISHED_TICK_STRINGS[0],
                    color,
                    message.as_str(),
                );
            }
        }
    }
}

fn finish_with_message(
    progress_bar: &ProgressBar,
    task: &StepTask,
    spinner_str: &str,
    color: Color,
    message: &str,
) {
    let message = style(message);
    let mut prefix = style(pad_str(
        task.step.stage.as_str(),
        12,
        Alignment::Right,
        None,
    ))
    .bold();
    let mut spinner = style(spinner_str).bold();
    prefix = prefix.fg(color);
    spinner = spinner.fg(color);

    let final_string = format!("{} {} - {} - {}", prefix, spinner, task.step.desc, message);

    progress_bar.println(final_string);
    progress_bar.finish_and_clear();
}

fn indent(levels: usize) -> String {
    "    ".repeat(levels)
}

#[async_trait]
impl PipelineEventHandler for ConsoleEventHandler {
    async fn pipeline_init(&self, pipeline: &Pipeline) {
        if self.inner.print_options.print_execution_tree {
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
        if stage.tasks.is_empty() {
            return; // Empty stage
        }
        let mut state = self.get_state_mut().await;

        let mut stage_state = StageState {
            name: stage.name.clone(),
            concurrent: stage.concurrent,
            progress_bar: ProgressBar::new(stage.tasks.len() as u64),
        };

        for task in &stage.tasks {
            let progress_bar = ProgressBar::new_spinner();
            let progress_bar = self.inner.multi_progress.add(progress_bar);
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

        // Add the stage progress bar after all it's child stages
        stage_state.progress_bar = self.inner.multi_progress.add(stage_state.progress_bar);
        update_stage_style(&stage_state, EXECUTING_TICK_STRING_COLOR);

        state.stage_states.insert(stage.id, stage_state);
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

    async fn stage_end(&self, stage: &Stage) {
        if stage.tasks.is_empty() {
            return; // Nothing to do
        }
        let state = self.get_state().await;
        let stage_state = state
            .stage_states
            .get(&stage.id)
            .unwrap_or_else(|| panic!("Unknown Stage: {} {}", stage.id, stage.name));

        stage_state.progress_bar.finish_and_clear();
    }

    async fn pipeline_finish(&self, pipeline: &Pipeline) {
        log::info!("Pipeline {} finished", pipeline.name());
        self.set_running(false);
    }
}

const PB_TICK_INTERVAL_MS: u64 = 80;

async fn spawn_background_tasks(console_output_writer: ConsoleEventHandler) {
    // Create a task that will "tick" each running ProgressBar until all are stopped
    let tick_clone = console_output_writer.clone();
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(PB_TICK_INTERVAL_MS));
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

            for stage_state in state.stage_states.values() {
                if !stage_state.progress_bar.is_finished() {
                    stage_state.progress_bar.tick();
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

fn update_task_style(
    stage_state: &StageState,
    task_state: &ConsoleTaskState,
    step_task: &StepTask,
    tick_strings: &[&str],
    spinner_color: &str,
    msg_color: &str,
) {
    let template_string = &*format!(
        "{{prefix:>12.{}.bold}} {{spinner:.{}}} - {} - {{msg:.{}}}",
        spinner_color, spinner_color, step_task.step.desc, msg_color
    );
    let pb_style = ProgressStyle::default_spinner()
        .tick_strings(tick_strings)
        .template(template_string);
    task_state.progress_bar.set_prefix(stage_state.name.clone());
    task_state.progress_bar.set_style(pb_style);
}

fn update_stage_style(stage_state: &StageState, prefix_color: &str) {
    let pb_style = ProgressStyle::default_spinner()
        .template(&*format!(
            "{{prefix:>12.{}.bold}} [{{bar:57}}] {{pos}}/{{len}} {{msg}}",
            prefix_color
        ))
        .progress_chars("=> ");
    stage_state
        .progress_bar
        .set_prefix(Cow::from(stage_state.name.clone()));
    stage_state.progress_bar.set_style(pb_style);
}
