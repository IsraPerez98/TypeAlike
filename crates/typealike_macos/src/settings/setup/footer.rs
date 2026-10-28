//! The footer of the setup window: the progress dots at the leading edge, then the back button and
//! the default button that goes on, at the trailing edge, below a separator.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSAccessibilityStaticTextRole, NSBox, NSBoxType, NSButton, NSColor, NSControlSize,
    NSLayoutConstraint, NSStackView, NSStackViewGravity, NSTitlePosition, NSUserInterfaceLayoutOrientation,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSEdgeInsets, NSString};
use typealike_core::setup::{SetupView, StepProgress};

use crate::settings::controls::{push_button, show_push_button};
use crate::settings::layout::{NO_INSETS, fill_width, separator, stack};
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
const CURRENT_DOT_WIDTH: CGFloat = 18.0;
const DOT_SPACING: CGFloat = 6.0;
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
        bar.addView_inGravity(&dots.stack, NSStackViewGravity::Leading);
        bar.addView_inGravity(&back, NSStackViewGravity::Trailing);
        bar.addView_inGravity(&next, NSStackViewGravity::Trailing);
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

    /// Shows the progress and the buttons of `view`. The back button hides on the first step.
    pub(super) fn render(&mut self, view: &SetupView, binder: &mut Binder<'_>) {
        self.dots.render(&view.progress, binder.main_thread);
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

/// A dot for each step. The dot of the step that shows is longer and has the accent color.
/// Assistive technology reads the dots as one text, such as "Step 2 of 5".
#[derive(Debug)]
struct ProgressDots {
    stack: Retained<NSStackView>,
    dots: Vec<Dot>,
}

impl ProgressDots {
    fn new(main_thread: MainThreadMarker) -> Self {
        let stack = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            DOT_SPACING,
            NO_INSETS,
            main_thread,
        );
        stack.setAccessibilityElement(true);
        // SAFETY: AppKit defines the role before any code runs.
        stack.setAccessibilityRole(Some(unsafe { NSAccessibilityStaticTextRole }));
        Self {
            stack,
            dots: Vec::new(),
        }
    }

    fn render(&mut self, progress: &StepProgress, main_thread: MainThreadMarker) {
        if self.dots.len() != progress.count {
            self.rebuild(progress.count, main_thread);
        }
        for (index, dot) in self.dots.iter().enumerate() {
            dot.show(dot_look(index, progress.number));
        }
        let text = NSString::from_str(&progress.text);
        // SAFETY: the value of a static text is a string.
        unsafe { self.stack.setAccessibilityValue(Some(&text)) };
    }

    fn rebuild(&mut self, count: usize, main_thread: MainThreadMarker) {
        for dot in self.dots.drain(..) {
            dot.shape.removeFromSuperview();
        }
        for _ in 0..count {
            let dot = Dot::new(main_thread);
            self.stack.addArrangedSubview(&dot.shape);
            self.dots.push(dot);
        }
    }
}

/// Whether a dot shows the step that the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DotLook {
    Current,
    Other,
}

/// The look of the dot at `index`, from 0, while the window shows the step with the `number`,
/// from 1.
fn dot_look(index: usize, number: usize) -> DotLook {
    if index.checked_add(1) == Some(number) {
        DotLook::Current
    } else {
        DotLook::Other
    }
}

/// One dot of the progress, with the constraint of its width.
#[derive(Debug)]
struct Dot {
    shape: Retained<NSBox>,
    width: Retained<NSLayoutConstraint>,
}

impl Dot {
    fn new(main_thread: MainThreadMarker) -> Self {
        let shape = NSBox::new(main_thread);
        shape.setBoxType(NSBoxType::Custom);
        shape.setTitlePosition(NSTitlePosition::NoTitle);
        shape.setBorderWidth(0.0);
        shape.setCornerRadius(DOT_SIZE / 2.0);
        shape.setAccessibilityElement(false);
        let width = shape.widthAnchor().constraintEqualToConstant(DOT_SIZE);
        NSLayoutConstraint::activateConstraints(&NSArray::from_slice(&[
            &*width,
            &*shape.heightAnchor().constraintEqualToConstant(DOT_SIZE),
        ]));
        Self { shape, width }
    }

    fn show(&self, look: DotLook) {
        let (fill, width) = match look {
            DotLook::Current => (NSColor::controlAccentColor(), CURRENT_DOT_WIDTH),
            DotLook::Other => (NSColor::quaternaryLabelColor(), DOT_SIZE),
        };
        self.shape.setFillColor(&fill);
        self.width.setConstant(width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_of_the_shown_step_is_current() {
        assert_eq!(dot_look(1, 2), DotLook::Current);
    }

    #[test]
    fn dot_of_another_step_is_not_current() {
        assert_eq!(dot_look(2, 2), DotLook::Other);
    }
}
