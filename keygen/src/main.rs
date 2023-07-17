use clap::Parser;
use std::path::PathBuf;

use ethers::{prelude::k256::ecdsa::SigningKey, utils::secret_key_to_address};

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
        default_value = "aggregator.keystore"
    )]
    filename: String,
}

fn main() {
    let opt = Options::parse();
    let dir = opt.keystore_dir;
    let name = opt.filename;
    let password = opt.password;

    let mut rng = rand::thread_rng();
    let (secret, _) = eth_keystore::new(dir.clone(), &mut rng, password, Some(&name)).unwrap();
    let signer = SigningKey::from_bytes(secret.as_slice().into()).unwrap();
    let address = secret_key_to_address(&signer);

    let mut full_path: PathBuf = dir;
    full_path.push(name);

    println!(
        "New Keystore with address {:?} created at {}",
        address,
        full_path.display()
    )
}
