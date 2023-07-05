use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction, utils::rlp::Rlp};
use jf_primitives::merkle_tree::namespaced_merkle_tree::NamespaceProof;
use sequencer::{Block, Vm, VmId, VmTransaction};

pub mod polygon_zkevm;

#[derive(Clone, Debug)]
pub struct EvmTransaction {
    tx: TypedTransaction,
    sig: Signature,
}

impl VmTransaction for EvmTransaction {
    fn encode(&self) -> Vec<u8> {
        self.rlp_signed().to_vec()
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        let (tx, sig) = TypedTransaction::decode_signed(&Rlp::new(bytes)).ok()?;
        Some(Self { tx, sig })
    }
}

impl EvmTransaction {
    pub fn new(tx: TypedTransaction, sig: Signature) -> Self {
        Self { tx, sig }
    }

    pub fn signature(&self) -> Signature {
        self.sig
    }

    pub fn rlp_base(&self) -> Bytes {
        self.tx.rlp()
    }

    pub fn rlp_signed(&self) -> Bytes {
        self.tx.rlp_signed(&self.sig)
    }

    pub fn hash(&self) -> H256 {
        self.tx.hash(&self.sig)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ZkEvm {
    pub chain_id: u64,
}

impl Vm for ZkEvm {
    type Transaction = EvmTransaction;

    fn id(&self) -> VmId {
        self.chain_id.into()
    }
}

impl ZkEvm {
    /// Extract the VM transactions from a block.
    pub fn vm_transactions(&self, block: &Block) -> Vec<<Self as Vm>::Transaction> {
        let proof = block.get_namespace_proof(self.id());
        let transactions = proof.get_namespace_leaves();
        // Note: this discards transactions that cannot be decoded.
        transactions
            .iter()
            .flat_map(|txn| txn.as_vm(self))
            .collect()
    }
}
