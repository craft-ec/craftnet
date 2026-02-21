//! Print the distribution guest verification key hash.
//!
//! Run with:
//! ```
//! SP1_PROVER=network NETWORK_PRIVATE_KEY=<key> \
//!   cargo run -p craftnet-prover --features sp1 --example vkey_hash
//! ```
//!
//! The output is the `DISTRIBUTION_VKEY_HASH` constant needed in the
//! on-chain settlement program for Groth16 proof verification.

fn main() {
    #[cfg(feature = "sp1")]
    {
        let prover = craftnet_prover::DistributionProver::new();
        let hash = prover.vkey_hash();
        println!("Distribution guest vkey hash: {}", hash);
    }

    #[cfg(not(feature = "sp1"))]
    {
        eprintln!("Error: This example requires the `sp1` feature.");
        eprintln!("Run with: cargo run -p craftnet-prover --features sp1 --example vkey_hash");
        std::process::exit(1);
    }
}
