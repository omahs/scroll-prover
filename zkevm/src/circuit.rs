use anyhow::bail;
use halo2_proofs::halo2curves::bn256::Fr;
use halo2_proofs::plonk::Circuit as Halo2Circuit;

use mpt_circuits::{hash::Hashable, operation::AccountOp, EthTrie, EthTrieCircuit, HashCircuit};

use once_cell::sync::Lazy;

use strum::IntoEnumIterator;
use types::eth::BlockTrace;

use zkevm_circuits::evm_circuit::test::get_test_degree as evm_circuit_get_test_degree;
use zkevm_circuits::evm_circuit::witness::Block;
use zkevm_circuits::evm_circuit::EvmCircuit as EvmCircuitImpl;
use zkevm_circuits::state_circuit::StateCircuit as StateCircuitImpl;
use zkevm_circuits::super_circuit::SuperCircuit as SuperCircuitImpl;

mod builder;
mod mpt;

use crate::utils::read_env_var;

use self::builder::{block_trace_to_witness_block, block_traces_to_witness_block};

const MAX_TXS: usize = 25;
const MAX_CALLDATA: usize = 400_000;
const MAX_RWS: usize = 500_000;
//pub static MAX_TXS: Lazy<usize> = Lazy::new(|| read_env_var("MAX_TXS", 15));
//pub static MAX_RWS: Lazy<usize> = Lazy::new(|| read_env_var("MAX_RWS", 500_000));
pub static DEGREE: Lazy<usize> = Lazy::new(|| read_env_var("DEGREE", 19));
pub static AGG_DEGREE: Lazy<usize> = Lazy::new(|| read_env_var("AGG_DEGREE", 26));

pub trait TargetCircuit {
    type Inner: Halo2Circuit<Fr>;
    fn name() -> String;
    /// used to generate vk&pk
    fn empty() -> Self::Inner
    where
        Self: Sized,
    {
        Self::from_block_traces(&[]).unwrap().0
    }
    fn from_block_trace(block_trace: &BlockTrace) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        Self::from_block_traces(std::slice::from_ref(block_trace))
    }
    fn from_block_traces(
        block_traces: &[BlockTrace],
    ) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized;

    fn estimate_rows(_block_traces: &[BlockTrace]) -> usize {
        0
    }
    fn public_input_len() -> usize {
        0
    }
    // It is usually safer to use MockProver::verify than MockProver::verify_at_rows
    fn get_active_rows(block_traces: &[BlockTrace]) -> (Vec<usize>, Vec<usize>) {
        (
            (0..Self::estimate_rows(block_traces)).into_iter().collect(),
            (0..Self::estimate_rows(block_traces)).into_iter().collect(),
        )
    }
}

pub struct SuperCircuit {}

impl TargetCircuit for SuperCircuit {
    type Inner = SuperCircuitImpl<Fr, MAX_TXS, MAX_CALLDATA, MAX_RWS>;

    fn name() -> String {
        "super".to_string()
    }

    fn from_block_traces(block_traces: &[BlockTrace]) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        let witness_block = block_traces_to_witness_block(block_traces)?;
        let (k, inner, instance) = Self::Inner::build_from_witness_block(witness_block)?;
        if k as usize > *DEGREE {
            bail!(
                "circuit not enough: DEGREE = {}, less than k needed: {}",
                *DEGREE,
                k
            );
        }
        debug_assert_eq!(instance.len(), 1);
        Ok((inner, instance))
    }

    fn estimate_rows(block_traces: &[BlockTrace]) -> usize {
        let witness_block = block_traces_to_witness_block(block_traces).unwrap();
        // evm only now
        Self::Inner::get_num_rows_required(&witness_block)
    }

    fn public_input_len() -> usize {
        1
    }
}

pub struct EvmCircuit {}

impl TargetCircuit for EvmCircuit {
    type Inner = EvmCircuitImpl<Fr>;

    fn name() -> String {
        "evm".to_string()
    }

    fn from_block_trace(block_trace: &BlockTrace) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        let witness_block = block_trace_to_witness_block(block_trace)?;
        let inner = EvmCircuitImpl::<Fr>::new(witness_block);
        let instance = vec![];
        Ok((inner, instance))
    }

    fn from_block_traces(block_traces: &[BlockTrace]) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        let witness_block = block_traces_to_witness_block(block_traces)?;
        let inner = EvmCircuitImpl::<Fr>::new(witness_block);
        let instance = vec![];
        Ok((inner, instance))
    }

    fn estimate_rows(block_traces: &[BlockTrace]) -> usize {
        match block_traces_to_witness_block(block_traces) {
            Ok(witness_block) => {
                evm_circuit_get_test_degree(&witness_block);
                EvmCircuitImpl::<Fr>::get_num_rows_required(&witness_block)
            }
            Err(e) => {
                log::error!("convert block result to witness block failed: {:?}", e);
                0
            }
        }
    }
}

pub struct StateCircuit {}
impl TargetCircuit for StateCircuit {
    type Inner = StateCircuitImpl<Fr>;

    fn name() -> String {
        "state".to_string()
    }

    // TODO: use from_block_trace(&Default::default()) ?
    fn empty() -> Self::Inner {
        StateCircuitImpl::<Fr>::default()
    }

    fn from_block_trace(block_trace: &BlockTrace) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        Self::from_block_traces(std::slice::from_ref(block_trace))
    }

    fn from_block_traces(block_traces: &[BlockTrace]) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        let witness_block = block_traces_to_witness_block(block_traces)?;
        let inner = StateCircuitImpl::<Fr>::new(
            witness_block.rws,
            // TODO: put it into CircuitParams?
            (1 << *DEGREE) - 64,
        );
        let instance = vec![];
        Ok((inner, instance))
    }

    fn estimate_rows(block_traces: &[BlockTrace]) -> usize {
        let witness_block = block_traces_to_witness_block(block_traces).unwrap();
        1 + witness_block
            .rws
            .0
            .iter()
            .fold(0usize, |total, (_, v)| v.len() + total)
    }
    fn get_active_rows(block_traces: &[BlockTrace]) -> (Vec<usize>, Vec<usize>) {
        let witness_block = block_traces_to_witness_block(block_traces).unwrap();
        let rows = Self::estimate_rows(block_traces);
        let active_rows: Vec<_> = (if witness_block.circuits_params.max_rws == 0 {
            0..rows
        } else {
            (witness_block.circuits_params.max_rws - rows)..witness_block.circuits_params.max_rws
        })
        .into_iter()
        .collect();
        (active_rows.clone(), active_rows)
    }
}

fn mpt_rows() -> usize {
    ((1 << *DEGREE) - 10) / <Fr as Hashable>::hash_block_size()
}

fn trie_data_from_blocks<'d>(
    block_traces: impl IntoIterator<Item = &'d BlockTrace>,
) -> EthTrie<Fr> {
    let mut trie_data: EthTrie<Fr> = Default::default();
    for block_trace in block_traces.into_iter() {
        let storage_ops: Vec<AccountOp<_>> = block_trace
            .mpt_witness
            .iter()
            .map(|tr| tr.try_into().unwrap())
            .collect();
        trie_data.add_ops(storage_ops);
    }

    trie_data
}

pub struct ZktrieCircuit {}

impl TargetCircuit for ZktrieCircuit {
    type Inner = EthTrieCircuit<Fr>;

    fn name() -> String {
        "zktrie".to_string()
    }
    fn empty() -> Self::Inner {
        let dummy_trie: EthTrie<Fr> = Default::default();
        let (circuit, _) = dummy_trie.circuits(mpt_rows());
        circuit
    }

    fn from_block_traces(block_traces: &[BlockTrace]) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        let trie_data = trie_data_from_blocks(block_traces);
        //        let (rows, _) = trie_data.use_rows();
        //        log::info!("zktrie use rows {}", rows);
        let (mpt_circuit, _) = trie_data.circuits(mpt_rows());
        let instance = vec![];
        Ok((mpt_circuit, instance))
    }

    fn from_block_trace(block_trace: &BlockTrace) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        let (mpt_circuit, _) = trie_data_from_blocks(Some(block_trace)).circuits(mpt_rows());
        let instance = vec![];
        Ok((mpt_circuit, instance))
    }

    fn estimate_rows(block_traces: &[BlockTrace]) -> usize {
        let (mpt_rows, _) = trie_data_from_blocks(block_traces).use_rows();
        mpt_rows
    }

    fn get_active_rows(block_traces: &[BlockTrace]) -> (Vec<usize>, Vec<usize>) {
        // we have compare and pick the maxium for lookup and gate rows, here we
        // just make sure it not less than 64 (so it has contained all constant rows)
        let ret = Self::estimate_rows(block_traces);
        ((0..ret.max(64)).collect(), (0..ret.max(64)).collect())
    }
}

pub struct PoseidonCircuit {}

impl TargetCircuit for PoseidonCircuit {
    type Inner = HashCircuit<Fr>;

    fn name() -> String {
        "poseidon".to_string()
    }
    fn empty() -> Self::Inner {
        let dummy_trie: EthTrie<Fr> = Default::default();
        let (_, circuit) = dummy_trie.circuits(mpt_rows());
        circuit
    }

    fn from_block_traces(block_traces: &[BlockTrace]) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        let trie_data = trie_data_from_blocks(block_traces);
        //        let (_, rows) = trie_data.use_rows();
        //        log::info!("poseidon use rows {}", rows);
        let (_, circuit) = trie_data.circuits(mpt_rows());
        let instance = vec![];
        Ok((circuit, instance))
    }

    fn from_block_trace(block_trace: &BlockTrace) -> anyhow::Result<(Self::Inner, Vec<Vec<Fr>>)>
    where
        Self: Sized,
    {
        let (_, circuit) = trie_data_from_blocks(Some(block_trace)).circuits(mpt_rows());
        let instance = vec![];
        Ok((circuit, instance))
    }

    fn estimate_rows(block_traces: &[BlockTrace]) -> usize {
        let (_, rows) = trie_data_from_blocks(block_traces).use_rows();
        rows
    }
}
