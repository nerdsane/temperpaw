//! Read-only Effort resource delivery merge gate.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run(_ctx_ptr: i32, _ctx_len: i32) -> i32 {
    effort_resource_delivery::guest::run(effort_resource_delivery::merge, "ResourceDeliveryMergeRejected")
}
