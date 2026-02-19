/// HistoryList -- MRU cache for AZO compression.
use crate::model::{BoolState, EntropyBitProb};
use crate::range::RangeDecoder;

pub struct HistoryList<T: Copy> {
    pub(crate) rep: Vec<T>,
}

impl<T: Copy> HistoryList<T> {
    pub fn new(items: Vec<T>) -> Self {
        HistoryList { rep: items }
    }

    /// Shift all entries right by 1, insert value at index 0.
    pub fn add(&mut self, value: T) {
        let last = self.rep.len() - 1;
        for i in (1..=last).rev() {
            self.rep[i] = self.rep[i - 1];
        }
        self.rep[0] = value;
    }

    /// Shift entries [0..del_idx-1] right to [1..del_idx], insert at [0].
    pub fn add_at(&mut self, value: T, del_idx: usize) {
        for i in (1..=del_idx).rev() {
            self.rep[i] = self.rep[i - 1];
        }
        self.rep[0] = value;
    }
}

/// SymbolCode: HistoryList + BoolState + EntropyBitProb for decoding indices.
pub struct SymbolCode {
    history: HistoryList<u32>,
    bool_state: BoolState,
    index_prob: EntropyBitProb,
}

impl SymbolCode {
    pub fn new(history_size: usize, alphabet_size: usize, init_val: u32) -> Self {
        let items: Vec<u32> = (0..history_size).map(|i| init_val + i as u32).collect();
        SymbolCode {
            history: HistoryList::new(items),
            bool_state: BoolState::new(8),
            index_prob: EntropyBitProb::new(alphabet_size),
        }
    }

    /// Decode: try history first, then decode new value.
    /// Returns (is_hit, value).
    pub fn decode(&mut self, entropy: &mut RangeDecoder) -> (bool, u32) {
        if self.bool_state.decode(entropy) != 0 {
            // History hit
            let idx = self.index_prob.decode(entropy) as usize;
            if idx < self.history.rep.len() {
                let value = self.history.rep[idx];
                self.history.add_at(value, idx);
                (true, value)
            } else {
                (false, 0)
            }
        } else {
            (false, 0)
        }
    }
}
