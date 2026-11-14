//! Opt-in whole-backend Rust allocation profiling; native malloc stays separate.
use serde_json::{Value, json};

#[cfg(all(
    feature = "heap-profile",
    any(target_os = "macos", target_os = "linux")
))]
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub(crate) fn metrics() -> Value {
    match backend::metrics() {
        Ok(value) => value,
        Err(error) => json!({"kind": backend::KIND, "available": false, "error": error}),
    }
}

pub(crate) fn status() -> Result<Value, String> {
    backend::status()
}

pub(crate) fn set_active(active: bool) -> Result<Value, String> {
    backend::set_active(active)?;
    status()
}

pub(crate) fn dump(path: &str) -> Result<Value, String> {
    let started = std::time::Instant::now();
    backend::dump(path)?;
    let bytes = std::fs::metadata(path)
        .map_err(|e| format!("dump metadata: {e}"))?
        .len();
    if bytes == 0 {
        return Err("profiler produced an empty heap dump".into());
    }
    Ok(
        json!({"path": path, "bytes": bytes, "duration_ms": started.elapsed().as_secs_f64() * 1000.0}),
    )
}

#[cfg(all(
    feature = "heap-profile",
    any(target_os = "macos", target_os = "linux")
))]
mod backend {
    use super::*;
    use tikv_jemalloc_ctl::{epoch, raw, stats};

    pub(super) const KIND: &str = "jemalloc (prefixed Rust global allocator)";
    fn error(e: impl std::fmt::Display) -> String {
        format!("jemalloc: {e}")
    }

    pub(super) fn metrics() -> Result<Value, String> {
        epoch::advance().map_err(error)?;
        Ok(json!({
            "kind": KIND, "available": true,
            "scope": "Rust global allocations only; excludes native malloc, mappings and WebContent",
            "allocated_bytes": stats::allocated::read().map_err(error)?,
            "active_bytes": stats::active::read().map_err(error)?,
            "resident_bytes": stats::resident::read().map_err(error)?,
            "retained_bytes": stats::retained::read().map_err(error)?,
        }))
    }

    pub(super) fn status() -> Result<Value, String> {
        // SAFETY: each NUL-terminated mallctl name has the exact documented type.
        unsafe {
            let compiled: bool = raw::read(b"config.prof\0").map_err(error)?;
            let enabled: bool = raw::read(b"opt.prof\0").map_err(error)?;
            if !compiled || !enabled {
                return Err("profiling unavailable: start with _RJEM_MALLOC_CONF=prof:true,prof_active:false (heap-profile build required)".into());
            }
            Ok(json!({
                "allocator": KIND, "compiled": compiled, "enabled": enabled,
                "active": raw::read::<bool>(b"prof.active\0").map_err(error)?,
                "new_threads_active": raw::read::<bool>(b"opt.prof_thread_active_init\0").map_err(error)?,
                "current_thread_active": raw::read::<bool>(b"thread.prof.active\0").map_err(error)?,
                "debug_assertions": raw::read::<bool>(b"config.debug\0").map_err(error)?,
                "sample_interval_bytes": 1u64.checked_shl(raw::read::<usize>(b"prof.lg_sample\0").map_err(error)? as u32),
                "executable": std::env::current_exe().map_err(error)?,
                "pid": std::process::id(),
                "configuration": std::env::var("_RJEM_MALLOC_CONF").ok(),
            }))
        }
    }

    pub(super) fn set_active(active: bool) -> Result<(), String> {
        status()?;
        // SAFETY: prof.active takes a bool and is synchronized by jemalloc.
        unsafe { raw::write(b"prof.active\0", active).map_err(error) }
    }

    pub(super) fn dump(path: &str) -> Result<(), String> {
        status()?;
        let path = std::ffi::CString::new(path).map_err(error)?;
        // SAFETY: prof.dump takes a pointer to a NUL-terminated filename; the
        // CString remains alive for the synchronous mallctl call.
        unsafe { raw::write(b"prof.dump\0", path.as_ptr()).map_err(error) }
    }
}

#[cfg(not(all(
    feature = "heap-profile",
    any(target_os = "macos", target_os = "linux")
)))]
mod backend {
    use super::*;
    pub(super) const KIND: &str = "system";
    pub(super) fn metrics() -> Result<Value, String> {
        Ok(
            json!({"kind": KIND, "available": false, "reason": "jemalloc diagnostic allocator not selected"}),
        )
    }
    pub(super) fn status() -> Result<Value, String> {
        Err("heap profiling unavailable: requires heap-profile feature on macOS or Linux; Windows uses system allocator".into())
    }
    pub(super) fn set_active(_: bool) -> Result<(), String> {
        status().map(|_| ())
    }
    pub(super) fn dump(_: &str) -> Result<(), String> {
        status().map(|_| ())
    }
}

#[cfg(test)]
mod tests;
