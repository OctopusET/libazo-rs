/// Match decoding for AZO compression.
/// Handles dictionary, distance history, distance/length coding.
use crate::history::{HistoryList, SymbolCode};
use crate::model::{BoolState, EntropyBitProb, PredictProb};
use crate::range::RangeDecoder;
use crate::table::{self, CODE_SIZE, CodeTable};

const DICTIONARY_SIZE: usize = 128;
const DISTANCE_HISTORY_SIZE: usize = 2;
const MATCH_MIN_DIST: u32 = 1;
const MATCH_MIN_LENGTH: u32 = 2;

#[derive(Clone, Copy)]
struct DictEntry {
    pos: u32,
    len: u32,
}

pub struct MatchCode {
    // Dictionary
    dict: Vec<DictEntry>,
    dict_find_bool: BoolState,
    dict_symbol: SymbolCode,

    // Distance
    dist_history: HistoryList<u32>,
    dist_history_bool: BoolState,
    dist_history_idx: EntropyBitProb,
    dist_code_prob: EntropyBitProb,
    dist_table: CodeTable,

    // Length
    length_predict: PredictProb,
    length_table: CodeTable,
}

impl Default for MatchCode {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchCode {
    pub fn new() -> Self {
        let dict: Vec<DictEntry> = (0..DICTIONARY_SIZE)
            .map(|i| DictEntry {
                pos: 0,
                len: MATCH_MIN_LENGTH + i as u32,
            })
            .collect();

        let dist_hist: Vec<u32> = (0..DISTANCE_HISTORY_SIZE)
            .map(|i| MATCH_MIN_DIST + i as u32)
            .collect();

        MatchCode {
            dict,
            dict_find_bool: BoolState::new(8),
            dict_symbol: SymbolCode::new(2, DICTIONARY_SIZE, 0),

            dist_history: HistoryList::new(dist_hist),
            dist_history_bool: BoolState::new(8),
            dist_history_idx: EntropyBitProb::new(DISTANCE_HISTORY_SIZE),
            dist_code_prob: EntropyBitProb::new(CODE_SIZE),
            dist_table: CodeTable::build_dist_table(),

            length_predict: PredictProb::new(CODE_SIZE, CODE_SIZE, 4),
            length_table: CodeTable::build_length_table(),
        }
    }

    /// Decode a match, returns (distance, length).
    pub fn decode(&mut self, entropy: &mut RangeDecoder, current_pos: u32) -> (u32, u32) {
        // Try dictionary
        if self.dict_find_bool.decode(entropy) != 0 {
            let (hit, idx) = self.dict_symbol.decode(entropy);
            if hit && (idx as usize) < DICTIONARY_SIZE {
                let entry = self.dict[idx as usize];
                let distance = current_pos - entry.pos;
                let length = entry.len;

                // MRU update
                let idx = idx as usize;
                for i in (1..=idx).rev() {
                    self.dict[i] = self.dict[i - 1];
                }
                self.dict[0] = DictEntry {
                    pos: entry.pos,
                    len: entry.len,
                };

                return (distance, length);
            }
        }

        // Decode distance
        let distance = self.decode_distance(entropy);

        // Decode length using distance code as context
        let dist_code = table::get_dist_code(distance) as usize;
        let length = self.decode_length(entropy, dist_code);

        // Add to dictionary
        for i in (1..DICTIONARY_SIZE).rev() {
            self.dict[i] = self.dict[i - 1];
        }
        self.dict[0] = DictEntry {
            pos: current_pos,
            len: length,
        };

        (distance, length)
    }

    fn decode_distance(&mut self, entropy: &mut RangeDecoder) -> u32 {
        // Try distance history
        if self.dist_history_bool.decode(entropy) != 0 {
            let idx = self.dist_history_idx.decode(entropy) as usize;
            if idx < self.dist_history.rep.len() {
                let dist = self.dist_history.rep[idx];
                self.dist_history.add_at(dist, idx);
                return dist;
            }
        }

        // Decode new distance
        let code_idx = self.dist_code_prob.decode(entropy) as usize;
        let mut dist = self.dist_table.base[code_idx];
        if self.dist_table.extra_bits[code_idx] > 0 {
            dist += entropy.decode_uniform(self.dist_table.extra_bits[code_idx]);
        }

        self.dist_history.add(dist);
        dist
    }

    fn decode_length(&mut self, entropy: &mut RangeDecoder, dist_code: usize) -> u32 {
        let code_idx = self.length_predict.decode(entropy, dist_code) as usize;
        let mut length = self.length_table.base[code_idx];
        if self.length_table.extra_bits[code_idx] > 0 {
            length += entropy.decode_uniform(self.length_table.extra_bits[code_idx]);
        }
        length
    }
}
