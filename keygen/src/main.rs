use clap::Parser;
use std::path::PathBuf;

use ethers::signers::{Signer, Wallet};

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
        env = "ESPRESSO_ZKEVM_KEYSTORE_PASSWORD",
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
    let (wallet, _) = Wallet::new_keystore(dir.clone(), &mut rng, password, Some(&name)).unwrap();
    let address = wallet.address();

    let mut buf: PathBuf = dir;
    buf.push(name);
    println!(
        "New Keystore with address {:?} created at {}",
        address,
        buf.display()
    )
}
