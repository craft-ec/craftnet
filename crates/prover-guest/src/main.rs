// Receipt prover guest removed — receipt ZK proving is no longer used.
// Only the distribution guest remains for on-chain Groth16 verification.

#![no_main]
sp1_zkvm::entrypoint!(main);

pub fn main() {
    panic!("receipt prover guest is deprecated — use distribution-guest instead");
}
