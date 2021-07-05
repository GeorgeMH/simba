#![allow(clippy::similar_names)]

use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::output::{PipelineEventHandler, TaskUpdate};
use crate::pipeline::{ExecutionResult, Pipeline, Stage, StepTask};
use std::time::Duration;
use async_trait::async_trait;

#[derive(Clone)]
pub struct ConsoleWriter {
    inner: Arc<ConsoleWriterInner>,
}

struct ConsoleWriterInner {
    state: RwLock<ConsoleWriterState>,
}

struct ConsoleWriterState {
    pub multi_progress: Arc<MultiProgress>,
    pub task_states: HashMap<u64, ConsoleTaskState>,
}

struct ConsoleTaskState {
    pub progress_bar: ProgressBar,
}

impl ConsoleWriter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ConsoleWriterInner {
                state: RwLock::new(ConsoleWriterState {
                    multi_progress: Arc::new(MultiProgress::new()),
                    task_states: HashMap::new(),
                }),
            }),
        }
    }

    fn get_state(&self) -> RwLockReadGuard<ConsoleWriterState> {
        self.inner
            .state
            .read()
            .expect("Failed acquiring output read lock")
    }

    fn get_state_mut(&self) -> RwLockWriteGuard<ConsoleWriterState> {
        self.inner
            .state
            .write()
            .expect("Failed acquiring state write lock")
    }
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
    progress_bar: &ProgressBar,
    step_task: &StepTask,
    tick_strings: &[&str],
    spinner_color: &str,
    msg_color: &str,
) {
    let pb_style = ProgressStyle::default_spinner()
        .tick_strings(tick_strings)
        .template(&*format!(
            "\t{{spinner:.{}}} - {} - {{msg:.{}}}",
            spinner_color, step_task.step.desc, msg_color
        ));
    progress_bar.set_style(pb_style);
}

fn update_stage_style(
    progress_bar: &ProgressBar,
    stage: &Stage,
    tick_strings: &[&str],
    _spinner_color: &str,
    _msg_color: &str,
) {
    let pb_style = ProgressStyle::default_spinner()
        .tick_strings(tick_strings)
        .template(&*format!(
            // "{{bar:.{}}} - Stage: {} - {{msg:.{}}}",
            "{} {{bar:20.green/yellow}} {{pos}}/{{len}}",
            stage.name() // spinner_color, stage.id(), msg_color
        ));
    progress_bar.set_style(pb_style);
}

#[async_trait]
impl PipelineEventHandler for ConsoleWriter {
    async fn pipeline_init(&self, pipeline: &Pipeline) {
        let mut state = self.get_state_mut();

        state.multi_progress = Arc::new(MultiProgress::new());

        for stage in pipeline.stages() {
            let stage_progress_bar = state
                .multi_progress
                .add(ProgressBar::new(stage.tasks().len() as u64));
            update_stage_style(
                &stage_progress_bar,
                stage,
                INIT_TICK_STRINGS,
                INIT_TICK_STRING_COLOR,
                INIT_TICK_STRING_COLOR,
            );
            stage_progress_bar.set_message("Pending");
            state.task_states.insert(
                stage.id(),
                ConsoleTaskState {
                    progress_bar: stage_progress_bar,
                },
            );

            for task in stage.tasks() {
                let progress_bar = state.multi_progress.add(ProgressBar::new_spinner());
                update_task_style(
                    &progress_bar,
                    task,
                    INIT_TICK_STRINGS,
                    INIT_TICK_STRING_COLOR,
                    INIT_TICK_STRING_COLOR,
                );

                progress_bar.set_message("Pending");

                state
                    .task_states
                    .insert(task.id, ConsoleTaskState { progress_bar });
            }
        }

        // Create a task that will "tick" each running ProgressBar until all are stopped
        let tick_clone = self.clone();
        tokio::task::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(80));
            loop {
                interval.tick().await;
                let state = tick_clone.get_state();
                let mut all_finished = true;
                for console_task_state in state.task_states.values() {
                    if !console_task_state.progress_bar.is_finished() {
                        console_task_state.progress_bar.tick();
                        all_finished = false;
                    }
                }
                if all_finished {
                    log::info!("All progress bars finished, stopping tick");
                    break;
                }
            }
        });

        // `MultiProgress::join` must be called to draw the progress bars
        let self_clone = self.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(error) = self_clone.get_state().multi_progress.join() {
                panic!("Failed joining multi-progress bar {}", error);
            }
        });
    }

    async fn stage_start(&self, stage: &Stage) {
        let state = self.get_state();
        let console_task_state = state
            .task_states
            .get(&stage.id())
            .unwrap_or_else(|| panic!("Unknown Stage: {}", stage.id()));

        update_stage_style(
            &console_task_state.progress_bar,
            stage,
            EXECUTING_TICK_STRING,
            EXECUTING_TICK_STRING_COLOR,
            EXECUTING_TICK_STRING_COLOR,
        );
    }

    async fn task_update(&self, task: &StepTask, task_update: TaskUpdate) {
        log::info!("Task State: {} - {:?}", task.id, task_update);

        match task_update {
            TaskUpdate::Processing(message) => {
                let state_guard = self.get_state();
                let console_task_state = state_guard
                    .task_states
                    .get(&task.id)
                    .unwrap_or_else(|| panic!("Unknown Task: {}", task.id));

                update_task_style(
                    &console_task_state.progress_bar,
                    task,
                    EXECUTING_TICK_STRING,
                    EXECUTING_TICK_STRING_COLOR,
                    EXECUTING_TICK_STRING_COLOR,
                );

                console_task_state
                    .progress_bar
                    .set_message(format!("Executing: {}", message));
            }
            TaskUpdate::Finished(step_result) => {
                let state_guard = self.get_state();

                let stage_task_state =
                    state_guard
                        .task_states
                        .get(&task.parent_id)
                        .unwrap_or_else(|| {
                            panic!("Unknown Parent {} on Task: {}", task.parent_id, task.id)
                        });
                stage_task_state.progress_bar.inc(1);

                let console_task_state = state_guard
                    .task_states
                    .get(&task.id)
                    .unwrap_or_else(|| panic!("Unknown Task: {}", task.id));

                match step_result.result {
                    ExecutionResult::Skipped(skipped_reason) => {
                        update_task_style(
                            &console_task_state.progress_bar,
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
                            &console_task_state.progress_bar,
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
                            &console_task_state.progress_bar,
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
    }

    async fn stage_end(&self, stage: &Stage) {
        log::info!("Stage End: {}", stage.id());
        let state = self.get_state();
        log::info!("Got State for Stage End");
        let console_task_state = state
            .task_states
            .get(&stage.id())
            .unwrap_or_else(|| panic!("Unknown Task: {}", stage.id()));
        update_stage_style(
            &console_task_state.progress_bar,
            stage,
            FINISHED_TICK_STRINGS,
            SUCCESS_COLOR,
            SUCCESS_COLOR,
        );
        console_task_state
            .progress_bar
            .finish_with_message("Finished");
    }

    async fn pipeline_finish(&self, pipeline: &Pipeline) {
        log::info!("Pipeline {} finished", pipeline.name())
    }
}
