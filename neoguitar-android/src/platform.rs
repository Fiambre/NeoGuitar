//! Bridge to the Java side (`NeoActivity`): the system file picker and files sent to the app.

use android_activity::AndroidApp;
use jni::objects::{JObject, JString};
use jni::JavaVM;
use std::path::PathBuf;

/// What the file picker (or an "Open with") produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Picked {
    Song(PathBuf),
    SoundFont(PathBuf),
    Lyrics(PathBuf),
    /// Text typed in the native prompt: (field id, text).
    Text(i32, String),
    Cancelled,
    Failed(String),
}

fn with_env<R>(app: &AndroidApp, f: impl FnOnce(&mut jni::JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    match f(&mut env, &activity) {
        Ok(v) => Some(v),
        Err(_) => {
            let _ = env.exception_clear();
            None
        }
    }
}

/// Opens the system picker. `kind`: 0 = song, 1 = SoundFont, 2 = lyrics.
pub fn pick_file(app: &AndroidApp, kind: i32) {
    with_env(app, |env, act| env.call_method(act, "pickFile", "(I)V", &[kind.into()]).map(|_| ()));
}

/// Asks for a line of text with a native dialog (and the system keyboard).
pub fn prompt_text(app: &AndroidApp, id: i32, title: &str, initial: &str) {
    with_env(app, |env, act| {
        let title = env.new_string(title)?;
        let initial = env.new_string(initial)?;
        env.call_method(act, "promptText", "(ILjava/lang/String;Ljava/lang/String;)V", &[id.into(), (&title).into(), (&initial).into()]).map(|_| ())
    });
}

/// The result of the last pick, if one arrived since the previous call.
pub fn take_picked(app: &AndroidApp) -> Option<Picked> {
    let text = with_env(app, |env, act| {
        let obj = env.call_method(act, "takePending", "()Ljava/lang/String;", &[])?.l()?;
        if obj.is_null() {
            return Ok(None);
        }
        let s: String = env.get_string(&JString::from(obj))?.into();
        Ok(Some(s))
    })??;
    let (kind, rest) = text.split_once('|')?;
    if let Some(id) = kind.strip_prefix('t').and_then(|n| n.parse::<i32>().ok()) {
        return Some(Picked::Text(id, rest.to_string()));
    }
    Some(match kind {
        "0" => Picked::Song(rest.into()),
        "1" => Picked::SoundFont(rest.into()),
        "2" => Picked::Lyrics(rest.into()),
        "C" => Picked::Cancelled,
        _ => Picked::Failed(rest.to_string()),
    })
}