//! Lua-scripted macro expansions ("especially LuaJIT" per the owner): a
//! `[normalize.macros]` expansion value prefixed with `lua:` is executed
//! as a LuaJIT script instead of being inserted literally -- see
//! `macros::expand_macros`, which strips the sentinel and calls
//! `run_script` here with the matched trigger text.
//!
//! Sandboxed: only `string`/`table`/`math` and a trimmed `os` (date/time
//! only -- `execute`/`exit`/`remove`/`rename`/`tmpname`/`getenv`/
//! `setlocale` are stripped) are loaded. No `io`, no `package`/`require`,
//! no `debug`, no LuaJIT `ffi`. A bounded instruction count also guards
//! against a runaway script (`while true do end`) hanging the pipeline.
//!
//! Any failure -- parse error, runtime error, a non-string return, or
//! hitting the instruction budget -- degrades gracefully: `run_script`
//! returns `None` and logs via `tracing::warn!`. A broken macro script must
//! never fail (or hang) a dictation turn; the caller falls back to leaving
//! the trigger text unexpanded.

use mlua::{Lua, LuaOptions, StdLib, Value};

/// Instruction budget before a script is aborted as runaway.
const MAX_INSTRUCTIONS: u32 = 1_000_000;

/// `os` table members that reach outside the sandbox (process control,
/// environment, filesystem paths) and are stripped after `os` is loaded,
/// leaving the read-only convenience functions (`os.date`, `os.time`,
/// `os.clock`, `os.difftime`) usable.
const UNSAFE_OS_FNS: &[&str] = &[
    "execute", "exit", "remove", "rename", "tmpname", "getenv", "setlocale",
];

/// Globals nilled out defensively even though the stdlibs that normally
/// define them (`package`, `debug`) are never opened -- cheap insurance
/// against a future stdlib change accidentally reintroducing a loader or
/// LuaJIT's `ffi`.
const BLOCKED_GLOBALS: &[&str] = &[
    "load",
    "loadstring",
    "dofile",
    "loadfile",
    "require",
    "package",
    "ffi",
    "debug",
];

/// Runs `script` (Lua source, sentinel already stripped) with the matched
/// trigger text bound to the global `arg`, returning its `return`ed
/// string. `None` on any failure -- see the module doc comment for what
/// counts as failure and why this never propagates as an error.
pub(super) fn run_script(script: &str, arg: &str) -> Option<String> {
    let lua = match sandboxed_lua() {
        Ok(lua) => lua,
        Err(e) => {
            tracing::warn!("lua macro: failed to create sandbox: {e}");
            return None;
        }
    };

    if let Err(e) = lua.globals().set("arg", arg) {
        tracing::warn!("lua macro: failed to bind arg: {e}");
        return None;
    }

    match lua.load(script).eval::<Value>() {
        Ok(Value::String(s)) => match s.to_str() {
            Ok(s) => Some(s.to_string()),
            Err(e) => {
                tracing::warn!("lua macro: script returned non-UTF-8 string: {e}");
                None
            }
        },
        Ok(other) => {
            tracing::warn!("lua macro: script returned a non-string value ({other:?}), ignoring");
            None
        }
        Err(e) => {
            tracing::warn!("lua macro: script failed: {e}");
            None
        }
    }
}

/// A fresh LuaJIT state with only `string`/`table`/`math`/(trimmed) `os`
/// loaded, plus an instruction-count hook that aborts a runaway script.
fn sandboxed_lua() -> mlua::Result<Lua> {
    let safe_libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::OS;
    let lua = Lua::new_with(safe_libs, LuaOptions::default())?;

    if let Ok(Value::Table(os)) = lua.globals().get("os") {
        for name in UNSAFE_OS_FNS {
            os.set(*name, Value::Nil)?;
        }
    }
    for name in BLOCKED_GLOBALS {
        lua.globals().set(*name, Value::Nil)?;
    }

    let triggers = mlua::HookTriggers::new().every_nth_instruction(MAX_INSTRUCTIONS);
    lua.set_hook(triggers, |_lua, _debug| {
        Err(mlua::Error::RuntimeError(
            "macro script exceeded its instruction budget".to_string(),
        ))
    });

    Ok(lua)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirms_luajit_backend() {
        // Unrestricted state purely to prove the compiled backend is
        // LuaJIT (the `jit` table only exists there), independent of
        // whatever libs the sandbox itself chooses to load.
        let lua = Lua::new();
        let version: String = lua.load("return jit.version").eval().unwrap();
        assert!(
            version.contains("LuaJIT"),
            "expected a LuaJIT version string, got {version:?}"
        );
    }

    #[test]
    fn returns_a_literal_string() {
        assert_eq!(run_script("return 'hi'", ""), Some("hi".to_string()));
    }

    #[test]
    fn transforms_the_matched_argument() {
        assert_eq!(
            run_script("return string.upper(arg or '')", "shout"),
            Some("SHOUT".to_string())
        );
    }

    #[test]
    fn os_date_is_available() {
        // The example from the design doc: date/time functions stay usable.
        let result = run_script("return type(os.date('%Y'))", "");
        assert_eq!(result, Some("string".to_string()));
    }

    #[test]
    fn broken_script_degrades_to_none() {
        assert_eq!(run_script("this is not lua", ""), None);
    }

    #[test]
    fn non_string_return_degrades_to_none() {
        assert_eq!(run_script("return 42", ""), None);
    }

    #[test]
    fn sandbox_blocks_os_execute() {
        assert_eq!(run_script("return os.execute('true')", ""), None);
    }

    #[test]
    fn sandbox_blocks_io() {
        assert_eq!(run_script("return io.open('/etc/passwd')", ""), None);
    }

    #[test]
    fn sandbox_blocks_require_and_ffi() {
        assert_eq!(run_script("return require('os')", ""), None);
        assert_eq!(run_script("return ffi.new('int[1]')", ""), None);
    }

    #[test]
    fn instruction_budget_stops_an_infinite_loop() {
        assert_eq!(run_script("while true do end", ""), None);
    }
}
