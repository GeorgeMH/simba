use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use tokio::task::JoinHandle;


use crate::output::PipelineEventHandler;
use crate::pipeline::{ExecutionResult, Pipeline, Stage, StepTask, TaskState};

#[derive(Clone)]
pub struct ConsoleWriter {
    inner: Arc<ProgressBarWriterInner>,
}

struct ProgressBarWriterInner {
    state: RwLock<ConsoleWriterState>,
}

struct ConsoleWriterState {
    pub multi_progress: Arc<MultiProgress>,
    pub tick_handle: Option<JoinHandle<()>>,
    pub task_states: HashMap<u64, ConsoleTaskState>,
}

struct ConsoleTaskState {
    pub progress_bar: ProgressBar,
}

impl ConsoleWriter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ProgressBarWriterInner {
                state: RwLock::new(ConsoleWriterState {
                    multi_progress: Arc::new(MultiProgress::new()),
                    tick_handle: None,
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
    _msg_color: &str
) {
    let pb_style = ProgressStyle::default_spinner()
        .tick_strings(tick_strings)
        .template(&*format!(
            // "{{bar:.{}}} - Stage: {} - {{msg:.{}}}",
            "{} {{bar:20.green/yellow}} {{pos}}/{{len}}",
            stage.name()
            // spinner_color, stage.id(), msg_color
        ));
    progress_bar.set_style(pb_style);
}

impl PipelineEventHandler for ConsoleWriter {
    fn pipeline_init(&self, pipeline: &Pipeline) {
        let mut state = self.get_state_mut();

        state.multi_progress = Arc::new(MultiProgress::new());

        for stage in pipeline.stages() {
            let stage_progress_bar = state.multi_progress.add(ProgressBar::new(stage.tasks().len() as u64));
            // stage_progress_bar.enable_steady_tick(80);
            update_stage_style(&stage_progress_bar, stage, INIT_TICK_STRINGS, INIT_TICK_STRING_COLOR, INIT_TICK_STRING_COLOR);
            stage_progress_bar.set_message("Pending");
            state.task_states.insert(stage.id(), ConsoleTaskState { progress_bar: stage_progress_bar });

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
                progress_bar.enable_steady_tick(80);

                state
                    .task_states
                    .insert(task.id, ConsoleTaskState { progress_bar });
            }
        }

        let self_clone = self.clone();
        log::info!("Foo");
        let join_handle = tokio::task::spawn_blocking(move || {
            log::info!("MultiProgress.join start");
            let _result = self_clone.get_state().multi_progress.join();
            log::info!("MultiProgress.join end");
        });
        log::info!("Bar");
        state.tick_handle = Some(join_handle);
    }

    fn stage_start(&self, stage: &Stage) {
        let state = self.get_state();
        let console_task_state = state
            .task_states
            .get(&stage.id())
            .unwrap_or_else(|| panic!("Unknown Stage: {}", stage.id()));
        update_stage_style(&console_task_state.progress_bar, stage, EXECUTING_TICK_STRING, EXECUTING_TICK_STRING_COLOR, EXECUTING_TICK_STRING_COLOR);
    }

    fn task_update(&self, task: &StepTask, task_state: TaskState) {
        log::info!("Task State: {} - {:?}", task.id, task_state);

        match task_state {
            TaskState::Pending => {
                todo!("Yeah")
            }
            TaskState::Executing(message) => {
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
            TaskState::Result(step_result) => {
                let state_guard = self.get_state();

                let stage_task_state = state_guard
                    .task_states
                    .get(&task.parent_id)
                    .unwrap_or_else(|| panic!("Unknown Parent {} on Task: {}", task.parent_id, task.id));
                stage_task_state.progress_bar.inc(1);

                let console_task_state = state_guard
                    .task_states
                    .get(&task.id)
                    .unwrap_or_else(|| panic!("Unknown Task: {}", task.id));
                console_task_state.progress_bar.disable_steady_tick();

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

    fn stage_end(&self, stage: &Stage) {
        log::info!("Stage End: {}", stage.id());
        let state = self.get_state();
        log::info!("Got State for Stage End");
        let console_task_state = state
            .task_states
            .get(&stage.id())
            .unwrap_or_else(|| panic!("Unknown Task: {}", stage.id()));
        update_stage_style(&console_task_state.progress_bar, stage, FINISHED_TICK_STRINGS, SUCCESS_COLOR, SUCCESS_COLOR);
        console_task_state.progress_bar.finish_with_message("Finished");
    }

    fn pipeline_finish(&self, pipeline: &Pipeline) {
        log::info!("Pipeline {} finished", pipeline.name())
    }
}
