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

pub fn wilson_lcb(successes: u64, runs: u64, z: f64) -> f64 {
    if runs == 0 {
        return 0.0;
    }
    let n = runs as f64;
    let p = successes as f64 / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = p + z2 / (2.0 * n);
    let margin = z * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt();
    ((center - margin) / denom).clamp(0.0, 1.0)
}

pub fn median_upper(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

pub fn min_beats(k: u64, z: f64) -> u64 {
    (0..=k).find(|&m| wilson_lcb(m, k, z) > 0.5).unwrap_or(k + 1)
}

pub fn binom_tail_ge(k: u64, q: f64, m: u64) -> f64 {
    let mut pmf = (1.0 - q).powi(k as i32);
    let ratio = q / (1.0 - q);
    let mut acc = if m == 0 { pmf } else { 0.0 };
    for i in 0..k {
        pmf *= (k - i) as f64 / (i + 1) as f64 * ratio;
        if i + 1 >= m {
            acc += pmf;
        }
    }
    acc.min(1.0)
}
