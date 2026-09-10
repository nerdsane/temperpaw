//! Read-only Effort gates for exact resource-owned delivery operations.
use dsf_resource_common::*;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(target_arch = "wasm32")]
pub mod guest;
const MANIFEST: &str = include_str!("../../../../dsf-factory/specs/module-contracts.json");
const MAX_OPERATIONS: usize = 8;

#[derive(Clone)]
pub struct Binding {
    pub effort_id: String,
    pub plan: String,
    pub head: String,
    pub sequence: u64,
}
impl Binding {
    pub fn parse(id: &str, row: &Value) -> Result<Self, Error> {
        identifier(id)?;
        let head = required(row, "resource_delivery_head")?.to_string();
        if !full_sha(&head) {
            return Err(Error::Binding(
                "delivery head must be an exact source revision",
            ));
        }
        let sequence = field(row, "delivery_sequence")
            .and_then(Value::as_u64)
            .filter(|sequence| *sequence > 0)
            .ok_or(Error::Binding("delivery sequence"))?;
        let plan = required(row, "resource_delivery_plan")?.to_string();
        parse_plan(&plan)?;
        Ok(Self {
            effort_id: id.into(),
            plan,
            head,
            sequence,
        })
    }
    fn current(&self, runtime: &mut Runtime<impl Host>) -> Result<Value, Error> {
        let row = runtime.row("Efforts", &self.effort_id)?;
        if required(&row, "resource_delivery_plan")? != self.plan
            || required(&row, "resource_delivery_head")? != self.head
            || field(&row, "delivery_sequence").and_then(Value::as_u64) != Some(self.sequence)
            || field(&row, "deploy_configured") == Some(&Value::Bool(true))
        {
            return Err(Error::Binding("Effort delivery invocation changed"));
        }
        Ok(row)
    }
    pub fn callback(&self, action: &str) -> Callback {
        Callback {
            action: action.into(),
            params: json!({"expected_delivery_plan":self.plan,"expected_delivery_head":self.head,"expected_delivery_sequence":self.sequence}),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    operations: Vec<Expected>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    entity_type: String,
    resource_id: String,
    action: String,
    operation_key: String,
    operation_sequence: u64,
    revision: String,
    configuration_sha256: String,
    proof_ref: String,
}
#[derive(Deserialize)]
struct Manifest {
    resources: BTreeMap<String, Resource>,
    callback_shapes: BTreeMap<String, BTreeMap<String, String>>,
}
#[derive(Deserialize)]
struct Resource {
    entity_set: String,
    verification_flags: BTreeMap<String, String>,
    modules: BTreeMap<String, Module>,
}
#[derive(Deserialize)]
struct Module {
    callbacks: BTreeMap<String, String>,
}
struct Checked {
    expected: Expected,
    entity_set: String,
    flag: String,
    verified_field: &'static str,
}
fn parse_plan(raw: &str) -> Result<Vec<Checked>, Error> {
    if raw.len() > 32768 {
        return Err(Error::Binding("delivery plan exceeds byte limit"));
    }
    let plan: Plan =
        serde_json::from_str(raw).map_err(|_| Error::Binding("invalid delivery plan"))?;
    if plan.operations.is_empty() || plan.operations.len() > MAX_OPERATIONS {
        return Err(Error::Binding(
            "delivery plan needs one to eight resource operations",
        ));
    }
    let manifest: Manifest = serde_json::from_str(MANIFEST)
        .map_err(|_| Error::Binding("invalid generated resource contract"))?;
    let mut unique = BTreeSet::new();
    let mut checked = Vec::new();
    for operation in plan.operations {
        identifier(&operation.resource_id)?;
        identifier(&operation.operation_key)?;
        identifier(&operation.proof_ref)?;
        if !unique.insert((operation.entity_type.clone(), operation.resource_id.clone()))
            || operation.operation_sequence == 0
            || operation.configuration_sha256.len() != 64
            || !operation
                .configuration_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(Error::Binding(
                "delivery operations need unique resources and exact sequence/configuration identity",
            ));
        }
        let resource = manifest
            .resources
            .get(&operation.entity_type)
            .ok_or(Error::Binding("unsupported resource type"))?;
        let flag = resource
            .verification_flags
            .get(&operation.action)
            .ok_or(Error::Binding("unsupported resource action"))?;
        let callback = format!("{}VerificationSucceeded", operation.action);
        let shape = resource
            .modules
            .values()
            .find_map(|module| module.callbacks.get(&callback))
            .and_then(|name| manifest.callback_shapes.get(name))
            .ok_or(Error::Binding("resource verification callback missing"))?;
        let verified_field = if shape.contains_key("verified_configuration") {
            "verified_configuration"
        } else if shape.contains_key("verified_revision") {
            "verified_revision"
        } else {
            return Err(Error::Binding("resource verification binding missing"));
        };
        if (!operation.revision.is_empty() && !full_sha(&operation.revision))
            || (verified_field == "verified_revision" && !full_sha(&operation.revision))
        {
            return Err(Error::Binding("resource revision must be exact"));
        }
        checked.push(Checked {
            expected: operation,
            entity_set: resource.entity_set.clone(),
            flag: flag.clone(),
            verified_field,
        });
    }
    Ok(checked)
}
fn proof(
    runtime: &mut Runtime<impl Host>,
    binding: &Binding,
    operation: &Expected,
) -> Result<(), Error> {
    let packet = runtime.row("ProofPackets", &operation.proof_ref)?;
    if required(&packet, "effort_id")? != binding.effort_id {
        return Err(Error::Binding("resource proof belongs to another Effort"));
    }
    chain_proof_ready::proof_packet_holds(&packet, Some(&binding.head)).map_err(Error::Proof)?;
    let artifact_ref = required(&packet, "artifact_ref")?;
    if required(&runtime.row("Files", artifact_ref)?, "status")? != "Ready" {
        return Err(Error::Binding("resource proof artifact is not ready"));
    }
    let artifact = json_body(
        runtime.read("Files", artifact_ref, true)?,
        "resource change proof",
    )?;
    let change = artifact
        .get("resource_change")
        .ok_or(Error::Binding("resource change proof binding missing"))?;
    for (name, expected) in [
        ("resource_id", operation.resource_id.as_str()),
        ("entity_type", operation.entity_type.as_str()),
        ("action", operation.action.as_str()),
        ("operation_key", operation.operation_key.as_str()),
        ("revision", operation.revision.as_str()),
        (
            "configuration_sha256",
            operation.configuration_sha256.as_str(),
        ),
    ] {
        if change.get(name).and_then(Value::as_str) != Some(expected) {
            return Err(Error::Binding(
                "proof identifies another resource operation",
            ));
        }
    }
    if change.get("operation_sequence").and_then(Value::as_u64)
        != Some(operation.operation_sequence)
    {
        return Err(Error::Binding(
            "proof identifies another operation sequence",
        ));
    }
    Ok(())
}
fn validate_operations(
    runtime: &mut Runtime<impl Host>,
    binding: &Binding,
    effort: &Value,
) -> Result<(), Error> {
    let attached = field(effort, "proof_packet_ids")
        .map(|_| decoded(effort, "proof_packet_ids"))
        .transpose()?
        .unwrap_or(json!([]));
    let attached = attached
        .as_array()
        .ok_or(Error::Binding("Effort proof IDs must be a list"))?;
    for checked in parse_plan(&binding.plan)? {
        if field(effort, "proof_packet_id").and_then(Value::as_str)
            != Some(&checked.expected.proof_ref)
            && !attached
                .iter()
                .any(|id| id.as_str() == Some(&checked.expected.proof_ref))
        {
            return Err(Error::Binding(
                "resource proof is not attached to the Effort",
            ));
        }
        let row = runtime.row(&checked.entity_set, &checked.expected.resource_id)?;
        if required(&row, "status")? == "Retired" {
            return Err(Error::Binding("delivery resource is retired"));
        }
        if !decoded(&row, "allowed_operations")?
            .as_array()
            .is_some_and(|operations| {
                operations
                    .iter()
                    .any(|value| value.as_str() == Some(&checked.expected.action))
            })
        {
            return Err(Error::Binding("resource does not allow the planned action"));
        }
        proof(runtime, binding, &checked.expected)?;
    }
    Ok(())
}
pub fn validate(runtime: &mut Runtime<impl Host>, binding: &Binding) -> Result<Callback, Error> {
    let row = binding.current(runtime)?;
    if !["Building", "InReview", "Proving", "Merged"].contains(&required(&row, "status")?) {
        return Err(Error::Binding(
            "Effort cannot configure delivery in this state",
        ));
    }
    validate_operations(runtime, binding, &row)?;
    Ok(binding.callback("ResourceDeliveryConfigured"))
}
pub fn merge(runtime: &mut Runtime<impl Host>, binding: &Binding) -> Result<Callback, Error> {
    let row = binding.current(runtime)?;
    if required(&row, "status")? != "ResourceMergeChecking"
        || required(&row, "head_sha")? != binding.head
    {
        return Err(Error::Binding("Effort is not checking this merge"));
    }
    for flag in [
        "resource_delivery_configured",
        "review_passed",
        "evaluation_passed",
        "proof_attached",
        "e2e_ok",
        "decisions_file_ready",
        "merge_risk_clear",
    ] {
        if field(&row, flag) != Some(&Value::Bool(true)) {
            return Err(Error::Binding("Effort merge gate is not satisfied"));
        }
    }
    let ids = decoded(&row, "review_run_ids")?;
    let ids = ids
        .as_array()
        .filter(|ids| !ids.is_empty() && ids.len() <= 16)
        .ok_or(Error::Binding("bounded review panel IDs required"))?;
    let mut reviews = Vec::new();
    for id in ids {
        reviews.push(runtime.row(
            "ReviewRuns",
            id.as_str().ok_or(Error::Binding("review ID"))?,
        )?);
    }
    let packet = runtime.row("ProofPackets", required(&row, "proof_packet_id")?)?;
    chain_merge_ready::review_panel_holds(&reviews, Some(&binding.head)).map_err(Error::Proof)?;
    chain_merge_ready::proof_packet_holds(&packet, Some(&binding.head)).map_err(Error::Proof)?;
    validate_operations(runtime, binding, &row)?;
    Ok(binding.callback("ResourceDeliveryMerged"))
}
pub fn verify(runtime: &mut Runtime<impl Host>, binding: &Binding) -> Result<Callback, Error> {
    let effort = binding.current(runtime)?;
    if required(&effort, "status")? != "ResourceVerifying"
        || required(&effort, "head_sha")? != binding.head
        || field(&effort, "resource_delivery_merged") != Some(&Value::Bool(true))
    {
        return Err(Error::Binding(
            "Effort is not verifying merged resource delivery",
        ));
    }
    let mut evidence = Vec::new();
    for checked in parse_plan(&binding.plan)? {
        let operation = &checked.expected;
        let row = runtime.row(&checked.entity_set, &operation.resource_id)?;
        if required(&row, "status")? != "Active"
            || field(&row, "operation_verified") != Some(&Value::Bool(true))
            || field(&row, &checked.flag) != Some(&Value::Bool(true))
        {
            return Err(Error::Pending("resource operation is not verified"));
        }
        for (name, expected) in [
            ("effort_id", binding.effort_id.as_str()),
            ("operation_key", operation.operation_key.as_str()),
            ("request_revision", operation.revision.as_str()),
            ("proof_ref", operation.proof_ref.as_str()),
            ("verified_resource_id", operation.resource_id.as_str()),
        ] {
            if field(&row, name).and_then(Value::as_str) != Some(expected) {
                return Err(Error::Binding("resource completed a different operation"));
            }
        }
        if field(&row, "operation_sequence").and_then(Value::as_u64)
            != Some(operation.operation_sequence)
        {
            return Err(Error::Binding("resource operation sequence differs"));
        }
        let configuration = required(&row, "request_configuration")?;
        if format!("{:x}", Sha256::digest(configuration.as_bytes()))
            != operation.configuration_sha256
        {
            return Err(Error::Binding("resource configuration differs"));
        }
        let verified = required(&row, checked.verified_field)?;
        if verified
            != if checked.verified_field == "verified_configuration" {
                configuration
            } else {
                operation.revision.as_str()
            }
        {
            return Err(Error::Binding("verified resource value differs"));
        }
        evidence.push(json!({"entity_type":operation.entity_type,"resource_id":operation.resource_id,"action":operation.action,"operation_key":operation.operation_key,"operation_sequence":operation.operation_sequence,"resource_ref":format!("{}/tdata/{}('{}')",runtime.base,checked.entity_set,operation.resource_id),"provider_evidence_ref":required(&row,"provider_evidence_ref")?,"flow_evidence_ref":required(&row,"flow_evidence_ref")?,"telemetry_evidence_ref":required(&row,"telemetry_evidence_ref")?}));
    }
    let mut callback = binding.callback("ResourceDeliveryVerified");
    callback.params["resource_delivery_evidence"] =
        json!(serde_json::to_string(&evidence).map_err(|_| Error::Response("delivery evidence"))?);
    Ok(callback)
}
#[cfg(test)]
mod tests;
