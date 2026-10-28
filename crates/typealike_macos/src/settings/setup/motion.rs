//! The motion of the setup window when it shows another step. The new step fades in and slides in
//! from the side of the button that the user pressed. With Reduce Motion, it only fades in.

use std::cmp::Ordering;

use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWorkspace};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSNumber, NSString};
use objc2_quartz_core::{
    CABasicAnimation, CAMediaTiming as _, CAMediaTimingFunction, CATransition, kCAMediaTimingFunctionEaseOut,
    kCATransition, kCATransitionFade,
};
use typealike_core::setup::SetupStep;

const STEP_CHANGE_SECONDS: f64 = 0.3;
const SLIDE_DISTANCE: CGFloat = 24.0;
const SLIDE_KEY_PATH: &str = "transform.translation.x";
const SLIDE_ANIMATION_KEY: &str = "setupStepSlide";

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
    let edge = match position(step).cmp(&position(shown?)) {
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

fn position(step: SetupStep) -> u8 {
    match step {
        SetupStep::Welcome => 0,
        SetupStep::Model => 1,
        SetupStep::Permission => 2,
        SetupStep::About => 3,
        SetupStep::Ready => 4,
    }
}

/// Animates the changes of the layer-backed `view` in the same pass of the run loop with
/// `transition`.
pub(super) fn animate(view: &NSView, transition: StepTransition) {
    let Some(layer) = view.layer() else {
        return;
    };
    // SAFETY: Core Animation defines the key before any code runs.
    layer.addAnimation_forKey(&crossfade(), Some(unsafe { kCATransition }));
    if let StepTransition::Slide(edge) = transition {
        layer.addAnimation_forKey(&slide(edge), Some(&NSString::from_str(SLIDE_ANIMATION_KEY)));
    }
}

fn crossfade() -> Retained<CATransition> {
    let fade = CATransition::animation();
    // SAFETY: Core Animation defines the transition type before any code runs.
    fade.setType(unsafe { kCATransitionFade });
    fade.setDuration(STEP_CHANGE_SECONDS);
    fade.setTimingFunction(Some(&ease_out()));
    fade
}

fn slide(edge: Edge) -> Retained<CABasicAnimation> {
    let slide = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str(SLIDE_KEY_PATH)));
    let start = NSNumber::new_f64(slide_start(edge));
    let end = NSNumber::new_f64(0.0);
    // SAFETY: a translation of the transform takes a number.
    unsafe {
        slide.setFromValue(Some(&start));
        slide.setToValue(Some(&end));
    }
    slide.setDuration(STEP_CHANGE_SECONDS);
    slide.setTimingFunction(Some(&ease_out()));
    slide
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
        let transition = step_transition(Some(SetupStep::Model), SetupStep::Permission, Motion::Full);

        assert_eq!(transition, Some(StepTransition::Slide(Edge::Trailing)));
    }

    #[test]
    fn step_before_slides_in_from_the_leading_edge() {
        let transition = step_transition(Some(SetupStep::About), SetupStep::Model, Motion::Full);

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
