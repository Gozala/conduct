// Copyright 2026 The Dialog DB Project
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The clock of the target the demo runs on: the browser's, through the global object so that the page and the
//! service worker share them, and natively the process's, where the
//! host framework's tests run.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::sync::OnceLock;
    use std::time::Instant;

    /// Nanoseconds since the first reading in this process.
    pub fn now_nanos() -> u64 {
        static ORIGIN: OnceLock<Instant> = OnceLock::new();
        let elapsed = ORIGIN.get_or_init(Instant::now).elapsed();
        u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use wasm_bindgen::{JsCast, JsValue};

    /// Nanoseconds in a millisecond, the unit of the browser's clock.
    const NANOS_PER_MILLI: f64 = 1_000_000.0;

    /// Nanoseconds since the time origin of the context, from
    /// `performance.now()`.
    pub fn now_nanos() -> u64 {
        let millis = global_method("performance", "now")
            .and_then(|(performance, now)| now.call0(&performance).ok())
            .and_then(|reading| reading.as_f64())
            .unwrap_or_else(js_sys::Date::now);
        // A float-to-integer cast saturates, which is what a reading past
        // the range of `u64` should do.
        (millis * NANOS_PER_MILLI) as u64
    }

    /// The function `name` of the global's property `object`, or of the
    /// global itself when `object` is empty, with the value to call it
    /// on.
    fn global_method(object: &str, name: &str) -> Option<(JsValue, js_sys::Function)> {
        let mut this: JsValue = js_sys::global().into();
        if !object.is_empty() {
            this = js_sys::Reflect::get(&this, &JsValue::from_str(object)).ok()?;
        }
        let function =
            js_sys::Reflect::get(&this, &JsValue::from_str(name)).ok()?.dyn_into::<js_sys::Function>().ok()?;
        Some((this, function))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::now_nanos;
#[cfg(target_arch = "wasm32")]
pub use web::now_nanos;

/// Milliseconds since the time origin of the context.
pub fn now_millis() -> f64 {
    now_nanos() as f64 / 1_000_000.0
}

/// `error` with each of its causes, as one line of text: the polyfill
/// puts a trap's own message, such as `wasm trap: null reference`, in a
/// cause.
pub fn describe(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

/// Milliseconds since the Unix epoch, by the wall clock, which the page
/// and the service worker share.
pub fn wall_millis() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
}
