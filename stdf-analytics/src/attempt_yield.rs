// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

//! Exact all-attempt yield over already-scoped physical PRR records.
//!
//! Each call to [`YieldAccumulator::add`] represents one PRR, including attempts
//! with no measurements or no resolved device identity. The caller owns source
//! verification, scope selection and explicit attempt exclusions. This reducer
//! performs no identity deduplication, latest-attempt selection or measurement
//! filtering, and does not infer a PRR verdict from bins or test results.

use std::io;

/// The recorded PRR outcome, independently of device identity and exclusions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Unknown,
}

impl Verdict {
    /// Decodes PART_FLG using the same outcome policy as the attempt reader.
    ///
    /// Bits 2 (abnormal end) and 4 (pass/fail invalid) take precedence over bit 3
    /// (failed). Other bits do not change the recorded outcome.
    pub fn from_prr_flags(flags: u8) -> Self {
        if flags & 0x14 != 0 {
            Self::Unknown
        } else if flags & 8 != 0 {
            Self::Fail
        } else {
            Self::Pass
        }
    }
}

/// Caller-provided disposition of one physical PRR inside the requested scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttemptDisposition {
    pub verdict: Verdict,
    /// False retains the attempt and counts it as unresolved if it is eligible.
    pub identity_resolved: bool,
    /// True removes this attempt from every eligible-population count.
    pub excluded: bool,
}

/// An exact count ratio. Counts are preserved without fraction simplification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ratio {
    pub numerator: u64,
    pub denominator: u64,
}

/// Counts and yields for the scoped physical-attempt population.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YieldSummary {
    /// Equals `eligible + excluded`.
    pub scoped_attempts: u64,
    /// Equals `pass + fail + unknown`.
    pub eligible: u64,
    pub excluded: u64,
    pub pass: u64,
    pub fail: u64,
    pub unknown: u64,
    /// Eligible identity-unresolved attempts; overlaps the verdict categories.
    pub unresolved: u64,
    /// `pass / eligible`; absent exactly when there are no eligible attempts.
    pub all_attempts_yield: Option<Ratio>,
    /// `pass / (pass + fail)`; absent exactly when there are no known verdicts.
    pub known_verdict_yield: Option<Ratio>,
}

/// Constant-space PRR counter with checked, atomic-on-error updates.
///
/// Excluded attempts contribute only to `scoped_attempts` and `excluded`.
/// Identity resolution never changes eligibility or the recorded verdict.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct YieldAccumulator {
    scoped_attempts: u64,
    eligible: u64,
    excluded: u64,
    pass: u64,
    fail: u64,
    unknown: u64,
    unresolved: u64,
}

impl YieldAccumulator {
    /// Adds one already-scoped physical attempt, leaving all counts unchanged
    /// if any counter would overflow.
    pub fn add(&mut self, attempt: AttemptDisposition) -> io::Result<()> {
        let mut delta = Self {
            scoped_attempts: 1,
            ..Self::default()
        };
        if attempt.excluded {
            delta.excluded = 1;
        } else {
            delta.eligible = 1;
            delta.unresolved = u64::from(!attempt.identity_resolved);
            match attempt.verdict {
                Verdict::Pass => delta.pass = 1,
                Verdict::Fail => delta.fail = 1,
                Verdict::Unknown => delta.unknown = 1,
            }
        }
        self.merge(&delta)
    }

    /// Adds another accumulator without consuming or modifying it.
    ///
    /// The caller must ensure the populations should be counted together; this
    /// reducer does not detect repeated sources or repeated physical attempts.
    /// Overflow leaves this accumulator unchanged and does not poison it.
    pub fn merge(&mut self, other: &Self) -> io::Result<()> {
        let combined = Self {
            scoped_attempts: checked_count(self.scoped_attempts, other.scoped_attempts)?,
            eligible: checked_count(self.eligible, other.eligible)?,
            excluded: checked_count(self.excluded, other.excluded)?,
            pass: checked_count(self.pass, other.pass)?,
            fail: checked_count(self.fail, other.fail)?,
            unknown: checked_count(self.unknown, other.unknown)?,
            unresolved: checked_count(self.unresolved, other.unresolved)?,
        };
        *self = combined;
        Ok(())
    }

    /// Returns exact counts; zero denominators are represented by `None`.
    pub fn summary(&self) -> YieldSummary {
        // Private counts are changed only by invariant-preserving add/merge.
        // The known count cannot exceed the already-checked eligible count.
        let known = self
            .pass
            .checked_add(self.fail)
            .expect("known verdict count cannot exceed eligible attempts");
        let ratio = |denominator| {
            (denominator != 0).then_some(Ratio {
                numerator: self.pass,
                denominator,
            })
        };
        YieldSummary {
            scoped_attempts: self.scoped_attempts,
            eligible: self.eligible,
            excluded: self.excluded,
            pass: self.pass,
            fail: self.fail,
            unknown: self.unknown,
            unresolved: self.unresolved,
            all_attempts_yield: ratio(self.eligible),
            known_verdict_yield: ratio(known),
        }
    }
}

fn checked_count(left: u64, right: u64) -> io::Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "attempt yield count overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(verdict: Verdict, identity_resolved: bool, excluded: bool) -> AttemptDisposition {
        AttemptDisposition {
            verdict,
            identity_resolved,
            excluded,
        }
    }

    fn accumulate(attempts: &[AttemptDisposition]) -> YieldAccumulator {
        let mut accumulator = YieldAccumulator::default();
        for &attempt in attempts {
            accumulator.add(attempt).unwrap();
        }
        accumulator
    }

    #[test]
    fn prr_flags_decode_all_256_values_with_unknown_precedence() {
        use Verdict::{Fail as F, Pass as P, Unknown as U};
        // Hand-tabulated outcomes for low five bits 0 through 31. All eight
        // combinations of upper bits are irrelevant to the PRR verdict.
        let expected = [
            P, P, P, P, U, U, U, U, F, F, F, F, U, U, U, U, U, U, U, U, U, U, U, U, U, U, U, U, U,
            U, U, U,
        ];
        for high in 0u8..8 {
            for (low, &verdict) in expected.iter().enumerate() {
                let flags = (high << 5) | low as u8;
                assert_eq!(
                    Verdict::from_prr_flags(flags),
                    verdict,
                    "flags={flags:#04x}"
                );
            }
        }
    }

    #[test]
    fn empty_population_has_zero_counts_and_no_ratios() {
        assert_eq!(
            YieldAccumulator::default().summary(),
            YieldSummary {
                scoped_attempts: 0,
                eligible: 0,
                excluded: 0,
                pass: 0,
                fail: 0,
                unknown: 0,
                unresolved: 0,
                all_attempts_yield: None,
                known_verdict_yield: None,
            }
        );
    }

    #[test]
    fn exclusions_and_unresolved_identity_have_distinct_populations() {
        let summary = accumulate(&[
            attempt(Verdict::Pass, true, false),
            attempt(Verdict::Pass, false, false),
            attempt(Verdict::Fail, false, false),
            attempt(Verdict::Unknown, true, false),
            attempt(Verdict::Unknown, false, false),
            attempt(Verdict::Pass, false, true),
            attempt(Verdict::Fail, false, true),
            attempt(Verdict::Unknown, false, true),
        ])
        .summary();
        assert_eq!(
            summary,
            YieldSummary {
                scoped_attempts: 8,
                eligible: 5,
                excluded: 3,
                pass: 2,
                fail: 1,
                unknown: 2,
                unresolved: 3,
                all_attempts_yield: Some(Ratio {
                    numerator: 2,
                    denominator: 5,
                }),
                known_verdict_yield: Some(Ratio {
                    numerator: 2,
                    denominator: 3,
                }),
            }
        );
    }

    #[test]
    fn all_excluded_population_has_no_verdicts_unresolved_or_ratios() {
        let summary = accumulate(&[
            attempt(Verdict::Pass, false, true),
            attempt(Verdict::Fail, false, true),
            attempt(Verdict::Unknown, false, true),
        ])
        .summary();
        assert_eq!(summary.scoped_attempts, 3);
        assert_eq!(summary.excluded, 3);
        assert_eq!(summary.eligible, 0);
        assert_eq!((summary.pass, summary.fail, summary.unknown), (0, 0, 0));
        assert_eq!(summary.unresolved, 0);
        assert_eq!(summary.all_attempts_yield, None);
        assert_eq!(summary.known_verdict_yield, None);
    }

    #[test]
    fn unknown_only_population_has_zero_all_attempt_yield_and_no_known_ratio() {
        let summary = accumulate(&[
            attempt(Verdict::Unknown, false, false),
            attempt(Verdict::Unknown, true, false),
        ])
        .summary();
        assert_eq!(summary.eligible, 2);
        assert_eq!(summary.unknown, 2);
        assert_eq!(summary.unresolved, 1);
        assert_eq!(
            summary.all_attempts_yield,
            Some(Ratio {
                numerator: 0,
                denominator: 2,
            })
        );
        assert_eq!(summary.known_verdict_yield, None);
    }

    #[test]
    fn ratios_preserve_population_counts_without_reducing_fractions() {
        let mut accumulator = accumulate(&[
            attempt(Verdict::Pass, true, false),
            attempt(Verdict::Fail, true, false),
            attempt(Verdict::Unknown, true, false),
        ]);
        accumulator.merge(&accumulator.clone()).unwrap();
        let summary = accumulator.summary();
        assert_eq!(
            summary.all_attempts_yield,
            Some(Ratio {
                numerator: 2,
                denominator: 6,
            })
        );
        assert_eq!(
            summary.known_verdict_yield,
            Some(Ratio {
                numerator: 2,
                denominator: 4,
            })
        );
    }

    #[test]
    fn merge_matches_single_pass_and_is_commutative_and_associative() {
        let attempts = [
            attempt(Verdict::Pass, false, false),
            attempt(Verdict::Unknown, false, false),
            attempt(Verdict::Fail, true, false),
            attempt(Verdict::Pass, false, true),
            attempt(Verdict::Pass, true, false),
        ];
        let expected = accumulate(&attempts);
        let a = accumulate(&attempts[..2]);
        let b = accumulate(&attempts[2..4]);
        let c = accumulate(&attempts[4..]);
        let mut ab_c = a.clone();
        ab_c.merge(&b).unwrap();
        ab_c.merge(&c).unwrap();
        let mut bc = b.clone();
        bc.merge(&c).unwrap();
        let mut a_bc = a.clone();
        a_bc.merge(&bc).unwrap();
        let mut cba = c.clone();
        cba.merge(&b).unwrap();
        cba.merge(&a).unwrap();
        assert_eq!(ab_c, expected);
        assert_eq!(a_bc, expected);
        assert_eq!(cba, expected);
        assert_eq!(expected.summary().scoped_attempts, 5);
        assert_eq!(expected.summary().eligible, 4);
        assert_eq!(expected.summary().excluded, 1);
        assert_eq!(expected.summary().unresolved, 2);
        assert_eq!(
            (a.summary().scoped_attempts, b.summary().scoped_attempts),
            (2, 2)
        );
        assert_eq!(c.summary().scoped_attempts, 1);
        let mut with_empty = expected.clone();
        with_empty.merge(&YieldAccumulator::default()).unwrap();
        assert_eq!(with_empty, expected);
    }

    fn maximum_population(attempt: AttemptDisposition) -> YieldAccumulator {
        let mut accumulator = accumulate(&[attempt]);
        // n -> 2n + 1, starting at 1: after 63 rounds this is 2^64 - 1.
        for _ in 0..63 {
            accumulator.merge(&accumulator.clone()).unwrap();
            accumulator.add(attempt).unwrap();
        }
        assert_eq!(accumulator.summary().scoped_attempts, u64::MAX);
        accumulator
    }

    #[test]
    fn add_and_merge_overflow_are_atomic_and_do_not_poison_state() {
        for disposition in [
            attempt(Verdict::Pass, false, false),
            attempt(Verdict::Fail, false, false),
            attempt(Verdict::Unknown, false, false),
            attempt(Verdict::Pass, false, true),
        ] {
            let mut accumulator = maximum_population(disposition);
            let before = accumulator.clone();
            let summary_before = accumulator.summary();
            assert_eq!(
                accumulator.add(disposition).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert_eq!(accumulator, before);
            assert_eq!(accumulator.summary(), summary_before);

            let other = accumulate(&[attempt(Verdict::Pass, true, false)]);
            let other_before = other.clone();
            assert_eq!(
                accumulator.merge(&other).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert_eq!(accumulator, before);
            assert_eq!(other, other_before);

            accumulator.merge(&YieldAccumulator::default()).unwrap();
            assert_eq!(accumulator, before);
        }
    }
}
