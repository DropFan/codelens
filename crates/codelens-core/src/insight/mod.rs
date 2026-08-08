//! Code insight analysis — health scoring, hotspot detection, trend tracking.

pub mod coupling;
pub mod diff;
pub mod estimation;
pub mod health;
pub mod hotspot;
pub mod scoring;
pub mod trend;

use serde::Serialize;
use std::fmt;

/// Health grade: A through F.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Grade {
    A,
    B,
    C,
    D,
    F,
}

impl fmt::Display for Grade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Grade::A => write!(f, "A"),
            Grade::B => write!(f, "B"),
            Grade::C => write!(f, "C"),
            Grade::D => write!(f, "D"),
            Grade::F => write!(f, "F"),
        }
    }
}

/// Generic delta between two values.
#[derive(Debug, Clone, Serialize)]
pub struct DeltaValue<T: Serialize> {
    pub from: T,
    pub to: T,
    pub delta: T,
    pub percent: f64,
}

impl DeltaValue<usize> {
    pub fn new(from: usize, to: usize) -> Self {
        let delta = to.saturating_sub(from);
        let percent = if from > 0 {
            (to as f64 - from as f64) / from as f64 * 100.0
        } else if to > 0 {
            100.0
        } else {
            0.0
        };
        Self {
            from,
            to,
            delta,
            percent,
        }
    }

    pub fn signed_delta(&self) -> i64 {
        self.to as i64 - self.from as i64
    }
}

impl DeltaValue<f64> {
    pub fn new_f64(from: f64, to: f64) -> Self {
        let delta = to - from;
        let percent = if from.abs() > f64::EPSILON {
            delta / from * 100.0
        } else if to.abs() > f64::EPSILON {
            100.0
        } else {
            0.0
        };
        Self {
            from,
            to,
            delta,
            percent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grade_display() {
        assert_eq!(Grade::A.to_string(), "A");
        assert_eq!(Grade::F.to_string(), "F");
    }

    #[test]
    fn test_grade_ordering() {
        assert!(Grade::A < Grade::B);
        assert!(Grade::B < Grade::F);
    }

    #[test]
    fn test_delta_value_increase() {
        let d = DeltaValue::new(100, 120);
        assert_eq!(d.delta, 20);
        assert!((d.percent - 20.0).abs() < 0.01);
        assert_eq!(d.signed_delta(), 20);
    }

    #[test]
    fn test_delta_value_decrease() {
        let d = DeltaValue::new(100, 80);
        assert_eq!(d.delta, 0);
        assert_eq!(d.signed_delta(), -20);
        assert!((d.percent - (-20.0)).abs() < 0.01);
    }

    #[test]
    fn test_delta_value_from_zero() {
        let d = DeltaValue::new(0, 50);
        assert!((d.percent - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_delta_value_f64() {
        let d = DeltaValue::new_f64(82.0, 78.0);
        assert!((d.delta - (-4.0)).abs() < 0.01);
    }
}
