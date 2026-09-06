//! SDK entry boundary. The caller supplies one integration function.
use crate::*;
use temper_wasm_sdk::{Context, set_error_result, set_success_result};

/// Temper SDK-backed host capabilities.
pub struct Guest(Context);
impl Host for Guest {
    fn request(&mut self, request: &Request) -> Result<Response, Error> {
        self.0
            .http_call(
                request.method,
                &request.url,
                &request.headers,
                &request.body,
            )
            .map(|r| Response {
                status: r.status,
                body: r.body,
            })
            .map_err(|_| Error::Transport)
    }
    fn secret(&mut self, name: &str) -> Result<String, Error> {
        identifier(name)?;
        self.0.get_secret(name).map_err(|_| Error::Transport)
    }
}

/// Invoke one adapter and emit its declared callback through the WASM ABI.
pub fn run(
    adapter: impl FnOnce(&mut Runtime<Guest>, &Operation) -> Result<Callback, Error>,
    failure: &str,
) -> i32 {
    let mut operation_key = String::new();
    let result = (|| {
        let ctx = Context::from_host().map_err(|_| Error::Response("invocation context"))?;
        let operation = Operation::parse(&ctx.entity_id, &ctx.entity_state)?;
        operation_key = operation.key.clone();
        let base = ctx
            .config
            .get("temper_api_url")
            .cloned()
            .ok_or(Error::Binding("missing Temper URL"))?;
        let key = ctx
            .config
            .get("temper_api_key")
            .cloned()
            .ok_or(Error::Binding("missing Temper credential"))?;
        let tenant = ctx.tenant.clone();
        let mut guest = Guest(ctx);
        adapter(
            &mut Runtime {
                host: &mut guest,
                base: &base,
                tenant: &tenant,
                key: &key,
                now_ms: Context::get_time_millis(),
            },
            &operation,
        )
    })();
    match result {
        Ok(callback) => set_success_result(callback.action, &callback.params),
        Err(Error::Blocked(ask_id)) if failure == "ValidationFailed" => set_success_result(
            "ValidationBlocked",
            &json!({"operation_key":operation_key,"ask_id":ask_id,"error_message":"linked required decision is unresolved or denied"}),
        ),
        Err(Error::ProviderFailed(reason)) if failure == "VerificationPending" => {
            set_success_result("VerificationFailed", &json!({"error_message":reason}))
        }
        Err(error) => set_error_result(&error.to_string()),
    }
    0
}
