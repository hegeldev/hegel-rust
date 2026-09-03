pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

#[derive(Default, Clone, Copy)]
pub struct Evidence {
    pub runs: u64,
    pub fails: u64,
}

fn wilson(fails: u64, runs: u64, z: f64, upper: bool) -> f64 {
    if runs == 0 {
        return if upper { 1.0 } else { 0.0 };
    }
    let n = runs as f64;
    let p = fails as f64 / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = p + z2 / (2.0 * n);
    let margin = z * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt();
    let bound = if upper {
        (center + margin) / denom
    } else {
        (center - margin) / denom
    };
    bound.clamp(0.0, 1.0)
}

pub fn wilson_lcb(fails: u64, runs: u64, z: f64) -> f64 {
    wilson(fails, runs, z, false)
}

pub fn wilson_weighted(fails: f64, total: f64, z: f64, upper: bool) -> f64 {
    if total <= 0.0 {
        return if upper { 1.0 } else { 0.0 };
    }
    let p = fails / total;
    let z2 = z * z;
    let denom = 1.0 + z2 / total;
    let center = p + z2 / (2.0 * total);
    let margin = z * ((p * (1.0 - p) + z2 / (4.0 * total)) / total).sqrt();
    let bound = if upper {
        (center + margin) / denom
    } else {
        (center - margin) / denom
    };
    bound.clamp(0.0, 1.0)
}

pub fn wilson_ucb(fails: u64, runs: u64, z: f64) -> f64 {
    wilson(fails, runs, z, true)
}

pub fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
