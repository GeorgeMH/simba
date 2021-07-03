use crate::Result;
use linked_hash_map::LinkedHashMap;
use rlua::{Context, Function, Lua, Result as LuaResult, Table};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::{RwLock, RwLockReadGuard};

const LUA_TEMPLATE_ENGINE_MOD: &str = include_str!("../lua/templates.lua");
const LUA_JSON_MOD: &str = include_str!("../lua/json.lua");
const LUA_SIMBA_MOD: &str = include_str!("../lua/simba.lua");

const SIMBA: &str = "simba";

const ENVIRONMENT_NAME: &str = "environment";

pub(crate) struct LuaContext {
    pub lua: RwLock<Lua>,
}

impl LuaContext {
    pub fn new() -> Result<LuaContext> {
        let lua = rlua::Lua::new();

        lua.context(|ctx| {
            let globals = ctx.globals();
            load_lua_module(ctx, &globals, "template", LUA_TEMPLATE_ENGINE_MOD)?;
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

    pub async fn context<F, R>(&self, f: F) -> Result<R>
        where
            F: FnOnce(Context) -> Result<R>
    {
        let foo = tokio::task::block_in_place(move || {
            let lua = self.get_lua();
            Ok(lua.context(f)?)
        });
        foo
    }

    ///
    /// Evaluates the given lua code in the SIMBA context.
    ///
    pub async fn evaluate_bool(&self, lua_code: &str) -> Result<bool> {
        self.context(|ctx| {
            let eval_in_env: Function = get_simba_function(&ctx.globals(), "eval_in_env")?;
            Ok(eval_in_env.call(lua_code)?)
        }).await
    }

    ///
    /// Writes the given key value pair to the `ctx` table in rlua
    ///
    pub async fn set_on_context<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.context(
            move |ctx| {
                let globals = ctx.globals();
                let update_context_json: Function = get_simba_function(&globals, "update_context_json")?;

                let environment: Table = globals.get(ENVIRONMENT_NAME)?;
                let env_context: Table = environment.get("ctx")?;

                let json = serde_json::to_string_pretty(value)
                    .map_err(|err| rlua::Error::RuntimeError(format!("{}", err)))?;

                update_context_json.call((env_context, key, json))?;
                Ok(())
            }
        ).await
    }

    ///
    /// Returns the value of the specified key on the `ctx` table or an error if it can't be converted
    /// to type T.
    ///
    pub async fn _get_from_context<T: DeserializeOwned>(&self, key: &str) -> Result<T> {
        self.context(|ctx| {
            let globals = ctx.globals();
            let get_context_json: Function = get_simba_function(&globals, "get_context_json")?;

            let environment: Table = globals.get(ENVIRONMENT_NAME)?;
            let env_context: Table = environment.get("ctx")?;

            let json: String = get_context_json.call((env_context, key))?;

            let result: T = serde_json::from_str(json.as_str())
                .map_err(|err| rlua::Error::RuntimeError(format!("{}", err)))?;

            Ok(result)
        }).await
    }

    ///
    /// Executes an ordered map of variable name to lua templates. The result of each template
    /// is assigned to the `ctx` table in rlua.
    ///
    pub async fn execute_named_templates(&self, scripts: &LinkedHashMap<String, String>) -> Result<()> {
        self.context(|ctx| {
            let globals = ctx.globals();
            let environment: Table = globals.get(ENVIRONMENT_NAME)?;
            let context: Table = environment.get("ctx")?;
            for (key, script) in scripts {
                let value = execute_lua_template(ctx, &script)?;
                log::debug!("Environment Update {}={}", key, value);
                context.set(key.as_str(), value.as_str())?;
            }
            Ok(())
        }).await
    }

    ///
    /// Executes the given lua template and returns the result as a string.
    ///
    pub async fn execute_template(&self, lua_template: &str) -> Result<String> {
        self.context(|ctx| {
            let result = execute_lua_template(ctx, lua_template)?;
            Ok(result)
        }).await
    }
}

fn execute_lua_template(ctx: Context, lua_template: &str) -> LuaResult<String> {
    let eval_template: Function = get_simba_function(&ctx.globals(), "eval_template")?;
    let rendered_string: String = eval_template.call(lua_template)?;
    log::trace!("Rendered String: {}={}", lua_template, rendered_string);
    LuaResult::Ok(rendered_string)
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
