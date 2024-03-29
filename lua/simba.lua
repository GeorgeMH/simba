-- Lua API for the simba environment

local M = {}

function M.init_environment()
end

function M.print_environment()
    for k,v in pairs(_G) do
        print("GLOBAL", k, v)
    end

    for k,v in pairs(ctx) do
        print("CTX", k, v)
    end
end

function M.eval_in_env_json(chunkName, code)
    --M.print_environment()
    -- Run the Lua code and get the result.
    local func, err = load(code, chunkName)
    if not func then
        return err
    end
    local result = func()
    return json.encode(result)
end

function M.set_as_json(table, table_key, json_str)
    table[table_key] = json.decode(json_str)
end

function M.get_as_json(table, table_key)
    -- TODO: This works around an issue with the json library converting an empty table to an array instead of empty object
    -- TODO: we need to switch to a better json library or not use json at all
    local ctx_size = 0
    for k,v in pairs(table[table_key]) do
        ctx_size = ctx_size + 1;
    end
    if ctx_size == 0 then
        return "{}"
    end
    local json = json.encode(table[table_key])
    return json
end

return M
