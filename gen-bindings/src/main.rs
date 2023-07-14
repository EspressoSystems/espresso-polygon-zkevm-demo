// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use ethers::prelude::{Abigen, MultiAbigen};
use std::env;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn find_paths(dir: &str, ext: &str) -> Vec<PathBuf> {
    glob::glob(&format!("{dir}/**/*{ext}"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect()
}

/// Read the contract ABI and generate rust bindings.
fn main() -> Result<(), ()> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir.parent().unwrap();

    // 1. Generate bindings for all zkevm-contracts
    //
    // Hardhat's debug files trip up MultiAbigen otherwise we could use
    // MultiAbigen::from_json_files instead
    let artifacts: Vec<_> = find_paths(
        workspace_dir
            .join("zkevm-contracts/artifacts/contracts")
            .to_str()
            .unwrap(),
        ".json",
    )
    .into_iter()
    .filter(|path| !path.to_str().unwrap().ends_with(".dbg.json"))
    .collect();

    let mut abigens = MultiAbigen::from_abigens(
        artifacts
            .iter()
            .map(|path| Abigen::from_file(path).unwrap()),
    );

    // 2. Generate bindings for other contracts (including Matic)
    //    in gen-bindings/contracts/abi.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for abigen in MultiAbigen::from_json_files(manifest_dir.join("contracts/abi"))
        .expect("Failed to read contracts")
        .iter()
    {
        abigens.push(abigen.clone());
    }

    for abigen in MultiAbigen::from_abigens(
        artifacts
            .iter()
            .map(|path| Abigen::from_file(path).unwrap()),
    )
    .iter()
    {
        abigens.push(abigen.clone());
    }

    // Remove existing bindings
    let bindings_dir = workspace_dir.join("contract-bindings/src/bindings");
    if bindings_dir.exists() {
        std::fs::remove_dir_all(&bindings_dir).unwrap();
    }

    // Generate bindings
    let bindings = abigens.build().unwrap();
    bindings.write_to_module(&bindings_dir, false).unwrap();

    // Unfortunately the bindings are not always correctly formatted.
    Command::new("rustfmt")
        .arg(bindings_dir.join("mod.rs").to_str().unwrap())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    println!("zkevm-contract bindings written to {bindings_dir:?}");

    Ok(())
}
