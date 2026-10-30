//! The game's float32 1v1 Glicko-2 update and threshold estimates.

/// Volatility and rating origin come from settings; K is ruleset-specific.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub r0: f32,
    pub sigma: f32,
    pub k: f32,
}

/// Maximum match count returned by a reach or drop estimate.
pub const LIMIT: u32 = 999;

const SCALE: f32 = 173.717_8_f32;
const PI2: f32 = 9.869_605_f32;
const TAU: f32 = 0.5_f32;
const EPS: f32 = 1.0e-5_f32;
const MAX_ITERATIONS: usize = 100;

fn g(phi: f32) -> f32 {
    1.0_f32 / ((((phi * 3.0_f32) * phi) / PI2) + 1.0_f32).sqrt()
}

fn volatility_fx(x: f32, d2: f32, p2: f32, v: f32, a: f32) -> f32 {
    let ex = x.exp();
    let num = ex * (((d2 - p2) - v) - ex);
    let base = (p2 + v) + ex;
    let den = base * base;
    (num / (den + den)) - ((x - a) / (TAU * TAU))
}

fn glicko2(
    r: f32,
    rd: f32,
    sigma: f32,
    r0: f32,
    opp_r: f32,
    opp_rd: f32,
    score: f32,
) -> (f32, f32) {
    let mu = (r - r0) / SCALE;
    let phi = rd / SCALE;
    let mu_j = (opp_r - r0) / SCALE;
    let phi_j = opp_rd / SCALE;
    let g_j = g(phi_j);
    let e_j = 1.0_f32 / ((-((mu - mu_j) * g_j)).exp() + 1.0_f32);
    let v = 1.0_f32 / ((g_j * g_j) * e_j * (1.0_f32 - e_j));
    let gsum = g_j * (score - e_j);
    let delta = gsum * v;

    let d2 = delta * delta;
    let p2 = phi * phi;
    let a = (sigma * sigma).ln();

    let mut lower = a;
    let mut upper;
    if d2 > p2 + v {
        upper = ((d2 - p2) - v).ln();
    } else {
        upper = a - TAU;
        // Bound the source algorithm's bracket search for pathological inputs.
        for _ in 0..MAX_ITERATIONS {
            if volatility_fx(upper, d2, p2, v, a).partial_cmp(&0.0_f32)
                != Some(core::cmp::Ordering::Less)
            {
                break;
            }
            upper -= TAU;
        }
    }

    let mut f_lower = volatility_fx(lower, d2, p2, v, a);
    let mut f_upper = volatility_fx(upper, d2, p2, v, a);
    // Bound Illinois iteration as well; ordinary game settings converge first.
    for _ in 0..MAX_ITERATIONS {
        if (f_lower - f_upper).abs() <= EPS {
            break;
        }
        let c = lower + (((lower - upper) * f_lower) / (f_upper - f_lower));
        let f_c = volatility_fx(c, d2, p2, v, a);
        if f_c == 0.0_f32 {
            lower = c;
            break;
        }
        if f_upper == 0.0_f32 {
            lower = upper;
            break;
        }
        if f_upper * f_c < 0.0_f32 {
            lower = upper;
            f_lower = f_upper;
        } else {
            f_lower *= 0.5_f32;
        }
        upper = c;
        f_upper = f_c;
    }

    let sigma_new = (lower * 0.5_f32).exp();
    let phi_star = ((sigma_new * sigma_new) + p2).sqrt();
    let phi_new = 1.0_f32 / ((1.0_f32 / (phi_star * phi_star) + 1.0_f32 / v).sqrt());
    let mu_new = mu + ((phi_new * phi_new) * gsum);
    (r0 + (mu_new * SCALE), phi_new * SCALE)
}

/// Apply one 1v1 game. The returned second value is the new deviation; C is
/// represented by the first value and therefore equals the new A.
pub fn update(a: i32, b: i32, opp_a: i32, opp_b: i32, win: bool, p: &Params) -> (i32, i32) {
    let score = if win { 1.0_f32 } else { 0.0_f32 };
    let (r_new, rd_new) = glicko2(
        a as f32,
        b as f32,
        p.sigma,
        p.r0,
        opp_a as f32,
        opp_b as f32,
        score,
    );
    let change = ((r_new - a as f32) * p.k) * 1.0_f32;
    ((a + change as i32).max(1), rd_new as i32)
}

/// Return the smallest deviation after 64 alternating games against a mirror.
pub fn settled_deviation(p: &Params, b0: i32) -> i32 {
    let mut a = p.r0 as i32;
    let mut b = b0;
    let mut smallest = b;
    for i in 0..64 {
        (a, b) = update(a, b, a, b, i % 2 == 0, p);
        smallest = smallest.min(b);
    }
    smallest
}

/// Count consecutive wins against an opponent at the current rating.
pub fn wins_to_reach(a: i32, b: i32, threshold: i32, opp_b: i32, p: &Params) -> u32 {
    let (mut rating, mut deviation) = (a, b);
    for count in 1..=LIMIT {
        (rating, deviation) = update(rating, deviation, rating, opp_b, true, p);
        if rating >= threshold {
            return count;
        }
    }
    LIMIT
}

/// Count consecutive losses against an opponent at the current rating.
pub fn losses_to_drop(a: i32, b: i32, threshold: i32, opp_b: i32, p: &Params) -> u32 {
    let (mut rating, mut deviation) = (a, b);
    for count in 1..=LIMIT {
        (rating, deviation) = update(rating, deviation, rating, opp_b, false, p);
        if rating < threshold {
            return count;
        }
    }
    LIMIT
}

/// Estimate matches to reach Elite or losses before falling below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Estimate {
    Reach(u32),
    Keep(u32),
}

pub fn estimate(a: i32, b: i32, c: i32, threshold: i32, opp_b: i32, p: &Params) -> Estimate {
    if c >= threshold {
        Estimate::Keep(losses_to_drop(a, b, threshold, opp_b, p))
    } else {
        Estimate::Reach(wins_to_reach(a, b, threshold, opp_b, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P09: Params = Params {
        r0: 1500.0,
        sigma: 0.06,
        k: 0.9,
    };
    const P10: Params = Params { k: 1.0, ..P09 };

    #[test]
    fn known_updates_match_the_game_model() {
        assert_eq!(update(1500, 130, 1500, 130, true, &P09), (1536, 123));
        assert_eq!(update(1500, 130, 1500, 130, false, &P09), (1464, 123));
        assert_eq!(update(1464, 123, 1518, 130, false, &P09), (1436, 117));
        assert_eq!(update(1464, 123, 1518, 130, true, &P09), (1501, 117));
        assert_eq!(update(1464, 123, 1500, 130, false, &P09).0, 1435);
        assert_eq!(update(1464, 123, 1500, 130, true, &P09).0, 1500);
    }

    #[test]
    fn settled_deviation_matches_the_mirror_series() {
        assert_eq!(settled_deviation(&P09, 130), 60);
    }

    #[test]
    fn win_estimates_match_the_expected_sequences() {
        for (a, b, expected) in [
            (1500, 130, 5),
            (1518, 130, 4),
            (1500, 60, 16),
            (1560, 100, 4),
            (1600, 60, 5),
            (1620, 60, 3),
            (1635, 60, 1),
        ] {
            assert_eq!(wins_to_reach(a, b, 1642, 60, &P09), expected);
        }
        assert_eq!(wins_to_reach(1500, 130, 1642, 60, &P10), 4);
        assert_eq!(wins_to_reach(1500, 60, 1642, 60, &P10), 15);
        assert_eq!(wins_to_reach(1600, 60, 1642, 60, &P10), 5);
    }

    #[test]
    fn loss_estimates_match_the_expected_sequences() {
        for (a, b, opp_b, expected) in [
            (1642, 60, 60, 1),
            (1650, 60, 60, 1),
            (1700, 60, 60, 7),
            (1700, 100, 60, 3),
            (1800, 60, 60, 18),
            (1900, 60, 60, 29),
            (2000, 60, 60, 40),
        ] {
            assert_eq!(losses_to_drop(a, b, 1642, opp_b, &P09), expected);
        }
        assert_eq!(losses_to_drop(1700, 60, 1642, 60, &P10), 6);
        assert_eq!(losses_to_drop(1800, 60, 1642, 60, &P10), 16);
        assert_eq!(losses_to_drop(2000, 60, 1642, 60, &P10), 36);
    }

    #[test]
    fn estimate_uses_displayed_c_to_choose_the_direction() {
        assert_eq!(estimate(1500, 130, 1642, 1642, 60, &P09), Estimate::Keep(1));
        assert_eq!(
            estimate(1500, 130, 1641, 1642, 60, &P09),
            Estimate::Reach(5)
        );
    }

    #[test]
    fn generated_python_vectors_match_exactly() {
        let mut count = 0;
        for line in include_str!("glicko_vectors.txt").lines() {
            let mut fields = line.split_whitespace();
            let a: i32 = fields.next().unwrap().parse().unwrap();
            let b: i32 = fields.next().unwrap().parse().unwrap();
            let opp_a: i32 = fields.next().unwrap().parse().unwrap();
            let opp_b: i32 = fields.next().unwrap().parse().unwrap();
            let win = fields.next().unwrap() == "W";
            let k: f32 = fields.next().unwrap().parse().unwrap();
            let expected_a: i32 = fields.next().unwrap().parse().unwrap();
            let expected_b: i32 = fields.next().unwrap().parse().unwrap();
            assert!(fields.next().is_none());
            let params = Params { k, ..P09 };
            let actual = update(a, b, opp_a, opp_b, win, &params);
            assert_eq!(
                actual,
                (expected_a, expected_b),
                "vector inputs: a={a}, b={b}, opp_a={opp_a}, opp_b={opp_b}, win={win}, k={k}; Python output=({expected_a}, {expected_b}), Rust output=({}, {})",
                actual.0,
                actual.1
            );
            count += 1;
        }
        assert_eq!(count, 304);
    }
}
