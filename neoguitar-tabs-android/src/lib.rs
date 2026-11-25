//! NeoGuitar Tabs (Android companion). The only native code: one JNI function that runs a `gprotab` plugin
//! command (`info`, `search <text> [page]`, `download <id> <dir>`) and returns its JSON line. The Java side
//! (`TabsActivity`) draws the screen and, once a tab is downloaded, hands it to NeoGuitar.
#![cfg(target_os = "android")]

use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::jstring;
use jni::JNIEnv;

fn read_args(env: &mut JNIEnv, args: &JObjectArray) -> jni::errors::Result<Vec<String>> {
    let n = env.get_array_length(args)?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let obj = env.get_object_array_element(args, i)?;
        let s: String = env.get_string(&JString::from(obj))?.into();
        out.push(s);
    }
    Ok(out)
}

fn error_json(msg: &str) -> String {
    let escaped: String = msg
        .chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            '\n' | '\r' => vec![' '],
            c => vec![c],
        })
        .collect();
    format!("{{\"error\":\"{escaped}\"}}")
}

fn run(args: &[String]) -> String {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => return error_json(&format!("runtime: {e}")),
    };
    rt.block_on(gprotab::plugin::run_json(&refs)).0
}

/// `static native String run(String[] args)` in `com.fiambre.neoguitar.tabs.Native`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_fiambre_neoguitar_tabs_Native_run<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, args: JObjectArray<'l>) -> jstring {
    let out = match read_args(&mut env, &args) {
        Ok(a) => std::panic::catch_unwind(|| run(&a)).unwrap_or_else(|_| error_json("the plugin crashed")),
        Err(e) => error_json(&format!("bad arguments: {e}")),
    };
    env.new_string(out).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}
