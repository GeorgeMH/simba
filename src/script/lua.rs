use crate::script::{ScriptContext, ScriptEngine, ScriptResponse};
use crate::Result;
use async_trait::async_trait;

use rlua::{Context, Function, Lua, Result as LuaResult, Table};

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard};

#[derive(Debug)]
pub enum LuaEvent {
    Execute {
        context: ScriptContext,
        code: String,
        response: tokio::sync::oneshot::Sender<ScriptResponse>,
    },
}

#[derive(Clone)]
pub struct LuaScriptEngine {
    inner: Arc<LuaInner>,
}

struct LuaInner {
    sender: std::sync::mpsc::SyncSender<LuaEvent>,
}

impl LuaScriptEngine {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(10_0000);
        start_lua_event_processor(receiver);

        Ok(Self {
            inner: Arc::new(LuaInner { sender }),
        })
    }
}

#[async_trait]
impl ScriptEngine for LuaScriptEngine {
    async fn execute(&self, context: ScriptContext, code: &str) -> Result<ScriptResponse> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.inner.sender.send(LuaEvent::Execute {
            context,
            code: code.to_string(),
            response: sender,
        })?;

        let lua_response = receiver.await?;
        Ok(lua_response)
    }
}

fn start_lua_event_processor(receiver: std::sync::mpsc::Receiver<LuaEvent>) {
    std::thread::spawn(move || {
        if let Err(error) = lua_event_processor(&receiver) {
            log::error!("Failed processing lua events {}", error);
        }
    });
}

fn lua_event_processor(receiver: &std::sync::mpsc::Receiver<LuaEvent>) -> Result<()> {
    let lua_context = LuaContext::new()?;
    loop {
        let event = receiver.recv()?;
        log::info!("Lua Event: {:?}", event);

        match event {
            LuaEvent::Execute {
                context,
                code,
                response,
            } => {
                let response_result = lua_context.evaluate(&context, code.as_str());
                handle_script_response_result(response, context, response_result);
            }
        };
    }
}

fn handle_script_response_result(
    sender: tokio::sync::oneshot::Sender<ScriptResponse>,
    context: ScriptContext,
    response: Result<ScriptResponse>,
) {
    let final_response = match response {
        Err(error) => ScriptResponse {
            context,
            value: Err(error),
        },
        Ok(response) => response,
    };
    if let Err(failed_to_send) = sender.send(final_response) {
        panic!("Failed sending script response: {:?}", failed_to_send)
    }
}

const LUA_JSON_MOD: &str = include_str!("../../lua/json.lua");
const LUA_SIMBA_MOD: &str = include_str!("../../lua/simba.lua");

const SIMBA: &str = "simba";

const ENVIRONMENT_NAME: &str = "environment";

pub(crate) struct LuaContext {
    lua: RwLock<Lua>,
}

impl LuaContext {
    pub fn new() -> Result<LuaContext> {
        let lua = rlua::Lua::new();

        lua.context(|ctx| {
            let globals = ctx.globals();
            load_lua_module(ctx, &globals, "json", LUA_JSON_MOD)?;
            load_lua_module(ctx, &globals, "simba", LUA_SIMBA_MOD)?;
            init_environment(ctx)?;

            LuaResult::Ok(())
        })?;

        Ok(LuaContext {
            lua: RwLock::new(lua),
        })
    }

    fn get_lua(&self) -> RwLockReadGuard<Lua> {
        self.lua.read().expect("Failed acquiring Lua Read Lock")
    }

    pub fn evaluate(
        &self,
        script_context: &ScriptContext,
        lua_code: &str,
    ) -> Result<ScriptResponse> {
        let lua = self.get_lua();
        Ok(lua.context(|ctx| {
            set_script_context(ctx, &script_context)?;

            let eval_in_env: Function = get_simba_function(&ctx.globals(), "eval_in_env_json")?;
            let json_str: String = eval_in_env.call(lua_code)?;

            let script_context = get_script_context(ctx)?;

            let value: Value = serde_json::from_str(json_str.as_str()).map_err(|err| {
                rlua::Error::RuntimeError(format!("Evaluate JsonError: {}\n{}", err, json_str))
            })?;

            LuaResult::Ok(ScriptResponse {
                context: script_context,
                value: Ok(value),
            })
        })?)
    }
}

fn set_script_context(ctx: Context, script_context: &ScriptContext) -> LuaResult<()> {
    let globals = ctx.globals();
    let update_context_json: Function = get_simba_function(&globals, "set_as_json")?;
    let environment: Table = globals.get(ENVIRONMENT_NAME)?;
    let json = serde_json::to_string_pretty(&script_context.data)
        .map_err(|err| rlua::Error::RuntimeError(format!("SetScriptContext Json {}", err)))?;
    update_context_json.call((environment, "ctx", json))?;
    LuaResult::Ok(())
}

fn get_script_context(ctx: Context) -> LuaResult<ScriptContext> {
    let globals = ctx.globals();
    let update_context_json: Function = get_simba_function(&globals, "get_as_json")?;
    let environment: Table = globals.get(ENVIRONMENT_NAME)?;
    let json: String = update_context_json.call((environment, "ctx"))?;
    let script_context_data: HashMap<String, Value> =
        serde_json::from_str(&json).map_err(|err| {
            rlua::Error::RuntimeError(format!("GetScriptContextJson {}\n{}", err, json))
        })?;
    LuaResult::Ok(ScriptContext {
        data: script_context_data,
    })
}

fn load_lua_module<'lua>(
    ctx: Context<'lua>,
    table: &Table<'lua>,
    name: &str,
    lua: &str,
) -> LuaResult<()> {
    let module_function = ctx.load(lua).into_function()?;

    // Most modules as function evaluate to a Table
    let module_table: Table = module_function.call(())?;

    // Update the Table with the new module
    table.set(name, module_table)?;
    LuaResult::Ok(())
}

fn init_environment(ctx: Context) -> LuaResult<Table> {
    let globals = ctx.globals();
    let init_environment: Function = get_simba_function(&globals, "init_environment")?;

    init_environment.call(())?;
    let environment: Table = globals.get(ENVIRONMENT_NAME)?;
    LuaResult::Ok(environment)
}

fn get_simba_function<'lua>(
    globals: &Table<'lua>,
    function_name: &str,
) -> LuaResult<Function<'lua>> {
    let simba_tbl: Table = globals.get(SIMBA)?;
    simba_tbl.get(function_name)
}
