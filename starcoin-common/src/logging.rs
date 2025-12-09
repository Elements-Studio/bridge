// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::in_test_configuration;
use once_cell::sync::Lazy;

#[macro_export]
macro_rules! fatal {
    ($msg:literal $(, $arg:expr)*) => {{
        if $crate::in_antithesis() {
            let full_msg = format!($msg $(, $arg)*);
            let json = $crate::logging::json!({ "message": full_msg });
            $crate::logging::assert_unreachable_antithesis!($msg, &json);
        }
        tracing::error!(fatal = true, $msg $(, $arg)*);
        panic!($msg $(, $arg)*);
    }};
}

pub use antithesis_sdk::assert_reachable as assert_reachable_antithesis;
pub use antithesis_sdk::assert_unreachable as assert_unreachable_antithesis;

pub use serde_json::json;

#[inline(always)]
pub fn crash_on_debug() -> bool {
    static CRASH_ON_DEBUG: Lazy<bool> = Lazy::new(|| {
        in_test_configuration() || std::env::var("STARCOIN_ENABLE_DEBUG_ASSERTIONS").is_ok()
    });

    *CRASH_ON_DEBUG
}

#[macro_export]
macro_rules! register_debug_fatal_handler {
    ($message:literal, $f:expr) => {
        // silence unused variable warnings from the body of the callback
        let _ = $f;
    };
}

#[macro_export]
macro_rules! debug_fatal {
    ($msg:literal $(, $arg:expr)*) => {{
        // In antithesis, rather than crashing, we will use the assert_unreachable_antithesis
        // macro to catch the signal that something has gone wrong.
        if !$crate::in_antithesis() && $crate::logging::crash_on_debug() {
            $crate::fatal!($msg $(, $arg)*);
        } else {
            let stacktrace = std::backtrace::Backtrace::capture();
            tracing::error!(debug_fatal = true, stacktrace = ?stacktrace, $msg $(, $arg)*);
            let location = concat!(file!(), ':', line!());
            if let Some(metrics) = starcoin_metrics::get_metrics() {
                metrics.system_invariant_violations.with_label_values(&[location]).inc();
            }
            if $crate::in_antithesis() {
                // antithesis requires a literal for first argument. pass the formatted argument
                // as a string.
                let full_msg = format!($msg $(, $arg)*);
                let json = $crate::logging::json!({ "message": full_msg });
                $crate::logging::assert_unreachable_antithesis!($msg, &json);
            }
        }
    }};
}

#[macro_export]
macro_rules! assert_reachable {
    () => {
        $crate::logging::assert_reachable!("");
    };
    ($message:literal) => {{
        $crate::logging::assert_reachable_antithesis!($message);
    }};
}

mod tests {
    #[test]
    #[should_panic]
    fn test_fatal() {
        fatal!("This is a fatal error");
    }

    #[test]
    #[should_panic]
    fn test_debug_fatal() {
        if cfg!(debug_assertions) {
            debug_fatal!("This is a debug fatal error");
        } else {
            // pass in release mode as well
            fatal!("This is a fatal error");
        }
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_debug_fatal_release_mode() {
        debug_fatal!("This is a debug fatal error");
    }
}
