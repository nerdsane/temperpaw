use super::*;
use temper_wasm_sdk::{Context, set_error_result, set_success_result};
pub struct Guest(Context);
impl Host for Guest {
    fn request(&mut self, request: &Request) -> Result<Response, Error> {
        if request.method != "GET" {
            return Err(Error::Binding("delivery gate only reads records"));
        }
        self.0
            .http_call(
                request.method,
                &request.url,
                &request.headers,
                &request.body,
            )
            .map(|response| Response {
                status: response.status,
                body: response.body,
            })
            .map_err(|_| Error::Transport)
    }
    fn secret(&mut self, _: &str) -> Result<String, Error> {
        Err(Error::Binding("delivery gate has no provider credentials"))
    }
}
pub fn run(
    adapter: impl FnOnce(&mut Runtime<Guest>, &Binding) -> Result<Callback, Error>,
    failure: &str,
) -> i32 {
    let mut captured = None;
    let result = (|| {
        let ctx = Context::from_host().map_err(|_| Error::Response("Effort invocation context"))?;
        if ctx.entity_type.rsplit('.').next() != Some("Effort") {
            return Err(Error::Binding("delivery gate belongs to Effort"));
        }
        let binding = Binding::parse(&ctx.entity_id, &ctx.entity_state)?;
        captured = Some(binding.clone());
        let base = ctx
            .config
            .get("temper_api_url")
            .cloned()
            .ok_or(Error::Binding("missing Temper URL"))?;
        let tenant = ctx.tenant.clone();
        adapter(
            &mut Runtime {
                host: &mut Guest(ctx),
                base: &base,
                tenant: &tenant,
                now_ms: Context::get_time_millis(),
            },
            &binding,
        )
    })();
    match result {
        Ok(callback) => set_success_result(&callback.action, &callback.params),
        Err(_) => match captured {
            Some(binding) => {
                let mut callback = binding.callback(failure);
                callback.params["error_message"] = json!(
                    "Resource delivery records are unavailable, incomplete, or differ from the accepted plan; inspect the linked resources and retry this check."
                );
                set_success_result(&callback.action, &callback.params);
            }
            None => set_error_result(
                "Effort delivery invocation could not be decoded; no uncorrelated callback emitted",
            ),
        },
    }
    0
}
