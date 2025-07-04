use candid::Decode;
use ic_base_types::CanisterId;
use ic_config::subnet_config::SubnetConfig;
use ic_config::{execution_environment::Config as HypervisorConfig, flag_status::FlagStatus};
use ic_registry_subnet_type::SubnetType;
use ic_state_machine_tests::{
    ErrorCode, StateMachine, StateMachineBuilder, StateMachineConfig, UserError, WasmResult,
};
use ic_types::Cycles;
use serde::Deserialize;

/// This number should not exceed the length of the canister output queue,
/// which is currently 500.
pub const CANISTERS_PER_BATCH: u64 = 490;

thread_local! {
    // Store test canister Wasm in a thread local memory to avoid re-reading the file for each bench.
    static TEST_CANISTER_WASM: Vec<u8> = canister_test::Project::cargo_bin_maybe_from_env("test_canister", &[]).bytes();
}

pub fn test_canister_wasm() -> Vec<u8> {
    TEST_CANISTER_WASM.with(|wasm| wasm.clone())
}

pub fn env() -> StateMachine {
    let hypervisor_config = HypervisorConfig {
        rate_limiting_of_heap_delta: FlagStatus::Disabled,
        ..Default::default()
    };
    StateMachineBuilder::new()
        .with_config(Some(StateMachineConfig::new(
            SubnetConfig::new(SubnetType::Application),
            hypervisor_config,
        )))
        .with_checkpoints_enabled(false)
        .with_subnet_type(SubnetType::Application)
        .with_snapshot_download_enabled(true)
        .with_snapshot_upload_enabled(true)
        .build()
}

pub fn setup() -> (StateMachine, CanisterId) {
    let env = env();
    let test_canister = env
        .install_canister_with_cycles(
            test_canister_wasm(),
            vec![],
            None,
            Cycles::new(u128::MAX / 2),
        )
        .unwrap();

    (env, test_canister)
}

pub fn expect_reply<T>(result: Result<WasmResult, UserError>) -> T
where
    T: for<'de> Deserialize<'de> + candid::CandidType,
{
    match result {
        Ok(wasm_result) => match wasm_result {
            WasmResult::Reply(bytes) => Decode!(&bytes, T).unwrap(),
            WasmResult::Reject(msg) => panic!("Unexpected reject: {}", msg),
        },
        Err(err) => panic!("Unexpected error: {}", err),
    }
}

pub fn expect_error(
    result: Result<WasmResult, UserError>,
    error_code: ErrorCode,
    partial_message: &str,
) {
    match result {
        Ok(wasm_result) => match wasm_result {
            WasmResult::Reply(bytes) => panic!("Unexpected reply: {bytes:?}"),
            WasmResult::Reject(msg) => panic!("Unexpected reject: {}", msg),
        },
        Err(err) => {
            assert_eq!(err.code(), error_code);
            assert!(
                err.description().contains(partial_message),
                "Actual: {}",
                err.description()
            );
        }
    }
}
