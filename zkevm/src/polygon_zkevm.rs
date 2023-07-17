// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::EvmTransaction;
use ethers::prelude::*;
use std::borrow::Borrow;

/// Encode transactions as expected by Polygon zkEVM.
///
/// Polygon zkEVM uses a non-standard EVM transaction encoding which mixes the legacy (for the base
/// transaction) and EIP-1559 (for the signature) encodings. This implementation is a direct port
/// from Go of the `state.helper.EncodeTransactions` function in the Polygon zkEVM node.
pub fn encode_transactions<T: Borrow<EvmTransaction>>(txs: impl IntoIterator<Item = T>) -> Bytes {
    txs.into_iter()
        .flat_map(|tx| {
            let tx = tx.borrow();

            let Signature { v, r, s } = tx.signature();
            let parity = if v <= 1 {
                // Ethers.rs uses a different signature normalization scheme than Polygon zkEVM. If `v` is
                // in [0, 1], it is already normalized to represent the y-parity of the signature,
                // but Polygon zkEVM encodes 0 as 27 and 1 as 28.
                v as u8
            } else {
                // If v > 1, it is not yet normalized, so we compute the parity, which we will then
                // map to 27 or 28.
                (1 - (v & 1)) as u8
            };
            let v_norm = 27 + parity;

            let tx_coded_rlp = tx.rlp_base();

            // The Polygon zkEVM Go implementation does some format-to-hex-with-padding and then
            // parsing hex in order to get the byte representation of `r`, `s`, and `v_norm` padded
            // out to 32, 32, and 1 bytes, respectively. We can use Rust's strong typing to avoid
            // this step, since all three parts of the signature are already stored in types with
            // the appropriate lengths: `v` and `r` are `U256`, which is 32 bytes, and `v_norm` is a
            // `u8`, which is 1 byte. Therefore we can simply append the byte representation of
            // these integers directly.
            let mut sig_bytes = [0; 65];
            r.to_big_endian(&mut sig_bytes[0..32]);
            s.to_big_endian(&mut sig_bytes[32..64]);
            sig_bytes[64] = v_norm;

            tx_coded_rlp.into_iter().chain(sig_bytes)
        })
        .collect::<Vec<u8>>()
        .into()
}
