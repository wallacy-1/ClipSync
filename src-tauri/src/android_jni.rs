//! Ponte JNI mínima Rust → Kotlin (só Android). Reutilizada pelo keystore
//! (`KeyGuard`) e pelo teclado (`ImeBridge`). Threads nativas usam o class
//! loader do sistema, por isso a classe do app é carregada pelo loader do
//! `Context` que o runtime do Tauri (tao) guarda.
use crate::error::{AppError, Result};
use jni::objects::{JClass, JObject, JValue};
use jni::{JNIEnv, JavaVM};

/// Executa `f` com um `JNIEnv` anexado, a classe `class` (nome com pontos)
/// já carregada e o `Context` da Activity principal.
pub fn with_class<T>(class: &str, f: impl FnOnce(&mut JNIEnv, &JClass, &JObject) -> Result<T>) -> Result<T> {
    let ctx = tauri::tao::platform::android::prelude::main_android_context()
        .ok_or_else(|| AppError::Other("contexto Android indisponível".into()))?;
    // SAFETY: ponteiros fornecidos pelo runtime e válidos durante toda a vida do processo.
    let vm = unsafe { JavaVM::from_raw(ctx.java_vm.cast()) }.map_err(|e| AppError::Other(e.to_string()))?;
    let mut env = vm.attach_current_thread().map_err(|e| AppError::Other(e.to_string()))?;
    let context = unsafe { JObject::from_raw(ctx.context_jobject.cast()) };

    let loader = env
        .call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .and_then(|v| v.l())
        .map_err(|e| AppError::Other(format!("getClassLoader: {e}")))?;
    let name = env.new_string(class).map_err(|e| AppError::Other(e.to_string()))?;
    let cls = env
        .call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])
        .and_then(|v| v.l())
        .map_err(|e| AppError::Other(format!("loadClass {class}: {e}")))?;
    let cls = JClass::from(cls);

    let out = f(&mut env, &cls, &context);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err(AppError::Other(format!("{class}: exceção Java")));
    }
    out
}

pub fn jstring_to_string(env: &mut JNIEnv, obj: JObject) -> Result<String> {
    let js = jni::objects::JString::from(obj);
    let s: String = env.get_string(&js).map_err(|e| AppError::Other(e.to_string()))?.into();
    Ok(s)
}
