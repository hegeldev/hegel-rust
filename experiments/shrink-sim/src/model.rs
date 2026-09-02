pub type Candidate = Vec<u64>;

pub const BUG_THRESHOLD: u64 = 50;

pub const MIX_P_HI: f64 = 0.9;
pub const MIX_P_LO: f64 = 0.05;
pub const MIX_W: f64 = 0.5;
pub const REPLAY_FIT: f64 = 0.8;

pub fn has_bug(c: &Candidate) -> bool {
    c.iter().any(|&a| a >= BUG_THRESHOLD)
}

pub fn shortlex_less(a: &Candidate, b: &Candidate) -> bool {
    (a.len(), a.as_slice()) < (b.len(), b.as_slice())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pin {
    Failing,
    Random,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Landscape {
    RisingWithSize,
    DeterministicCore,
    Constant,
    NoiseFloor,
    Mixture { pin: Pin },
}

pub const ALL_LANDSCAPES: [Landscape; 4] = [
    Landscape::RisingWithSize,
    Landscape::DeterministicCore,
    Landscape::Constant,
    Landscape::NoiseFloor,
];

impl Landscape {
    pub fn p(self, c: &Candidate) -> f64 {
        match self {
            Landscape::RisingWithSize => {
                if has_bug(c) {
                    (0.1 + 0.08 * (c.len().saturating_sub(1)) as f64).clamp(0.1, 0.95)
                } else {
                    0.0
                }
            }
            Landscape::DeterministicCore => {
                if c.iter().any(|&a| a == BUG_THRESHOLD) {
                    1.0
                } else if has_bug(c) {
                    0.35
                } else {
                    0.0
                }
            }
            Landscape::Constant => {
                if has_bug(c) {
                    0.5
                } else {
                    0.0
                }
            }
            Landscape::NoiseFloor => {
                if has_bug(c) {
                    0.9
                } else {
                    0.02
                }
            }
            Landscape::Mixture { .. } => {
                if has_bug(c) {
                    MIX_W * MIX_P_HI + (1.0 - MIX_W) * MIX_P_LO
                } else {
                    0.0
                }
            }
        }
    }

    pub fn pin_p(self, c: &Candidate, hi: bool) -> f64 {
        match self {
            Landscape::Mixture { .. } => {
                if !has_bug(c) {
                    0.0
                } else if hi {
                    MIX_P_HI
                } else {
                    MIX_P_LO
                }
            }
            _ => self.p(c),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Landscape::RisingWithSize => "L1 rising-with-size",
            Landscape::DeterministicCore => "L2 deterministic-core",
            Landscape::Constant => "L3 constant p=0.5",
            Landscape::NoiseFloor => "L4 noise-floor",
            Landscape::Mixture { pin: Pin::Failing } => "L5 mixture pin-failing",
            Landscape::Mixture { pin: Pin::Random } => "L5 mixture pin-random",
        }
    }
}
