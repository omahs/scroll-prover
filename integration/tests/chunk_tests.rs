use integration::test_util::{load_block_traces_for_test, ASSETS_DIR, PARAMS_DIR};
use prover::{utils::init_env_and_log, zkevm::Prover};

#[cfg(feature = "prove_verify")]
#[test]
fn test_chunk_prove_verify() {
    let output_dir = init_env_and_log("chunk_tests");
    log::info!("Initialized ENV and created output-dir {output_dir}");

    let chunk_trace = load_block_traces_for_test().1;
    log::info!("Loaded chunk trace");

    let mut zkevm_prover = Prover::from_dirs(PARAMS_DIR, ASSETS_DIR);
    log::info!("Constructed zkevm prover");

    for _i in 0..20 {
        zkevm_prover
            .gen_chunk_proof(chunk_trace.clone(), None, None, None)
            .unwrap();
    }
}
