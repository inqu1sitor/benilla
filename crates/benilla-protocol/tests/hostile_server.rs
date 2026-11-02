//! A hostile server's bytes: every server opcode fed deterministic random bodies, through the
//! parser and the event decoder. None may panic; an error is the right answer to garbage.

use benilla_protocol::messages;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let n = (self.next() % (max as u64 + 1)) as usize;
        // Mostly small numbers, so counts and sizes land in plausible ranges as often as not.
        (0..n)
            .map(|_| {
                let v = self.next();
                if v.is_multiple_of(3) {
                    v as u8
                } else {
                    (v % 4) as u8
                }
            })
            .collect()
    }
}

#[test]
fn every_opcode_survives_random_bodies() {
    let mut r = Rng(std::env::var("FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x5EED_1234_ABCD_0001));
    let rounds: usize = std::env::var("FUZZ_ROUNDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(48);
    for opcode in 0..=0x0500u16 {
        for _ in 0..rounds {
            let body = r.bytes(160);
            if let Ok(packet) = messages::parse_server(opcode, &body) {
                let _ = benilla_protocol::decode(packet);
            }
        }
    }
}
