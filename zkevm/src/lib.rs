// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction, utils::rlp::Rlp};
use hotshot_query_service::availability::{BlockQueryData, VidCommonQueryData};
use hotshot_types::vid::{vid_scheme, VidSchemeType};
use jf_primitives::vid::VidScheme;
use sequencer::{SeqTypes, Transaction};

pub mod polygon_zkevm;

#[derive(Clone, Debug)]
pub struct EvmTransaction {
    tx: TypedTransaction,
    sig: Signature,
}

impl EvmTransaction {
    pub fn new(tx: TypedTransaction, sig: Signature) -> Self {
        Self { tx, sig }
    }

    pub fn encode(&self) -> Vec<u8> {
        self.rlp_signed().to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (tx, sig) = TypedTransaction::decode_signed(&Rlp::new(bytes)).ok()?;
        Some(Self { tx, sig })
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

impl ZkEvm {
    pub fn wrap(&self, tx: &EvmTransaction) -> Transaction {
        Transaction::new(self.chain_id.into(), tx.encode())
    }
}

impl ZkEvm {
    /// Extract the VM transactions from a block payload.
    pub fn vm_transactions(
        &self,
        block: &BlockQueryData<SeqTypes>,
        common: &VidCommonQueryData<SeqTypes>,
    ) -> Vec<EvmTransaction> {
        let common = common.common();
        let num_storage_nodes = VidSchemeType::get_num_storage_nodes(common);
        let vid = vid_scheme(num_storage_nodes);

        let proof = block
            .payload()
            .namespace_with_proof(block.metadata(), self.chain_id.into(), common.clone())
            .unwrap();
        let transactions = proof
            .verify(&vid, &block.payload_hash(), block.metadata())
            .unwrap()
            .0;
        // Note: this discards transactions that cannot be decoded.
        transactions
            .iter()
            .filter_map(|txn| {
                if txn.namespace() == self.chain_id.into() {
                    EvmTransaction::decode(txn.payload())
                } else {
                    None
                }
            })
            .collect()
    }
}
