//! The motion of the setup window when it shows another step. The new step fades in and slides in
//! from the side of the button that the user pressed. With Reduce Motion, it only fades in.

use std::cmp::Ordering;

use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWorkspace};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSNumber, NSString};
use objc2_quartz_core::{
    CABasicAnimation, CAMediaTiming as _, CAMediaTimingFunction, kCAMediaTimingFunctionEaseOut,
};
use typealike_core::setup::SetupStep;

const STEP_CHANGE_SECONDS: f64 = 0.3;
const SLIDE_DISTANCE: CGFloat = 24.0;
const SLIDE_KEY_PATH: &str = "transform.translation.x";
const SLIDE_ANIMATION_KEY: &str = "setupStepSlide";
const FADE_KEY_PATH: &str = "opacity";
const FADE_ANIMATION_KEY: &str = "setupStepFade";

/// Whether the user asked macOS to reduce motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Motion {
    Full,
    Reduced,
}

impl Motion {
    /// The motion that the accessibility settings of macOS allow now.
    pub(super) fn of_system() -> Self {
        if NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion() {
            Self::Reduced
        } else {
            Self::Full
        }
    }
}

/// How the content of the setup window changes to another step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StepTransition {
    /// The new step fades in.
    Fade,
    /// The new step fades in and slides in from an edge of the window.
    Slide(Edge),
}

/// An edge of the window, in the reading direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Edge {
    Leading,
    Trailing,
}

/// The transition from the `shown` step to `step`, or `None` when the window shows its first step
/// or the same step again. A later step comes from the trailing edge, an earlier step from the
/// leading edge.
pub(super) fn step_transition(
    shown: Option<SetupStep>,
    step: SetupStep,
    motion: Motion,
) -> Option<StepTransition> {
    let edge = match step.cmp(&shown?) {
        Ordering::Equal => return None,
        Ordering::Greater => Edge::Trailing,
        Ordering::Less => Edge::Leading,
    };
    Some(match motion {
        Motion::Full => StepTransition::Slide(edge),
        Motion::Reduced => StepTransition::Fade,
    })
}

/// The horizontal distance in points from where the content starts to slide in from `edge`.
fn slide_start(edge: Edge) -> CGFloat {
    match edge {
        Edge::Leading => -SLIDE_DISTANCE,
        Edge::Trailing => SLIDE_DISTANCE,
    }
}

/// Animates the entrance of the new step in the layer-backed `view`, whose changes come in the same
/// pass of the run loop. The step before goes at once, so the two steps never show together.
pub(super) fn animate(view: &NSView, transition: StepTransition) {
    let Some(layer) = view.layer() else {
        return;
    };
    let fade_in = step_animation(FADE_KEY_PATH, 0.0, 1.0);
    layer.addAnimation_forKey(&fade_in, Some(&NSString::from_str(FADE_ANIMATION_KEY)));
    if let StepTransition::Slide(edge) = transition {
        let slide = step_animation(SLIDE_KEY_PATH, slide_start(edge), 0.0);
        layer.addAnimation_forKey(&slide, Some(&NSString::from_str(SLIDE_ANIMATION_KEY)));
    }
}

/// An animation of the number at `key_path` from `start` to `end`, for the time of a step change.
fn step_animation(key_path: &str, start: f64, end: f64) -> Retained<CABasicAnimation> {
    let animation = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str(key_path)));
    let start = NSNumber::new_f64(start);
    let end = NSNumber::new_f64(end);
    // SAFETY: the opacity and a translation of the transform take a number.
    unsafe {
        animation.setFromValue(Some(&start));
        animation.setToValue(Some(&end));
    }
    animation.setDuration(STEP_CHANGE_SECONDS);
    animation.setTimingFunction(Some(&ease_out()));
    animation
}

fn ease_out() -> Retained<CAMediaTimingFunction> {
    // SAFETY: Core Animation defines the name before any code runs.
    CAMediaTimingFunction::functionWithName(unsafe { kCAMediaTimingFunctionEaseOut })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_step_of_the_window_shows_without_a_transition() {
        let transition = step_transition(None, SetupStep::Welcome, Motion::Full);

        assert_eq!(transition, None);
    }

    #[test]
    fn same_step_again_shows_without_a_transition() {
        let transition = step_transition(Some(SetupStep::Model), SetupStep::Model, Motion::Full);

        assert_eq!(transition, None);
    }

    #[test]
    fn next_step_slides_in_from_the_trailing_edge() {
        let transition = step_transition(Some(SetupStep::Sources), SetupStep::Permission, Motion::Full);

        assert_eq!(transition, Some(StepTransition::Slide(Edge::Trailing)));
    }

    #[test]
    fn step_before_slides_in_from_the_leading_edge() {
        let transition = step_transition(Some(SetupStep::Model), SetupStep::Sources, Motion::Full);

        assert_eq!(transition, Some(StepTransition::Slide(Edge::Leading)));
    }

    #[test]
    fn reduced_motion_only_fades_to_the_next_step() {
        let transition = step_transition(Some(SetupStep::Welcome), SetupStep::Model, Motion::Reduced);

        assert_eq!(transition, Some(StepTransition::Fade));
    }

    #[test]
    fn slide_from_the_trailing_edge_starts_to_the_right() {
        assert!(slide_start(Edge::Trailing) > 0.0);
    }

    #[test]
    fn slide_from_the_leading_edge_starts_to_the_left() {
        assert!(slide_start(Edge::Leading) < 0.0);
    }
}
