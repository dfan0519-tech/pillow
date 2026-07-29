//! Shared adversarial adapters for runtime-host integration tests.
//!
//! Each integration-test crate only uses a subset of these helpers, so unused
//! items are expected when this module is compiled into a given binary.

#![allow(dead_code)]
#![allow(unused_imports)]

mod adapters;

pub use adapters::{
    BlockingArmAdapter, BlockingSyncSendAdapter, BlockingSyncSendShutdownErrorsAdapter,
    CancelUnblocksAdapter, ShutdownErrorsAdapter,
};
