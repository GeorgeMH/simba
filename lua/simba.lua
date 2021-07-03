-- Lua API for the simba environment

local M = {}

function M.init_environment()
    local env = {}
    env.table = table
    env.next = next
    env.type = type
    env.string = string
    env.pairs = pairs
    env.ipairs = ipairs
    env.print = print
    env.math = math
    env.io = io
    env.assert = assert
    env.require = require
    env.package = package
    env.os = os

    env.json = json
    env._G = env

    env.simba = M

    -- overall state for the execution
    env.ctx = {}

    environment = env
end

function M.print_environment()
    for k,v in pairs(_G) do
        print("GLOBAL", k, v)
    end

    for k,v in pairs(environment) do
        print("ENV", k, v)
    end

    for k,v in pairs(environment.ctx) do
        print("CTX", k, v)
    end
end

function M.eval_in_env(code)
    --M.print_environment()

    -- Run the Lua code and get the result.
    func, err = load(code, "eval", "t", environment)
    if not func then
        --print("Failed Executing Code: " .. err)
        return err
    end
    return func()
end

function M.eval_template(template_str)
    return template.compile(template_str, environment)
end

function M.update_context_json(table, table_key, json_str)
    table[table_key] = json.decode(json_str)
end

function M.get_context_json(table, table_key)
    local json = json.encode(table[table_key])
    return json
end

return M
