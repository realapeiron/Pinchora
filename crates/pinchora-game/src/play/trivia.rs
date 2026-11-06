//! Trivia rounds: showing a question's options in a random order.

use super::Question;

/// A shuffle of the four options from `seed`: slot `i` shows option `order[i]`.
pub fn permutation(seed: u64) -> [usize; 4] {
    // splitmix64 steps drive a Fisher-Yates shuffle, so one number fixes the whole order.
    let mut state = seed;
    let mut next = move || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let mut order = [0, 1, 2, 3];
    for i in (1..4).rev() {
        order.swap(i, (next() % (i as u64 + 1)) as usize);
    }
    order
}

pub fn encode_order(order: [usize; 4]) -> String {
    order.iter().map(|i| char::from(b'0' + *i as u8)).collect()
}

/// Reads an order written by [`encode_order`]; it must use each of 0 to 3 once.
pub fn decode_order(text: &str) -> Option<[usize; 4]> {
    let digits: Vec<usize> = text
        .chars()
        .map(|c| c.to_digit(4).map(|d| d as usize))
        .collect::<Option<_>>()?;
    let order: [usize; 4] = digits.try_into().ok()?;
    let mut seen = [false; 4];
    for i in order {
        seen[i] = true;
    }
    seen.iter().all(|s| *s).then_some(order)
}

impl Question {
    /// The options in the order of `order`.
    pub fn shown(&self, order: [usize; 4]) -> [&str; 4] {
        order.map(|i| self.options.get(i).map_or("", String::as_str))
    }

    /// Which slot of `order` holds the right answer.
    pub fn correct_slot(&self, order: [usize; 4]) -> Option<usize> {
        self.shown(order).iter().position(|o| *o == self.answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::{Difficulty, TriviaCategory};

    fn question() -> Question {
        Question {
            id: "ocean_001".into(),
            category: TriviaCategory::Ocean,
            difficulty: Difficulty::Easy,
            question: "Which is the biggest fish in the sea?".into(),
            options: ["Great white", "Manta ray", "Swordfish", "Whale shark"]
                .map(String::from)
                .to_vec(),
            answer: "Whale shark".into(),
            lang: Default::default(),
        }
    }

    #[test]
    fn every_seed_gives_a_real_permutation() {
        let mut distinct = std::collections::HashSet::new();
        for seed in 0..500 {
            let order = permutation(seed);
            let mut sorted = order;
            sorted.sort_unstable();
            assert_eq!(sorted, [0, 1, 2, 3], "seed {seed}");
            distinct.insert(order);
        }
        assert!(distinct.len() > 12, "the shuffle reaches most of the 24 orders");
    }

    #[test]
    fn the_right_answer_follows_the_shuffle() {
        let q = question();
        for seed in 0..50 {
            let order = permutation(seed);
            let slot = q.correct_slot(order).expect("answer is shown");
            assert_eq!(q.shown(order)[slot], "Whale shark");
        }
        assert_eq!(q.correct_slot([0, 1, 2, 3]), Some(3));
        assert_eq!(q.correct_slot([3, 2, 1, 0]), Some(0));
    }

    #[test]
    fn orders_survive_encoding_and_bad_ones_are_refused() {
        let order = permutation(7);
        assert_eq!(decode_order(&encode_order(order)), Some(order));
        for bad in ["", "012", "01234", "0122", "0124", "abcd"] {
            assert_eq!(decode_order(bad), None, "{bad:?}");
        }
    }
}
