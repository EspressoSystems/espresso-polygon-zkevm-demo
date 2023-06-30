use std::io;

#[async_std::main]
async fn main() -> io::Result<()> {
    faucet::main()
}
