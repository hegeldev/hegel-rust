use crate::stats::Rng;

#[derive(Clone, Copy)]
pub enum Landscape {
    Lin,
    Flat,
    Heavy,
    Disc,
    Gap,
}

pub const ALL: [Landscape; 5] = [
    Landscape::Lin,
    Landscape::Flat,
    Landscape::Heavy,
    Landscape::Disc,
    Landscape::Gap,
];

impl Landscape {
    pub fn name(self) -> &'static str {
        match self {
            Landscape::Lin => "L-lin",
            Landscape::Flat => "L-flat",
            Landscape::Heavy => "L-heavy",
            Landscape::Disc => "L-disc",
            Landscape::Gap => "L-gap",
        }
    }

    pub fn run(self, x: i64, rng: &mut Rng, miss: f64) -> Option<f64> {
        if miss > 0.0 && rng.f64() < miss {
            return None;
        }
        Some(self.sample(x, rng))
    }

    fn sample(self, x: i64, rng: &mut Rng) -> f64 {
        let xf = x as f64;
        match self {
            Landscape::Lin => xf + 5.0 * normal(rng),
            Landscape::Flat => 5.0 * normal(rng),
            Landscape::Heavy => xf + 5.0 * t_df2(rng),
            Landscape::Disc => poisson(5.0 + xf / 10.0, rng) as f64,
            Landscape::Gap => {
                let shift = if x >= 50 { 30.0 } else { 0.0 };
                xf + shift + 5.0 * normal(rng)
            }
        }
    }

    pub fn true_mean(self, x: i64) -> f64 {
        let xf = x as f64;
        match self {
            Landscape::Lin | Landscape::Heavy => xf,
            Landscape::Flat => 0.0,
            Landscape::Disc => 5.0 + xf / 10.0,
            Landscape::Gap => {
                if x >= 50 {
                    xf + 30.0
                } else {
                    xf
                }
            }
        }
    }

    pub fn true_median_x0(self) -> f64 {
        match self {
            Landscape::Disc => 5.0,
            _ => 0.0,
        }
    }

    pub fn has_gradient_above(self, x: i64) -> bool {
        !matches!(self, Landscape::Flat) && x < 100
    }
}

fn normal(rng: &mut Rng) -> f64 {
    let u1 = loop {
        let v = rng.f64();
        if v > 0.0 {
            break v;
        }
    };
    let u2 = rng.f64();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

fn t_df2(rng: &mut Rng) -> f64 {
    let u = loop {
        let v = rng.f64();
        if v > 0.0 && v < 1.0 {
            break v;
        }
    };
    (2.0 * u - 1.0) / (2.0 * u * (1.0 - u)).sqrt()
}

fn poisson(lambda: f64, rng: &mut Rng) -> u64 {
    let limit = (-lambda).exp();
    let mut k = 0u64;
    let mut p = 1.0;
    loop {
        p *= rng.f64();
        if p <= limit {
            return k;
        }
        k += 1;
    }
}
