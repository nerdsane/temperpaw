//! One concern: execute the bound DSF operation. IOA owns the lifecycle.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run(_ctx_ptr: i32, _ctx_len: i32) -> i32 {
    dsf_operation_common::guest::run(dsf_operation_common::execute, "ExecutionUncertain")
}
