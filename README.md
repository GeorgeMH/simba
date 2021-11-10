# SIMBA - Simple API Pipeline Executor

Scriptable API workflows on the command line.

This was primarily a TUI project I hacked around on for a few days as an excuse
to write some rust and embed lua into a native runtime.

## Goals
* Simple file format similar to something like ansible
* LUA script interface
* TUI 

## The Pipeline File

Run the following example pipeline file with:

```shell
cargo run -- -p ./pipeline.yaml
```

### Example Pipeline File

See [pipeline.yaml](pipeline.yaml)