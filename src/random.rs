//! Random ids, seeds and nonces, the shapes Excalidraw uses.
//!
//! Not cryptographic: SplitMix64 seeded once per thread from std's
//! per-process `RandomState` keys, which is enough for element ids.
use std::cell::Cell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// nanoid's default URL-safe alphabet.
const ID_ALPHABET: &[u8; 64] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";

/// nanoid's default length.
const ID_LENGTH: usize = 21;

thread_local! {
    static STATE: Cell<u64> = Cell::new(RandomState::new().build_hasher().finish());
}

/// Returns a random id like Excalidraw's `randomId()` (nanoid).
pub(crate) fn id() -> String {
    (0..ID_LENGTH)
        .map(|_| char::from(ID_ALPHABET[(next() >> 58) as usize]))
        .collect()
}

/// Returns a random integer in `0..2^31`, like Excalidraw's `randomInteger()`.
pub(crate) fn integer() -> i64 {
    (next() >> 33) as i64
}

/// SplitMix64.
fn next() -> u64 {
    STATE.with(|state| {
        let s = state.get().wrapping_add(0x9e37_79b9_7f4a_7c15);
        state.set(s);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    #[test]
    fn ids_and_integers_have_excalidraw_shapes() {
        let ids: HashSet<String> = (0..1000).map(|_| super::id()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids.iter().all(|id| {
            id.len() == 21
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }));
        assert!(
            (0..1000)
                .map(|_| super::integer())
                .all(|n| (0..1 << 31).contains(&n))
        );
    }
}
