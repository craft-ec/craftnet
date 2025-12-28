use sha2::{Sha256, Digest};
use std::time::Instant;

fn main() {
    for count in [1_000, 10_000, 100_000, 1_000_000] {
        let start = Instant::now();
        
        let mut secrets: Vec<[u8; 32]> = Vec::with_capacity(count);
        let mut hashes: Vec<[u8; 32]> = Vec::with_capacity(count);
        
        for i in 0u64..count as u64 {
            let mut hasher = Sha256::new();
            hasher.update(i.to_le_bytes());
            hasher.update(b"user_master_secret");
            let secret: [u8; 32] = hasher.finalize().into();
            
            let mut hasher = Sha256::new();
            hasher.update(&secret);
            let hash: [u8; 32] = hasher.finalize().into();
            
            secrets.push(secret);
            hashes.push(hash);
        }
        
        let elapsed = start.elapsed();
        println!("{:>10} credits: {:>8.2?}  ({:>12.0}/sec)  mem: {:>3}MB", 
            count, elapsed, 
            count as f64 / elapsed.as_secs_f64(),
            (count * 64) / 1024 / 1024);
    }
}
