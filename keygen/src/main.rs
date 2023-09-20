use clap::Parser;
use coins_bip32::{path::DerivationPath, Bip32Error};
use eth_keystore::encrypt_key;
use std::{path::PathBuf, str::FromStr};

use ethers::{
    prelude::k256::ecdsa::SigningKey,
    signers::{
        coins_bip39::{English, Mnemonic},
        WalletError,
    },
    utils::secret_key_to_address,
};

const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

#[derive(Parser, Clone, Debug)]
struct Options {
    #[clap(long, env = "ESPRESSO_ZKEVM_KEYSTORE_DIR", default_value = "./")]
    keystore_dir: PathBuf,

    #[clap(
        long,
        env = "ESPRESSO_ZKEVM_KEYSTORE_PASSWORD",
        default_value = "testonly"
    )]
    password: String,

    #[clap(
        long,
        env = "ESPRESSO_ZKEVM_KEYSTORE_NAME",
        default_value = "sequencer.keystore"
    )]
    filename: String,

    #[clap(
        long,
        env = "ESPRESSO_ZKEVM_KEYSTORE_MNEMONIC",
        default_value = TEST_MNEMONIC,
    )]
    mnemonic: String,

    #[clap(long, env = "ESPRESSO_ZKEVM_KEYSTORE_INDEX", default_value = "0")]
    index: u32,
}

fn mnemonic_to_key(
    mnemonic: &Mnemonic<English>,
    derivation_path: &DerivationPath,
) -> Result<SigningKey, WalletError> {
    let derived_priv_key = mnemonic.derive_key(derivation_path, None /* password */)?;
    let key: &coins_bip32::prelude::SigningKey = derived_priv_key.as_ref();
    Ok(SigningKey::from_bytes(&key.to_bytes())?)
}

fn create_key_store(
    dir: PathBuf,
    name: &str,
    mnemonic: &str,
    index: u32,
    password: &str,
) -> Result<(), Bip32Error> {
    let mnemonic = Mnemonic::<English>::new_from_phrase(mnemonic).unwrap();
    let derivation_path = DerivationPath::from_str(&format!("m/44'/60'/0'/0/{}", index)).unwrap();
    let signer = mnemonic_to_key(&mnemonic, &derivation_path).unwrap();
    let address = secret_key_to_address(&signer);

    let mut rng = rand::thread_rng();
    let sign_key_bytes = signer.to_bytes();
    encrypt_key(dir.clone(), &mut rng, sign_key_bytes, password, Some(name)).unwrap();
    println!(
        "New Keystore with address {:?} created at {:?}",
        address,
        dir.join(name)
    );

    Ok(())
}

fn main() {
    let opt = Options::parse();
    create_key_store(
        opt.keystore_dir,
        &opt.filename,
        &opt.mnemonic,
        opt.index,
        &opt.password,
    )
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use eth_keystore::decrypt_key;
    use ethers::{signers::coins_bip39::English, types::H160, utils::hex};
    use std::str::FromStr;

    #[test]
    fn test_mnemonic_to_key() {
        let expected_address: H160 = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
            .parse()
            .unwrap();
        let expected_private_key =
            hex::decode("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
                .unwrap();

        // Derive known key in memory
        let mnemonic = Mnemonic::<English>::new_from_phrase(TEST_MNEMONIC).unwrap();
        let derivation_path =
            DerivationPath::from_str(&format!("m/44'/60'/0'/0/{}", 0u32)).unwrap();
        let signer = mnemonic_to_key(&mnemonic, &derivation_path).unwrap();
        let signing_key_bytes = signer.to_bytes().to_vec();

        // Check the derived key matches the expected key
        assert_eq!(signing_key_bytes, expected_private_key);

        // Check the address of the key matches the expected address of the first account
        let address = secret_key_to_address(&signer);
        assert_eq!(address, expected_address);

        // Create a keystore and check the decrypted key matches the key derived in memory
        let name = "test.keystore";
        let tmpdir = tempfile::tempdir().unwrap();
        let dir = tmpdir.path().to_path_buf();
        let path = dir.join(name);
        create_key_store(dir, name, TEST_MNEMONIC, 0, "testonly").unwrap();

        let decrypted_key_bytes = decrypt_key(path, "testonly").unwrap();
        assert_eq!(decrypted_key_bytes, expected_private_key);
    }
}
