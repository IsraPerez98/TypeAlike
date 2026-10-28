//! The footer of the setup window: the progress dots at the leading edge, then the back button and
//! the default button that goes on, at the trailing edge, below a separator. An accent pill marks
//! the step that shows, and slides to the next step while the dots make room for it.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSAccessibilityStaticTextRole, NSAnimationContext, NSBox, NSBoxType, NSButton,
    NSColor, NSControlSize, NSLayoutConstraint, NSStackView, NSStackViewGravity, NSTitlePosition,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSEdgeInsets, NSSize, NSString, NSTimeInterval};
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
        match &view.back {
            Some(back) => show_push_button(&self.back, back, None, binder),
            None => self.back.setHidden(true),
        }
        show_push_button(&self.next, &view.next, None, binder);
        self.dots.render(&view.progress, motion, binder.main_thread);
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

/// A gray dot for each step. The slot of the step that shows is as wide as a pill, and an accent
/// pill covers it, so the gaps between the dots stay the same. When the step changes, the pill
/// slides to the new slot while that slot grows and the old one shrinks, unless the system reduces
/// motion. Assistive technology reads the dots as one text, such as "Step 2 of 5".
#[derive(Debug)]
struct ProgressDots {
    root: Retained<NSView>,
    stack: Retained<NSStackView>,
    pill: Retained<NSBox>,
    slots: Vec<Slot>,
    /// The number of the step that the pill marks, from 1, after the first render.
    shown: Option<usize>,
}

/// The gray dot of a step, with the constraints that change when its step shows.
#[derive(Debug)]
struct Slot {
    dot: Retained<NSBox>,
    width: Retained<NSLayoutConstraint>,
    /// Puts the pill over the dot, active only while the step of the dot shows.
    pill_leading: Retained<NSLayoutConstraint>,
}

impl ProgressDots {
    fn new(main_thread: MainThreadMarker) -> Self {
        let root = NSView::new(main_thread);
        root.setTranslatesAutoresizingMaskIntoConstraints(false);
        root.setAccessibilityElement(true);
        // SAFETY: AppKit defines the role before any code runs.
        root.setAccessibilityRole(Some(unsafe { NSAccessibilityStaticTextRole }));
        let stack = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            DOT_SPACING,
            NO_INSETS,
            main_thread,
        );
        stack.setTranslatesAutoresizingMaskIntoConstraints(false);
        root.addSubview(&stack);
        let pill = rounded_shape(&NSColor::controlAccentColor(), main_thread);
        pill.setTranslatesAutoresizingMaskIntoConstraints(false);
        root.addSubview(&pill);
        NSLayoutConstraint::activateConstraints(&NSArray::from_slice(&[
            &*stack
                .leadingAnchor()
                .constraintEqualToAnchor(&root.leadingAnchor()),
            &*stack
                .trailingAnchor()
                .constraintEqualToAnchor(&root.trailingAnchor()),
            &*stack.topAnchor().constraintEqualToAnchor(&root.topAnchor()),
            &*stack.bottomAnchor().constraintEqualToAnchor(&root.bottomAnchor()),
            &*pill
                .centerYAnchor()
                .constraintEqualToAnchor(&root.centerYAnchor()),
            &*pill.widthAnchor().constraintEqualToConstant(PILL_WIDTH),
            &*pill.heightAnchor().constraintEqualToConstant(DOT_SIZE),
        ]));
        Self {
            root,
            stack,
            pill,
            slots: Vec::new(),
            shown: None,
        }
    }

    fn render(&mut self, progress: &StepProgress, motion: Motion, main_thread: MainThreadMarker) {
        if self.slots.len() != progress.count {
            self.rebuild(progress.count, main_thread);
            self.shown = None;
        }
        if self.shown != Some(progress.number) {
            match (self.shown, motion) {
                (Some(_), Motion::Full) => self.slide_pill_to(progress.number),
                _ => self.mark(progress.number),
            }
            self.shown = Some(progress.number);
        }
        let text = NSString::from_str(&progress.text);
        // SAFETY: the value of a static text is a string.
        unsafe { self.root.setAccessibilityValue(Some(&text)) };
    }

    /// Animates the change to the step with the `number`. The other changes of the window apply
    /// first, so that only the dots and the pill move.
    fn slide_pill_to(&self, number: usize) {
        if let Some(window) = self.root.window() {
            window.layoutIfNeeded();
        }
        NSAnimationContext::beginGrouping();
        let context = NSAnimationContext::currentContext();
        context.setDuration(PILL_SLIDE_SECONDS);
        context.setAllowsImplicitAnimation(true);
        self.mark(number);
        self.root.layoutSubtreeIfNeeded();
        NSAnimationContext::endGrouping();
    }

    /// Widens the slot of the step with the `number` and puts the pill over it.
    fn mark(&self, number: usize) {
        for slot in &self.slots {
            slot.pill_leading.setActive(false);
        }
        for (step, slot) in (1..).zip(&self.slots) {
            slot.width.setConstant(slot_width(step, number));
        }
        if let Some(slot) = number.checked_sub(1).and_then(|index| self.slots.get(index)) {
            slot.pill_leading.setActive(true);
        }
    }

    fn rebuild(&mut self, count: usize, main_thread: MainThreadMarker) {
        for slot in self.slots.drain(..) {
            slot.pill_leading.setActive(false);
            slot.dot.removeFromSuperview();
        }
        for _ in 0..count {
            let dot = rounded_shape(&NSColor::quaternaryLabelColor(), main_thread);
            let width = dot.widthAnchor().constraintEqualToConstant(DOT_SIZE);
            NSLayoutConstraint::activateConstraints(&NSArray::from_slice(&[
                &*width,
                &*dot.heightAnchor().constraintEqualToConstant(DOT_SIZE),
            ]));
            self.stack.addArrangedSubview(&dot);
            let pill_leading = self
                .pill
                .leadingAnchor()
                .constraintEqualToAnchor(&dot.leadingAnchor());
            self.slots.push(Slot {
                dot,
                width,
                pill_leading,
            });
        }
    }
}

/// The width of the slot of the step with the `number` while the step with the `shown` number
/// shows: the width of the pill for the step that shows, else the width of a dot.
fn slot_width(number: usize, shown: usize) -> CGFloat {
    if number == shown { PILL_WIDTH } else { DOT_SIZE }
}

/// A shape with fully rounded ends in `fill`, which assistive technology skips.
fn rounded_shape(fill: &NSColor, main_thread: MainThreadMarker) -> Retained<NSBox> {
    let shape = NSBox::new(main_thread);
    shape.setBoxType(NSBoxType::Custom);
    shape.setTitlePosition(NSTitlePosition::NoTitle);
    shape.setBorderWidth(0.0);
    // With margins, the content view of a slot that shrinks gets no width and keeps the box wide.
    shape.setContentViewMargins(NSSize::ZERO);
    shape.setCornerRadius(DOT_SIZE / 2.0);
    shape.setFillColor(fill);
    shape.setAccessibilityElement(false);
    shape
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_slot_of_the_shown_step_is_as_wide_as_the_pill() {
        let widths = [1, 2, 3].map(|number| slot_width(number, 2));

        assert_eq!(widths, [DOT_SIZE, PILL_WIDTH, DOT_SIZE]);
    }
}
