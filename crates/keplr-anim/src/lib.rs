//! The motion catalogue.
//!
//! Every duration and curve in the window comes from here rather than from a
//! literal in a view, so the theme owns how fast Keplr moves and one place
//! decides what "expressive" means: a spring for anything a finger moved, an
//! eased curve for anything that changed state, a stagger for a list appearing,
//! a shimmer for work still in progress.
//!
//! A host holds these like any other state, reads `value(now)` once per frame,
//! and tells the window how long to keep waking.

use std::time::{Duration, Instant};

use keplr_theme::MotionTokens;
use rcus::{Animated, Curve};

/// The durations and curves, resolved from a theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub fast: Duration,
    pub normal: Duration,
    pub slow: Duration,
    pub ease: Curve,
    pub spring: Curve,
    /// True when every duration is zero and every curve is the identity.
    pub reduced: bool,
}

impl Motion {
    pub fn from(tokens: &MotionTokens) -> Self {
        let ms = |value: u16| Duration::from_millis(value as u64);
        if tokens.reduced_motion {
            return Motion {
                fast: Duration::ZERO,
                normal: Duration::ZERO,
                slow: Duration::ZERO,
                ease: Curve::Ease([0.0, 0.0, 1.0, 1.0]),
                spring: Curve::Ease([0.0, 0.0, 1.0, 1.0]),
                reduced: true,
            };
        }
        Motion {
            fast: ms(tokens.fast_ms),
            normal: ms(tokens.normal_ms),
            slow: ms(tokens.slow_ms),
            ease: Curve::Ease(tokens.ease),
            spring: Curve::Spring {
                stiffness: tokens.spring.stiffness,
                damping: tokens.spring.damping,
            },
            reduced: false,
        }
    }

    /// When a value animated at this speed will have arrived.
    pub fn ends_at(&self, started: Instant, normal: bool) -> Instant {
        started + if normal { self.normal } else { self.fast }
    }
}

impl Default for Motion {
    fn default() -> Self {
        Motion::from(&MotionTokens {
            fast_ms: 120,
            normal_ms: 180,
            slow_ms: 260,
            ease: [0.2, 0.0, 0.0, 1.0],
            spring: SpringTokens {
                stiffness: 220.0,
                damping: 26.0,
            },
            reduced_motion: false,
        })
    }
}

/// A rule under an activity bar item or a tab, which slides to wherever the
/// selection now is instead of jumping.
#[derive(Clone, Copy, Debug)]
pub struct Indicator {
    x: Animated<f32>,
    width: Animated<f32>,
    visible: bool,
}

impl Indicator {
    pub fn new(at: (f32, f32), now: Instant) -> Self {
        Indicator {
            x: Animated::new(
                at.0,
                at.0,
                now,
                Duration::ZERO,
                Curve::Ease([0.0, 0.0, 1.0, 1.0]),
            ),
            width: Animated::new(
                at.1,
                at.1,
                now,
                Duration::ZERO,
                Curve::Ease([0.0, 0.0, 1.0, 1.0]),
            ),
            visible: true,
        }
    }

    /// Moves the rule, easing when it has room to.
    pub fn move_to(&mut self, at: (f32, f32), now: Instant, motion: &Motion) {
        if (at.0 - self.x.target()).abs() < f32::EPSILON
            && (at.1 - self.width.target()).abs() < f32::EPSILON
        {
            return;
        }
        self.x = Animated::new(self.x.value(), at.0, now, motion.normal, motion.ease);
        self.width = Animated::new(self.width.value(), at.1, now, motion.normal, motion.ease);
    }

    /// Recomputes the rule's position for this frame.
    pub fn sample(&mut self, now: Instant) -> (f32, f32) {
        (self.x.value_at(now), self.width.value_at(now))
    }

    /// When the rule has stopped moving.
    pub fn ends_at(&self) -> Instant {
        self.x.ends_at().max(self.width.ends_at())
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

/// A panel's open or closed progress, from 0 to 1.
#[derive(Clone, Copy, Debug)]
pub struct Panel {
    progress: Animated<f32>,
    open: bool,
}

impl Panel {
    pub fn new(open: bool, now: Instant, motion: &Motion) -> Self {
        let from = if open { 0.0 } else { 0.0 };
        let to = if open { 1.0 } else { 0.0 };
        Panel {
            progress: Animated::new(from, to, now, motion.normal, motion.ease),
            open,
        }
    }

    pub fn set_open(&mut self, open: bool, now: Instant, motion: &Motion) {
        if open == self.open {
            return;
        }
        self.progress = Animated::new(
            self.progress.value(),
            if open { 1.0 } else { 0.0 },
            now,
            motion.normal,
            motion.ease,
        );
        self.open = open;
    }

    pub fn value(&mut self, now: Instant) -> f32 {
        self.progress.value_at(now)
    }

    pub fn ends_at(&self) -> Instant {
        self.progress.ends_at()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

/// A list that has just appeared, so its rows arrive one after another instead of
/// all at once.
///
/// The cap matters: a file tree with four hundred entries would otherwise spend
/// three seconds arriving, which reads as lag rather than as motion.
pub fn stagger_delay(index: usize, cap: usize) -> Duration {
    Duration::from_millis(index.min(cap) as u64 * 8)
}

/// Where a looping shimmer is in its cycle, 0 to 1. The only thing in the window
/// allowed to loop.
pub fn shimmer_phase(now: Instant, period: Duration) -> f32 {
    let period = period.as_secs_f32().max(0.001);
    (now.elapsed().as_secs_f32() % period) / period
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Instant {
        Instant::now() + Duration::from_millis(value)
    }

    fn tokens(reduced: bool) -> MotionTokens {
        MotionTokens {
            fast_ms: 120,
            normal_ms: 180,
            slow_ms: 260,
            ease: [0.2, 0.0, 0.0, 1.0],
            spring: SpringTokens {
                stiffness: 220.0,
                damping: 26.0,
            },
            reduced_motion: reduced,
        }
    }

    #[test]
    fn the_theme_owns_the_durations() {
        let motion = Motion::from(&tokens(false));
        assert_eq!(motion.fast, Duration::from_millis(120));
        assert_eq!(motion.normal, Duration::from_millis(180));
        assert_eq!(motion.slow, Duration::from_millis(260));
        assert!(!motion.reduced);
    }

    #[test]
    fn reduced_motion_zeroes_every_duration_but_keeps_the_curve_valid() {
        let motion = Motion::from(&tokens(true));
        assert_eq!(motion.fast, Duration::ZERO);
        assert_eq!(motion.normal, Duration::ZERO);
        assert_eq!(motion.slow, Duration::ZERO);
        assert!(motion.reduced);
        assert_eq!(
            motion.ease,
            Curve::Ease([0.0, 0.0, 1.0, 1.0]),
            "not a degenerate curve"
        );
    }

    #[test]
    fn an_indicator_is_where_it_was_put_until_it_is_moved() {
        let mut indicator = Indicator::new((10.0, 48.0), Instant::now());
        let motion = Motion::from(&tokens(false));
        indicator.move_to((10.0, 48.0), Instant::now(), &motion);
        assert_eq!(indicator.sample(Instant::now()), (10.0, 48.0));
        assert_eq!(
            indicator.ends_at(),
            Instant::now() + Duration::ZERO,
            "no travel, no waiting"
        );
    }

    #[test]
    fn an_indicator_is_between_two_places_while_it_moves() {
        let mut indicator = Indicator::new((0.0, 48.0), Instant::now());
        let motion = Motion::from(&tokens(false));
        let now = Instant::now();
        indicator.move_to((100.0, 48.0), now, &motion);
        let half = ms(90);
        let (x, width) = indicator.sample(half);
        assert!(x > 0.0 && x < 100.0, "halfway is between, got {x}");
        assert_eq!(width, 48.0, "and its width has not changed");
        assert_eq!(indicator.sample(ms(400)).0, 100.0, "it arrives");
    }

    #[test]
    fn an_indicator_with_reduced_motion_is_never_between() {
        let mut indicator = Indicator::new((0.0, 48.0), Instant::now());
        let motion = Motion::from(&tokens(true));
        let now = Instant::now();
        indicator.move_to((100.0, 48.0), now, &motion);
        assert_eq!(indicator.sample(ms(1)).0, 100.0, "it arrives at once");
    }

    #[test]
    fn a_panel_is_fully_open_or_fully_shut() {
        let now = Instant::now();
        let motion = Motion::from(&tokens(false));
        let mut panel = Panel::new(true, now, &motion);
        assert_eq!(panel.value(ms(400)), 1.0);
        panel.set_open(false, ms(400), &motion);
        assert_eq!(panel.value(ms(800)), 0.0);
        assert!(!panel.is_open());
    }

    #[test]
    fn a_panel_reduced_motion_jumps_between_its_end_states() {
        let now = Instant::now();
        let motion = Motion::from(&tokens(true));
        let mut panel = Panel::new(true, now, &motion);
        assert_eq!(panel.value(ms(1)), 1.0);
        panel.set_open(false, ms(1), &motion);
        assert_eq!(panel.value(ms(2)), 0.0, "no halfway");
    }

    #[test]
    fn a_stagger_stops_growing_after_its_cap() {
        assert_eq!(stagger_delay(3, 12), Duration::from_millis(24));
        assert_eq!(stagger_delay(12, 12), stagger_delay(400, 12));
        assert_eq!(
            stagger_delay(0, 12),
            Duration::ZERO,
            "the first row is not delayed"
        );
    }

    #[test]
    fn the_shimmer_is_a_position_in_its_cycle() {
        let period = Duration::from_millis(1400);
        for _ in 0..50 {
            let phase = shimmer_phase(Instant::now(), period);
            assert!((0.0..1.0).contains(&phase), "got {phase}");
        }
        assert_eq!(
            shimmer_phase(Instant::now(), Duration::ZERO) >= 0.0,
            true,
            "a zero period still returns a position"
        );
    }
}
