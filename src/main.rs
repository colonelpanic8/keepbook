use anyhow::Result;

fn main() -> Result<()> {
    println!("Keepbook - Personal Finance Manager");
    println!("====================================\n");
    println!("Usage: keepbook <command>\n");
    println!("Commands:");
    println!("  (coming soon)\n");
    println!("Proof-of-concept synchronizers are available as examples:");
    println!("  cargo run --example coinbase");
    println!("  cargo run --example plaid -- [setup|sync]");
    println!("  cargo run --example schwab -- [setup|sync]");

    Ok(())
}
