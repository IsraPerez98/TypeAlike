//! The footer of the setup window: the progress dots at the leading edge, then the back button and
//! the default button that goes on, at the trailing edge, below a separator. An accent pill marks
//! the dot of the step that shows, and slides to the dot of the next step.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSAccessibilityStaticTextRole, NSAnimatablePropertyContainer as _,
    NSAnimationContext, NSBox, NSBoxType, NSButton, NSColor, NSControlSize, NSLayoutConstraint, NSStackView,
    NSStackViewGravity, NSTitlePosition, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSEdgeInsets, NSString, NSTimeInterval};
use typealike_core::setup::{SetupView, StepProgress};

use crate::settings::controls::{push_button, show_push_button};
use crate::settings::layout::{NO_INSETS, fill_width, separator, stack};
use crate::settings::setup::motion::Motion;
use crate::settings::{ActionTarget, Binder};

const BAR_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 12.0,
    left: 20.0,
    bottom: 12.0,
    right: 20.0,
};
const BUTTON_SPACING: CGFloat = 12.0;
const BUTTON_MIN_WIDTH: CGFloat = 88.0;
const DOT_SIZE: CGFloat = 7.0;
const DOT_SPACING: CGFloat = 10.0;
const PILL_WIDTH: CGFloat = 19.0;
/// How far the pill reaches past each side of its dot.
const PILL_OVERHANG: CGFloat = (PILL_WIDTH - DOT_SIZE) / 2.0;
const PILL_SLIDE_SECONDS: NSTimeInterval = 0.3;
/// The key equivalent of the default button: the Return key.
const RETURN_KEY: &str = "\r";

/// The views of the footer, kept to update them on each render.
#[derive(Debug)]
pub(super) struct Footer {
    pub(super) root: Retained<NSStackView>,
    dots: ProgressDots,
    back: Retained<NSButton>,
    next: Retained<NSButton>,
}

impl Footer {
    /// Makes the footer, whose buttons send the press action of `target`.
    pub(super) fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let dots = ProgressDots::new(main_thread);
        let back = footer_button(target, main_thread);
        let next = footer_button(target, main_thread);
        next.setKeyEquivalent(&NSString::from_str(RETURN_KEY));
        let bar = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            BUTTON_SPACING,
            BAR_INSETS,
            main_thread,
        );
        bar.addView_inGravity(&dots.root, NSStackViewGravity::Leading);
        bar.addView_inGravity(&back, NSStackViewGravity::Trailing);
        bar.addView_inGravity(&next, NSStackViewGravity::Trailing);
        // The stack keeps its insets only at its hugging priority, so shorter dots make the bar height ambiguous.
        dots.root
            .heightAnchor()
            .constraintEqualToAnchor(&next.heightAnchor())
            .setActive(true);
        let divider = separator(main_thread);
        let root = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            0.0,
            NO_INSETS,
            main_thread,
        );
        root.addArrangedSubview(&divider);
        root.addArrangedSubview(&bar);
        fill_width(&divider, &root);
        fill_width(&bar, &root);
        Self {
            root,
            dots,
            back,
            next,
        }
    }

    /// Shows the progress and the buttons of `view`, with the `motion` of the system. The back
    /// button hides on the first step.
    pub(super) fn render(&mut self, view: &SetupView, motion: Motion, binder: &mut Binder<'_>) {
        self.dots.render(&view.progress, motion, binder.main_thread);
        match &view.back {
            Some(back) => show_push_button(&self.back, back, None, binder),
            None => self.back.setHidden(true),
        }
        show_push_button(&self.next, &view.next, None, binder);
    }
}

/// A large, hidden push button with a minimum width, which sends the press action of `target`.
fn footer_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
    let button = push_button(target, main_thread);
    button.setControlSize(NSControlSize::Large);
    button
        .widthAnchor()
        .constraintGreaterThanOrEqualToConstant(BUTTON_MIN_WIDTH)
        .setActive(true);
    button
}

/// A gray dot for each step, and an accent pill over the dot of the step that shows. When the step
/// changes, the pill slides to its new dot, unless the system reduces motion. Assistive technology
/// reads the dots as one text, such as "Step 2 of 5".
#[derive(Debug)]
struct ProgressDots {
    root: Retained<NSView>,
    stack: Retained<NSStackView>,
    dots: Vec<Retained<NSBox>>,
    pill_leading: Retained<NSLayoutConstraint>,
    /// The number of the step that the pill marks, from 1, after the first render.
    shown: Option<usize>,
}

impl ProgressDots {
    fn new(main_thread: MainThreadMarker) -> Self {
        let root = NSView::new(main_thread);
        root.setTranslatesAutoresizingMaskIntoConstraints(false);
        root.setAccessibilityElement(true);
        // SAFETY: AppKit defines the role before any code runs.
        root.setAccessibilityRole(Some(unsafe { NSAccessibilityStaticTextRole }));
        let insets = NSEdgeInsets {
            left: PILL_OVERHANG,
            right: PILL_OVERHANG,
            ..NO_INSETS
        };
        let stack = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            DOT_SPACING,
            insets,
            main_thread,
        );
        stack.setTranslatesAutoresizingMaskIntoConstraints(false);
        root.addSubview(&stack);
        let pill = rounded_shape(&NSColor::controlAccentColor(), main_thread);
        pill.setTranslatesAutoresizingMaskIntoConstraints(false);
        root.addSubview(&pill);
        let pill_leading = pill
            .leadingAnchor()
            .constraintEqualToAnchor(&root.leadingAnchor());
        NSLayoutConstraint::activateConstraints(&NSArray::from_slice(&[
            &*stack
                .leadingAnchor()
                .constraintEqualToAnchor(&root.leadingAnchor()),
            &*stack
                .trailingAnchor()
                .constraintEqualToAnchor(&root.trailingAnchor()),
            &*stack.topAnchor().constraintEqualToAnchor(&root.topAnchor()),
            &*stack.bottomAnchor().constraintEqualToAnchor(&root.bottomAnchor()),
            &*pill_leading,
            &*pill
                .centerYAnchor()
                .constraintEqualToAnchor(&root.centerYAnchor()),
            &*pill.widthAnchor().constraintEqualToConstant(PILL_WIDTH),
            &*pill.heightAnchor().constraintEqualToConstant(DOT_SIZE),
        ]));
        Self {
            root,
            stack,
            dots: Vec::new(),
            pill_leading,
            shown: None,
        }
    }

    fn render(&mut self, progress: &StepProgress, motion: Motion, main_thread: MainThreadMarker) {
        if self.dots.len() != progress.count {
            self.rebuild(progress.count, main_thread);
        }
        let offset = pill_offset(progress.number);
        match (self.shown, motion) {
            (Some(shown), Motion::Full) if shown != progress.number => self.slide_pill(offset),
            _ => self.pill_leading.setConstant(offset),
        }
        self.shown = Some(progress.number);
        let text = NSString::from_str(&progress.text);
        // SAFETY: the value of a static text is a string.
        unsafe { self.root.setAccessibilityValue(Some(&text)) };
    }

    fn slide_pill(&self, offset: CGFloat) {
        NSAnimationContext::beginGrouping();
        let context = NSAnimationContext::currentContext();
        context.setDuration(PILL_SLIDE_SECONDS);
        context.setAllowsImplicitAnimation(true);
        self.pill_leading.animator().setConstant(offset);
        NSAnimationContext::endGrouping();
    }

    fn rebuild(&mut self, count: usize, main_thread: MainThreadMarker) {
        for dot in self.dots.drain(..) {
            dot.removeFromSuperview();
        }
        for _ in 0..count {
            let dot = rounded_shape(&NSColor::quaternaryLabelColor(), main_thread);
            NSLayoutConstraint::activateConstraints(&NSArray::from_slice(&[
                &*dot.widthAnchor().constraintEqualToConstant(DOT_SIZE),
                &*dot.heightAnchor().constraintEqualToConstant(DOT_SIZE),
            ]));
            self.stack.addArrangedSubview(&dot);
            self.dots.push(dot);
        }
    }
}

/// The distance from the leading edge of the dots to the pill over the dot of the step with the
/// `number`, from 1. The pill reaches as far past its dot on each side.
fn pill_offset(number: usize) -> CGFloat {
    let steps_before = u32::try_from(number.saturating_sub(1)).unwrap_or(u32::MAX);
    CGFloat::from(steps_before) * (DOT_SIZE + DOT_SPACING)
}

/// A shape with fully rounded ends in `fill`, which assistive technology skips.
fn rounded_shape(fill: &NSColor, main_thread: MainThreadMarker) -> Retained<NSBox> {
    let shape = NSBox::new(main_thread);
    shape.setBoxType(NSBoxType::Custom);
    shape.setTitlePosition(NSTitlePosition::NoTitle);
    shape.setBorderWidth(0.0);
    shape.setCornerRadius(DOT_SIZE / 2.0);
    shape.setFillColor(fill);
    shape.setAccessibilityElement(false);
    shape
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pill_of_the_first_step_starts_at_the_leading_edge() {
        assert!(pill_offset(1).abs() < f64::EPSILON);
    }

    #[test]
    fn pill_moves_one_dot_and_one_space_for_each_step() {
        assert!((pill_offset(3) - 2.0 * (DOT_SIZE + DOT_SPACING)).abs() < f64::EPSILON);
    }
}
